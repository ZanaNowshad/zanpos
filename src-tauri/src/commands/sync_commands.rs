use crate::commands::rbac;
use crate::db::repositories::{ai_admin_repo, sync_repo};
use crate::errors::{AppError, AppResult};
use crate::sync::central_schema::CENTRAL_SCHEMA_SQL;
use crate::sync::supabase_client::{extract_project_ref, SupabaseClient};
use crate::AppState;
use serde::Serialize;
use sqlx::Row;
use tauri::State;

/// Resolve the active device_id from the database at runtime.
async fn active_device_id(state: &AppState) -> AppResult<String> {
    let row = sqlx::query(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No active device configured".into()))?;
    Ok(row.get("device_id"))
}

// ── sync_status ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn sync_status(state: State<'_, AppState>) -> Result<sync_repo::SyncStatus, AppError> {
    let worker_state = state.sync_worker.state.lock().await;
    let online = worker_state.online;
    let last_error = worker_state.last_error.clone();
    drop(worker_state);

    let device_id = active_device_id(&state).await?;
    let mut status = sync_repo::get_sync_status(&state.db, &device_id).await?;
    status.online = online;
    // If worker has a more specific error, surface it
    if last_error.is_some() {
        status.last_error = last_error;
    }
    Ok(status)
}

// ── sync_trigger_now ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn sync_trigger_now(state: State<'_, AppState>) -> Result<String, AppError> {
    state.sync_worker.run_once().await;
    let worker_state = state.sync_worker.state.lock().await;
    if worker_state.online {
        Ok("Sync completed successfully".to_string())
    } else if let Some(ref e) = worker_state.last_error {
        Ok(format!("Sync attempted — last error: {e}"))
    } else {
        Ok("Sync not configured — enter Supabase credentials in settings".to_string())
    }
}

// ── sync_force_full_resync ────────────────────────────────────────────────────

/// Recovery action: re-enqueue this device's ENTIRE catalog (branch, users,
/// products, categories, tax rules, prices, device) to the outbox, clear any
/// exhausted/failed queue rows, reset the one-time bootstrap guard, and push
/// immediately. Use this when a terminal's data never reached the cloud (e.g. the
/// schema was added after setup, or the key was briefly unreadable) so the outbox
/// looks "done" but the cloud is empty. Idempotent — the central RPC dedups by
/// idempotency key, so re-running it is always safe.
#[tauri::command]
pub async fn sync_force_full_resync(
    state: State<'_, AppState>,
    actor_user_id: String,
) -> Result<String, AppError> {
    crate::commands::rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    // Clear failed/exhausted rows so they don't shadow the fresh enqueue, and let
    // the bootstrap self-heal run again.
    let _ = sqlx::query("DELETE FROM sync_queue WHERE status IN ('failed', 'conflict')")
        .execute(&state.db)
        .await;
    let _ = sqlx::query(
        "UPDATE app_config SET value = '0' WHERE key IN ('sync_bootstrap_v1','sync_bootstrap_v2','sync_bootstrap_v3')",
    )
    .execute(&state.db)
    .await;

    // Re-enqueue the full catalog (branch is pushed via upsert_branch on next cycle;
    // everything else via the outbox → apply_sync_event RPC).
    crate::sync::outbox::enqueue_full_catalog(&state.db).await?;

    // Push + pull immediately.
    state.sync_worker.run_once().await;

    let queued: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sync_queue WHERE status IN ('pending','failed')")
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);
    let worker_state = state.sync_worker.state.lock().await;
    if worker_state.online {
        Ok(format!(
            "Full re-sync started. Catalog re-queued and pushing now ({queued} events remaining)."
        ))
    } else if let Some(ref e) = worker_state.last_error {
        Ok(format!(
            "Re-sync queued {queued} events but the push reported: {e}"
        ))
    } else {
        Ok(format!("Re-sync queued {queued} events."))
    }
}

// ── admin_setup_supabase ──────────────────────────────────────────────────────

