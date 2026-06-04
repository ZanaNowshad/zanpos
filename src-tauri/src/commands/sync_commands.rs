use crate::commands::rbac;
use crate::db::repositories::ai_admin_repo;
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

/// Tables that participate in sync (in FK-safe push order).
const SYNC_TABLES: &[&str] = &[
    "categories", "tax_rules", "products", "devices", "users", "customers",
    "shifts", "sales", "sale_items", "payments", "refunds", "refund_items",
    "stock_movements", "audit_logs", "delivery_orders", "product_prices",
];

/// Count pending rows across all syncable tables.
async fn count_pending(pool: &sqlx::SqlitePool) -> AppResult<i64> {
    let mut total: i64 = 0;
    for table in SYNC_TABLES {
        let sql = format!(
            "SELECT COUNT(*) FROM {} WHERE sync_status = 'pending'",
            table
        );
        let n: i64 = sqlx::query_scalar(&sql).fetch_one(pool).await.unwrap_or(0);
        total += n;
    }
    Ok(total)
}

// ── sync_status ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn sync_status(state: State<'_, AppState>) -> Result<serde_json::Value, AppError> {
    let worker_state = state.sync_worker.state.lock().await;
    let online = worker_state.online;
    let last_error = worker_state.last_error.clone();
    drop(worker_state);

    let device_id = active_device_id(&state).await?;
    let pending = count_pending(&state.db).await.unwrap_or(0);

    // Read last successful sync from watermark table
    let last_sync: Option<String> = sqlx::query_scalar(
        "SELECT last_pushed_at FROM sync_watermark WHERE table_name = 'sales'",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    Ok(serde_json::json!({
        "online": online,
        "supabase_configured": false,  // resolved below
        "pending_events": pending,
        "last_successful_sync_at": last_sync,
        "days_since_last_sync": null,
        "last_error": last_error,
        "device_id": device_id,
    }))
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

#[tauri::command]
pub async fn sync_force_full_resync(
    state: State<'_, AppState>,
    actor_user_id: String,
) -> Result<String, AppError> {
    crate::commands::rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    // Reset all rows back to pending so they re-push on next cycle
    for table in SYNC_TABLES {
        let sql = format!(
            "UPDATE {} SET sync_status = 'pending', sync_attempts = 0 WHERE sync_status = 'synced'",
            table
        );
        let _ = sqlx::query(&sql).execute(&state.db).await;
    }

    // Reset all watermarks to force a full re-pull
    let _ = sqlx::query("UPDATE sync_watermark SET last_pulled_at = '1970-01-01T00:00:00Z'")
        .execute(&state.db)
        .await;

    // Push + pull immediately
    state.sync_worker.run_once().await;

    let queued = count_pending(&state.db).await.unwrap_or(0);
    let worker_state = state.sync_worker.state.lock().await;
    if worker_state.online {
        Ok(format!("Full re-sync started. {queued} rows pending push."))
    } else if let Some(ref e) = worker_state.last_error {
        Ok(format!("Re-sync queued {queued} rows but push reported: {e}"))
    } else {
        Ok(format!("Re-sync queued {queued} rows."))
    }
}

// ── admin_setup_supabase ──────────────────────────────────────────────────────

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
        return Err(AppError::Validation("Personal access token is required".into()));
    }

    let client = SupabaseClient::new(url.clone(), service_key.clone());

    client.validate().await.map_err(|_| {
        AppError::Validation("Could not connect to Supabase — check the URL and service role key".into())
    })?;

    let project_ref = extract_project_ref(&url).ok_or_else(|| {
        AppError::Validation("Invalid Supabase URL format (expected https://xyz.supabase.co)".into())
    })?;

    client
        .migrate(&pat, &project_ref, CENTRAL_SCHEMA_SQL)
        .await
        .map_err(|e| AppError::Validation(format!("Schema migration failed: {e}")))?;

    ai_admin_repo::set_config(&state.db, "supabase_url", &url).await?;
    if crate::secure_store::set_secret("supabase_service_key", &service_key) {
        let _ = ai_admin_repo::set_config(&state.db, "supabase_service_key", "").await;
    } else {
        tracing::error!("CRITICAL: Failed to write service key to OS credential store.");
    }

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

    // Set all local data to pending so it pushes on next sync
    for table in SYNC_TABLES {
        let sql = format!("UPDATE {} SET sync_status = 'pending' WHERE sync_status = 'synced'", table);
        let _ = sqlx::query(&sql).execute(&state.db).await;
    }

    let worker = state.sync_worker.clone();
    tauri::async_runtime::spawn(async move { let _ = worker.run_once().await; });

    Ok(())
}

