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
pub const SYNC_TABLES: &[&str] = &[
    "categories", "tax_rules", "products", "devices", "users", "customers",
    "shifts", "sales", "sale_items", "payments", "refunds", "refund_items",
    "stock_movements", "audit_logs", "delivery_orders", "product_prices",
];

/// Maps each sync table to its primary key column.
pub fn table_pk(table: &str) -> &str {
    match table {
        "categories" => "category_id",
        "tax_rules" => "tax_rule_id",
        "products" => "product_id",
        "devices" => "device_id",
        "users" => "user_id",
        "customers" => "customer_id",
        "shifts" => "shift_id",
        "sales" => "sale_id",
        "sale_items" => "sale_item_id",
        "payments" => "payment_id",
        "refunds" => "refund_id",
        "refund_items" => "refund_item_id",
        "stock_movements" => "movement_id",
        "audit_logs" => "audit_log_id",
        "delivery_orders" => "delivery_id",
        "product_prices" => "price_id",
        _ => "id",
    }
}

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
    let worker_online = worker_state.online;
    let last_error = worker_state.last_error.clone();
    drop(worker_state);

    let device_id = active_device_id(&state).await?;
    let pending = count_pending(&state.db).await.unwrap_or(0);

    // Check Supabase configuration (two-phase: OS keyring, then DB fallback)
    let supabase_configured = {
        let url: Option<String> =
            sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'supabase_url'")
                .fetch_optional(&state.db)
                .await?
                .flatten();
        let key = crate::secure_store::get_secret("supabase_service_key")
            .unwrap_or_default();
        url.as_deref().is_some_and(|u| !u.is_empty()) && !key.is_empty()
    };

    // Online = Supabase is configured AND at least one cycle completed OR worker says so.
    // Even before the first cycle, if Supabase is configured, we're ready to sync.
    let online = worker_online || supabase_configured;

    // Read last successful sync from watermark table
    let last_sync: Option<String> = sqlx::query_scalar(
        "SELECT last_pushed_at FROM sync_watermark WHERE table_name = 'sales'",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let consecutive_failure_count = {
        let st = state.sync_worker.state.lock().await;
        st.consecutive_failures
    };

    Ok(serde_json::json!({
        "online": online,
        "supabase_configured": supabase_configured,
        "pending_events": pending,
        "last_successful_sync_at": last_sync,
        "days_since_last_sync": null,
        "last_error": last_error,
        "device_id": device_id,
        "consecutive_failure_count": consecutive_failure_count,
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

// ── sync_bulk_initial ─────────────────────────────────────────────────────────

/// Bulk-push ALL local data to Supabase in one shot (no 50-row batch limit).
/// Designed for first-time sync during setup — pushes all tables at once.
/// Called from setup_wizard_complete and setup_join_store.
#[tauri::command]
pub async fn sync_bulk_initial(state: State<'_, AppState>) -> Result<String, AppError> {
    let client = match state.sync_worker.load_client().await {
        Some(c) => c,
        None => return Ok("Sync skipped — Supabase not configured".into()),
    };

    let total = state.sync_worker.push_all_bulk(&client).await
        .map_err(|e| AppError::Internal(format!("Bulk sync failed: {e}")))?;

    // Also pull so this terminal gets any remote data
    let _ = state.sync_worker.run_once().await;

    Ok(format!("Initial sync complete: {} rows pushed to Supabase", total))
}

// ── setup_pull_catalog ────────────────────────────────────────────────────────

/// Blocking initial catalog pull for the JoinStore setup wizard.
/// Runs one full sync cycle synchronously so the terminal has products,
/// categories, and users before the setup wizard completes.
/// Returns a summary the frontend can display as confirmation.
#[derive(Debug, serde::Serialize)]
pub struct PullSummary {
    pub ok: bool,
    /// Approximate row count across products + categories + users
    pub rows_pulled: u64,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn setup_pull_catalog(
    state: State<'_, AppState>,
) -> Result<PullSummary, AppError> {
    // Run one push + pull cycle synchronously.
    // Watermarks are already at epoch for a fresh join (seeded by migration 0010_sync.sql).
    state.sync_worker.run_once().await;

    // Count key catalog rows as a concrete success indicator
    let products: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE is_active = 1")
            .fetch_one(&state.db).await.unwrap_or(0);
    let categories: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM categories")
            .fetch_one(&state.db).await.unwrap_or(0);
    let users: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_active = 1")
            .fetch_one(&state.db).await.unwrap_or(0);

    let worker_state = state.sync_worker.state.lock().await;
    let ok = worker_state.online;
    let error = worker_state.last_error.clone();
    drop(worker_state);

    Ok(PullSummary {
        ok,
        rows_pulled: (products + categories + users) as u64,
        error,
    })
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

// ── sync_reset_stuck ────────────────────────────────────────────────────────

/// Reset stuck rows (sync_attempts >= 10) back to pending with 0 attempts.
/// These rows were permanently excluded from the push pipeline due to repeated
/// failures (schema mismatch, auth errors, etc.). After fixing the root cause
/// (e.g. re-entering Supabase credentials, fixing schema), call this to recover.
#[tauri::command]
pub async fn sync_reset_stuck(
    state: State<'_, AppState>,
    actor_user_id: String,
) -> Result<String, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    let mut total = 0u32;
    for table in SYNC_TABLES {
        let sql = format!(
            "UPDATE {table} SET sync_attempts = 0 WHERE sync_status = 'pending' AND sync_attempts >= 10",
        );
        let rows = sqlx::query(&sql).execute(&state.db).await?.rows_affected();
        total += rows as u32;
        if rows > 0 {
            tracing::info!("Sync: reset {rows} stuck rows in {table}");
        }
    }

    Ok(format!("Reset {total} stuck rows across all tables. Sync worker will pick them up on the next cycle."))
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
        tracing::error!("CRITICAL: Failed to write service key to OS credential store. Falling back to DB (plaintext).");
        ai_admin_repo::set_config(&state.db, "supabase_service_key", &service_key).await?;
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
        tracing::error!("CRITICAL: Failed to write service key to OS credential store. Falling back to DB (plaintext).");
        ai_admin_repo::set_config(&state.db, "supabase_service_key", &service_key).await?;
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
    #[serde(rename = "sync_event_id")]
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
        let pk = table_pk(table);
        let sql = format!(
            "SELECT {pk} AS _pk, sync_status, sync_attempts, '' AS _err, created_at
             FROM {table} WHERE sync_status IN ('pending', 'failed')
             LIMIT 200",
        );
        if let Ok(rows) = sqlx::query(&sql).fetch_all(&state.db).await {
            for r in &rows {
                let row_id: String = r.get("_pk");
                items.push(SyncQueueItem {
                    id: format!("{table}:{row_id}"),
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
    let pk = table_pk(table);

    let sql = format!(
        "UPDATE {table} SET sync_status = 'pending', sync_attempts = 0 WHERE {pk} = ? AND sync_status = 'failed'",
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
    let pk = table_pk(table);

    let sql = format!("UPDATE {table} SET sync_status = 'synced' WHERE {pk} = ?");
    let rows = sqlx::query(&sql).bind(row_id).execute(&state.db).await?.rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound(format!("Sync item {id} not found")));
    }
    Ok(())
}

// ── sync_queue_stats ──────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SyncTableStats {
    pub table: String,
    pub pending: i64,
    pub failed: i64,
    pub max_attempts: i64,
    pub attempts_dist: String,
}

#[tauri::command]
pub async fn sync_queue_stats(state: State<'_, AppState>) -> Result<Vec<SyncTableStats>, AppError> {
    let mut stats = Vec::new();

    for table in SYNC_TABLES {
        let pending: i64 = sqlx::query_scalar(
            &format!("SELECT COUNT(*) FROM {table} WHERE sync_status = 'pending'"),
        )
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

        let failed: i64 = sqlx::query_scalar(
            &format!(
                "SELECT COUNT(*) FROM {table} WHERE sync_status = 'pending' AND sync_attempts >= 10"
            ),
        )
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

        let max_attempts: i64 = sqlx::query_scalar(
            &format!("SELECT COALESCE(MAX(sync_attempts), 0) FROM {table}"),
        )
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

        let _pk = table_pk(table);
        let attempt_dist: Vec<String> = sqlx::query_scalar(
            &format!(
                "SELECT 'att' || sync_attempts || '=' || COUNT(*) FROM {table} GROUP BY sync_attempts ORDER BY sync_attempts LIMIT 11"
            ),
        )
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        stats.push(SyncTableStats {
            table: table.to_string(),
            pending,
            failed,
            max_attempts,
            attempts_dist: attempt_dist.join(", "),
        });
    }

    Ok(stats)
}

// ── sync_diagnostics ────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SyncDiagTable {
    pub table: String,
    pub pending: i64,
    pub stuck: i64,
    pub max_attempts: i64,
    pub avg_attempts: f64,
}

#[derive(Serialize)]
pub struct SyncDiagnostics {
    pub supabase_configured: bool,
    pub can_load_client: bool,
    pub pending_events: i64,
    pub stuck_events: i64,
    pub last_sync_at: Option<String>,
    pub last_error: Option<String>,
    pub online: bool,
    pub tables: Vec<SyncDiagTable>,
}

#[tauri::command]
pub async fn sync_diagnostics(
    state: State<'_, AppState>,
) -> Result<SyncDiagnostics, AppError> {
    let worker_state = state.sync_worker.state.lock().await;
    let online = worker_state.online;
    let last_error = worker_state.last_error.clone();
    drop(worker_state);

    let url: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'supabase_url'")
            .fetch_optional(&state.db)
            .await?
            .flatten();
    let key = crate::secure_store::get_secret("supabase_service_key").unwrap_or_default();
    let supabase_configured = url.as_deref().is_some_and(|u| !u.is_empty()) && !key.is_empty();

    let last_sync: Option<String> = sqlx::query_scalar(
        "SELECT last_pushed_at FROM sync_watermark WHERE table_name = 'sales'",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let mut tables = Vec::new();
    let mut total_pending: i64 = 0;
    let mut total_stuck: i64 = 0;

    for table in SYNC_TABLES {
        let pending: i64 = sqlx::query_scalar(
            &format!("SELECT COUNT(*) FROM {table} WHERE sync_status = 'pending' AND sync_attempts < 10"),
        )
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

        let stuck: i64 = sqlx::query_scalar(
            &format!("SELECT COUNT(*) FROM {table} WHERE sync_status = 'pending' AND sync_attempts >= 10"),
        )
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

        let max_att: i64 = sqlx::query_scalar(
            &format!("SELECT COALESCE(MAX(sync_attempts), 0) FROM {table}"),
        )
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

        let avg_att: f64 = sqlx::query_scalar(
            &format!("SELECT COALESCE(AVG(CAST(sync_attempts AS REAL)), 0) FROM {table} WHERE sync_status = 'pending'"),
        )
        .fetch_one(&state.db)
        .await
        .unwrap_or(0.0);

        if pending > 0 || stuck > 0 {
            tables.push(SyncDiagTable {
                table: table.to_string(),
                pending,
                stuck,
                max_attempts: max_att,
                avg_attempts: format!("{:.1}", avg_att).parse().unwrap_or(0.0),
            });
        }

        total_pending += pending;
        total_stuck += stuck;
    }

    tables.sort_by_key(|t| -(t.pending + t.stuck));

    Ok(SyncDiagnostics {
        supabase_configured,
        can_load_client: supabase_configured,
        pending_events: total_pending,
        stuck_events: total_stuck,
        last_sync_at: last_sync,
        last_error,
        online,
        tables,
    })
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