/// Called from the setup wizard. Validates credentials, runs the central schema
/// migration via the Supabase Management API, then stores url + service key.
/// The PAT is never stored — used only for this one migration call.
#[tauri::command]
pub async fn admin_setup_supabase(
    state: State<'_, AppState>,
    url: String,
    service_key: String,
    pat: String,
) -> Result<(), AppError> {
    let url = url.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        return Err(AppError::Validation("Supabase URL is required".into()));
    }
    if service_key.is_empty() {
        return Err(AppError::Validation("Service role key is required".into()));
    }
    if pat.is_empty() {
        return Err(AppError::Validation(
            "Personal access token is required".into(),
        ));
    }

    let client = SupabaseClient::new(url.clone(), service_key.clone());

    // Step 1: Validate service role key
    client.validate().await.map_err(|_| {
        AppError::Validation(
            "Could not connect to Supabase — check the URL and service role key".into(),
        )
    })?;

    // Step 2: Extract project ref and run migration
    let project_ref = extract_project_ref(&url).ok_or_else(|| {
        AppError::Validation(
            "Invalid Supabase URL format (expected https://xyz.supabase.co)".into(),
        )
    })?;

    client
        .migrate(&pat, &project_ref, CENTRAL_SCHEMA_SQL)
        .await
        .map_err(|e| AppError::Validation(format!("Schema migration failed: {e}")))?;

    // Step 3: Persist config (PAT is intentionally NOT stored)
    ai_admin_repo::set_config(&state.db, "supabase_url", &url).await?;
    // Write service key to OS credential store. If it fails, keep the plaintext
    // fallback — never clear the only readable copy (prevents silent sync death).
    if crate::secure_store::set_secret("supabase_service_key", &service_key) {
        let _ = ai_admin_repo::set_config(&state.db, "supabase_service_key", "").await;
    } else {
        tracing::error!("CRITICAL: Failed to write service key to OS credential store. Keeping plaintext fallback to prevent sync death.");
    }

    // Record that the central schema has been migrated — this flag is checked by
    // setup_wizard_complete to prevent New-Store from completing without schema.
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('schema_migrated','1',?)
         ON CONFLICT(key) DO UPDATE SET value='1', updated_at=excluded.updated_at",
    )
    .bind(&now)
    .execute(&state.db)
    .await
    .ok();

    tracing::info!("Supabase configured: {url}");

    // CRITICAL (multi-terminal): enqueue the FULL current catalog — products,
    // categories, tax rules, the real owner + all users, prices, customers, and this
    // device — at the moment Supabase is first configured. The old boot-time
    // bootstrap ran before setup/Supabase existed and marked itself done, so the
    // outbox would otherwise be empty here and a joining terminal would see no users
    // and no devices. Then push immediately rather than waiting for the 30s tick.
    let _ = crate::sync::outbox::enqueue_full_catalog(&state.db).await;
    // Non-blocking: the UI must not block on network calls (offline-first principle).
    let worker = state.sync_worker.clone();
    tauri::async_runtime::spawn(async move {
        let _ = worker.run_once().await;
    });

    Ok(())
}

// ── admin_setup_supabase_creds_only ──────────────────────────────────────────

/// Store Supabase URL + service key without running schema migration.
/// Used when the setup wizard user skips the PAT step — schema migration
/// can be run later from Back Office → Sync.
#[tauri::command]
pub async fn admin_setup_supabase_creds_only(
    state: State<'_, AppState>,
    actor_user_id: String,
    url: String,
    service_key: String,
) -> Result<(), AppError> {
    rbac::owner_only(&state.db, &actor_user_id).await?;
    let url = url.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        return Err(AppError::Validation("Supabase URL is required".into()));
    }
    if service_key.is_empty() {
        return Err(AppError::Validation("Service role key is required".into()));
    }

    let client = SupabaseClient::new(url.clone(), service_key.clone());
    // Validate connection (no schema migration)
    client.validate().await.map_err(|_| {
        AppError::Validation(
            "Could not connect to Supabase — check the URL and service role key".into(),
        )
    })?;

    ai_admin_repo::set_config(&state.db, "supabase_url", &url).await?;
    if crate::secure_store::set_secret("supabase_service_key", &service_key) {
        let _ = ai_admin_repo::set_config(&state.db, "supabase_service_key", "").await;
    } else {
        tracing::error!("CRITICAL: Failed to write service key to OS credential store. Keeping plaintext fallback to prevent sync death.");
    }

    tracing::info!("Supabase credentials stored (no schema migration): {url}");

    // Multi-terminal: enqueue full catalog + sync now so a joining terminal sees
    // users/devices even when the schema migration step was skipped.
    let _ = crate::sync::outbox::enqueue_full_catalog(&state.db).await;
    // Non-blocking: the UI must not block on network calls (offline-first principle).
    let worker = state.sync_worker.clone();
    tauri::async_runtime::spawn(async move {
        let _ = worker.run_once().await;
    });

    Ok(())
}