// ── admin_setup_supabase_creds_only ──────────────────────────────────────────

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
    client.validate().await.map_err(|_| {
        AppError::Validation("Could not connect to Supabase — check the URL and service role key".into())
    })?;

    ai_admin_repo::set_config(&state.db, "supabase_url", &url).await?;
    if crate::secure_store::set_secret("supabase_service_key", &service_key) {
        let _ = ai_admin_repo::set_config(&state.db, "supabase_service_key", "").await;
    } else {
        tracing::error!("CRITICAL: Failed to write service key to OS credential store.");
    }

    tracing::info!("Supabase credentials stored (no schema migration): {url}");

    for table in SYNC_TABLES {
        let sql = format!("UPDATE {} SET sync_status = 'pending' WHERE sync_status = 'synced'", table);
        let _ = sqlx::query(&sql).execute(&state.db).await;
    }

    let worker = state.sync_worker.clone();
    tauri::async_runtime::spawn(async move { let _ = worker.run_once().await; });

    Ok(())
}

// ── sync_queue_list ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SyncQueueItem {
    pub id: String,            // composite: {table}:{row_id}
    pub entity_type: String,
    pub entity_id: String,
    pub operation: String,
    pub status: String,
    pub attempt_count: i64,
    pub last_error: Option<String>,
    pub created_at: String,
}

#[tauri::command]
pub async fn sync_queue_list(state: State<'_, AppState>) -> Result<Vec<SyncQueueItem>, AppError> {
    let mut items = Vec::new();

    for table in SYNC_TABLES {
        let sql = format!(
            "SELECT id, sync_status, sync_attempts, '', created_at
             FROM {} WHERE sync_status IN ('pending', 'failed')
             LIMIT 200",
            table
        );
        if let Ok(rows) = sqlx::query(&sql).fetch_all(&state.db).await {
            for r in &rows {
                let row_id: String = r.get("id");
                items.push(SyncQueueItem {
                    id: format!("{}:{}", table, row_id),
                    entity_type: table.to_string(),
                    entity_id: row_id,
                    operation: "upsert".to_string(),
                    status: r.get("sync_status"),
                    attempt_count: r.get("sync_attempts"),
                    last_error: None,
                    created_at: r.get("created_at"),
                });
            }
        }
    }

    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    items.truncate(200);
    Ok(items)
}

// ── sync_queue_retry ──────────────────────────────────────────────────────────

/// Resets a pending row back to 'pending' with zero attempts.
/// The id parameter is composite: {table}:{row_id}
#[tauri::command]
pub async fn sync_queue_retry(
    id: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    let (table, row_id) = id.split_once(':')
        .ok_or_else(|| AppError::Validation("Invalid sync item id format. Expected table:id".into()))?;

    let sql = format!(
        "UPDATE {} SET sync_status = 'pending', sync_attempts = 0 WHERE id = ? AND sync_status = 'failed'",
        table
    );
    let rows = sqlx::query(&sql).bind(row_id).execute(&state.db).await?.rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound(format!("Sync item {id} not found or not in retryable state")));
    }
    Ok(())
}

// ── sync_queue_dismiss ────────────────────────────────────────────────────────

/// Marks a pending/failed row as 'synced' to stop retrying it.
/// The id parameter is composite: {table}:{row_id}
#[tauri::command]
pub async fn sync_queue_dismiss(
    id: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    let (table, row_id) = id.split_once(':')
        .ok_or_else(|| AppError::Validation("Invalid sync item id format. Expected table:id".into()))?;

    let sql = format!("UPDATE {} SET sync_status = 'synced' WHERE id = ?", table);
    let rows = sqlx::query(&sql).bind(row_id).execute(&state.db).await?.rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound(format!("Sync item {id} not found")));
    }
    Ok(())
}

// ── admin_get_supabase_status ─────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SupabaseStatus {
    pub configured: bool,
    pub url: String,
}

#[tauri::command]
pub async fn admin_get_supabase_status(
    state: State<'_, AppState>,
) -> Result<SupabaseStatus, AppError> {
    let url = ai_admin_repo::get_config(&state.db, "supabase_url")
        .await?
        .unwrap_or_default();
    let key = {
        let from_os = crate::secure_store::get_secret("supabase_service_key").unwrap_or_default();
        if !from_os.is_empty() { from_os }
        else { ai_admin_repo::get_config(&state.db, "supabase_service_key").await.unwrap_or_default().unwrap_or_default() }
    };
    Ok(SupabaseStatus { configured: !url.is_empty() && !key.is_empty(), url })
}