// ── sync_queue_list ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SyncQueueItem {
    pub sync_event_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub operation: String,
    pub status: String,
    pub attempt_count: i64,
    pub last_attempt_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
}

#[tauri::command]
pub async fn sync_queue_list(state: State<'_, AppState>) -> Result<Vec<SyncQueueItem>, AppError> {
    let rows = sqlx::query(
        "SELECT sync_event_id, entity_type, entity_id, operation, status,
                attempt_count, last_attempt_at, last_error, created_at
         FROM sync_queue
         WHERE status IN ('pending', 'failed', 'conflict')
         ORDER BY created_at DESC
         LIMIT 200",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(rows
        .iter()
        .map(|r| SyncQueueItem {
            sync_event_id: r.get("sync_event_id"),
            entity_type: r.get("entity_type"),
            entity_id: r.get("entity_id"),
            operation: r.get("operation"),
            status: r.get("status"),
            attempt_count: r.get("attempt_count"),
            last_attempt_at: r.try_get("last_attempt_at").unwrap_or(None),
            last_error: r.try_get("last_error").unwrap_or(None),
            created_at: r.get("created_at"),
        })
        .collect())
}

// ── sync_queue_retry ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn sync_queue_retry(
    sync_event_id: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    // Reset attempt_count too — otherwise an event that already hit MAX_ATTEMPTS
    // stays excluded by the worker's `attempt_count < MAX_ATTEMPTS` push filter and
    // "Retry" silently does nothing. Clearing it gives the event a fresh 10 attempts.
    let rows_affected = sqlx::query(
        "UPDATE sync_queue SET status = 'pending', last_error = NULL, attempt_count = 0
         WHERE sync_event_id = ? AND status IN ('failed', 'conflict')",
    )
    .bind(&sync_event_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if rows_affected == 0 {
        return Err(AppError::NotFound(format!(
            "Sync event {sync_event_id} not found or not in a retryable state"
        )));
    }
    Ok(())
}

// ── sync_queue_dismiss ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn sync_queue_dismiss(
    sync_event_id: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let rows_affected = sqlx::query("DELETE FROM sync_queue WHERE sync_event_id = ?")
        .bind(&sync_event_id)
        .execute(&state.db)
        .await?
        .rows_affected();

    if rows_affected == 0 {
        return Err(AppError::NotFound(format!(
            "Sync event {sync_event_id} not found"
        )));
    }
    Ok(())
}

// ── admin_get_supabase_status ─────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SupabaseStatus {
    pub configured: bool,
    /// Public Supabase project URL, safe to surface in the UI for pre-filling
    /// the "change connection" form. The service role key is NEVER returned —
    /// it lives only in the OS credential store.
    pub url: String,
}

#[tauri::command]
pub async fn admin_get_supabase_status(
    state: State<'_, AppState>,
) -> Result<SupabaseStatus, AppError> {
    let url = ai_admin_repo::get_config(&state.db, "supabase_url")
        .await?
        .unwrap_or_default();
    // Two-phase key check — same as load_client in worker.rs
    let key = {
        let from_os = crate::secure_store::get_secret("supabase_service_key")
            .unwrap_or_default();
        if !from_os.is_empty() {
            from_os
        } else {
            ai_admin_repo::get_config(&state.db, "supabase_service_key")
                .await
                .unwrap_or_default()
                .unwrap_or_default()
        }
    };
    Ok(SupabaseStatus {
        configured: !url.is_empty() && !key.is_empty(),
        url,
    })
}
