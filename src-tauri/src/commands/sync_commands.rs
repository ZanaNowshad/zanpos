use crate::commands::rbac;
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::Serialize;
use sqlx::Row;
use tauri::State;

/// Resolve the active device_id for THIS terminal.
/// Prefers the app_config 'device_id' key written at setup time so the correct
/// identity is returned even after other terminals' device records sync locally.
async fn active_device_id(state: &AppState) -> AppResult<String> {
    if let Ok(Some(id)) =
        sqlx::query_scalar::<_, String>("SELECT value FROM app_config WHERE key = 'device_id'")
            .fetch_optional(&state.db)
            .await
    {
        if !id.is_empty() {
            return Ok(id);
        }
    }
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
    "branches", // was missing — local branch edits were invisible to admin commands
    "categories",
    "tax_rules",
    "products",
    "devices",
    "users",
    "customers",
    "shifts",
    "sales",
    "sale_items",
    "payments",
    "refunds",
    "refund_items",
    "stock_movements",
    "stock_levels", // was missing — stock levels invisible to admin commands
    "audit_logs",
    "delivery_orders",
    "product_prices",
    "cash_events", // was missing — cash events invisible to admin commands
];

/// Maps each sync table to its primary key column.
pub fn table_pk(table: &str) -> &str {
    match table {
        "branches" => "branch_id",
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
        "stock_levels" => "stock_level_id",
        "audit_logs" => "audit_log_id",
        "delivery_orders" => "delivery_id",
        "product_prices" => "price_id",
        "cash_events" => "cash_event_id",
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
pub async fn sync_status(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    let worker_state = state.sync_worker.state.lock().await;
    let worker_online = worker_state.online;
    let last_error = worker_state.last_error.clone();
    drop(worker_state);

    let device_id = active_device_id(&state).await?;
    let pending = count_pending(&state.db).await.unwrap_or(0);

    // Check hub configuration
    let hub_mode: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key='hub_mode'")
            .fetch_optional(&state.db)
            .await?
            .flatten();
    let hub_url_raw: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key='hub_url'")
            .fetch_optional(&state.db)
            .await?
            .flatten();
    let hub_url = hub_url_raw.filter(|s: &String| !s.is_empty());
    let is_hub = hub_mode.as_deref() == Some("1");
    let configured = is_hub
        || (hub_url.is_some()
            && crate::secure_store::get_secret("hub_store_token").is_some_and(|k| !k.is_empty()));
    let online = is_hub || worker_online || configured;

    // Read last successful sync from watermark table
    let last_sync: Option<String> =
        sqlx::query_scalar("SELECT last_pushed_at FROM sync_watermark WHERE table_name = 'sales'")
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
        "hub_configured": configured,
        "mode": if is_hub {"hub"} else if hub_url.is_some() {"terminal"} else {"standalone"},
        "hub_url": hub_url,
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
pub async fn sync_trigger_now(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    state.sync_worker.run_once().await;
    let worker_state = state.sync_worker.state.lock().await;
    if worker_state.online {
        Ok("Sync completed successfully".to_string())
    } else if let Some(ref e) = worker_state.last_error {
        Ok(format!("Sync attempted — last error: {e}"))
    } else {
        Ok("Sync not configured — connect this terminal to the hub in Settings → Hub".to_string())
    }
}

// ── sync_bulk_initial ─────────────────────────────────────────────────────────

/// Bulk-push ALL local data to the hub in one shot (no 50-row batch limit).
/// Designed for first-time sync during setup — pushes all tables at once.
/// Called from setup_wizard_complete and hub_join.
#[tauri::command]
pub async fn sync_bulk_initial(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let client = match state.sync_worker.load_client().await {
        Some(c) => c,
        None => return Ok("Sync skipped — hub not configured".into()),
    };

    let total = state
        .sync_worker
        .push_all_bulk(&client)
        .await
        .map_err(|e| AppError::Internal(format!("Bulk sync failed: {e}")))?;

    // Also pull so this terminal gets any remote data
    let _ = state.sync_worker.run_once().await;

    Ok(format!(
        "Initial sync complete: {} rows pushed to the hub",
        total
    ))
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
pub async fn setup_pull_catalog(state: State<'_, AppState>) -> Result<PullSummary, AppError> {
    // Called from the JoinStore wizard before the session user is established.
    // Gate on setup_complete to prevent post-setup abuse.
    let setup_done: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'setup_complete'")
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);
    if setup_done.as_deref() == Some("1") {
        return Err(AppError::Permission(
            "Setup is already complete. Use the sync panel to pull catalog updates.".into(),
        ));
    }
    // Run one push + pull cycle synchronously.
    // Watermarks are already at epoch for a fresh join (seeded by migration 0010_sync.sql).
    state.sync_worker.run_once().await;

    // Count key catalog rows as a concrete success indicator
    let products: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE is_active = 1")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let categories: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM categories")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_active = 1")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let worker_state = state.sync_worker.state.lock().await;
    // Bug-Join-09: derive ok from catalog presence OR worker online state.
    // worker_state.online alone is unreliable if run_once() was skipped by the
    // execution mutex; checking row counts provides a concrete success indicator.
    let ok = products > 0 || categories > 0 || worker_state.online;
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
    // FIX: also reset v2 app_config watermarks — the v2 worker uses these,
    // not the sync_watermark table. Without this, re-pull never actually happens.
    let _ = sqlx::query("DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'")
        .execute(&state.db)
        .await;

    // Reset consecutive failure counter and clear last_error so adaptive backoff
    // is lifted immediately — without this, force-resync still waits up to 5x the
    // normal interval before the first push cycle actually runs.
    {
        let mut st = state.sync_worker.state.lock().await;
        st.consecutive_failures = 0;
        st.last_error = None;
        st.online = false; // will be set to true by run_once() if push/pull succeeds
    }

    // Push + pull immediately
    state.sync_worker.run_once().await;

    let queued = count_pending(&state.db).await.unwrap_or(0);
    let worker_state = state.sync_worker.state.lock().await;
    if worker_state.online {
        Ok(format!("Full re-sync started. {queued} rows pending push."))
    } else if let Some(ref e) = worker_state.last_error {
        Ok(format!(
            "Re-sync queued {queued} rows but push reported: {e}"
        ))
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

// ── sync_queue_list ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SyncQueueItem {
    #[serde(rename = "sync_event_id")]
    pub id: String, // composite: {table}:{row_id}
    pub entity_type: String,
    pub entity_id: String,
    pub operation: String,
    pub status: String,
    pub attempt_count: i64,
    pub last_error: Option<String>,
    pub created_at: String,
}

#[tauri::command]
pub async fn sync_queue_list(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<SyncQueueItem>, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
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

    let (table, row_id) = id.split_once(':').ok_or_else(|| {
        AppError::Validation("Invalid sync item id format. Expected table:id".into())
    })?;
    let pk = table_pk(table);

    // FIX: the v2 worker never writes sync_status='failed' — stuck rows remain at
    // 'pending' with high sync_attempts (>= 10). Match both to make Retry button work.
    let sql = format!(
        "UPDATE {table} SET sync_status = 'pending', sync_attempts = 0 \
         WHERE {pk} = ? AND (sync_status = 'failed' OR (sync_status = 'pending' AND sync_attempts >= 10))",
    );
    let rows = sqlx::query(&sql)
        .bind(row_id)
        .execute(&state.db)
        .await?
        .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound(format!(
            "Sync item {id} not found or not in retryable state"
        )));
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

    let (table, row_id) = id.split_once(':').ok_or_else(|| {
        AppError::Validation("Invalid sync item id format. Expected table:id".into())
    })?;
    let pk = table_pk(table);

    let sql = format!("UPDATE {table} SET sync_status = 'synced' WHERE {pk} = ?");
    let rows = sqlx::query(&sql)
        .bind(row_id)
        .execute(&state.db)
        .await?
        .rows_affected();

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
pub async fn sync_queue_stats(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<SyncTableStats>, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let mut stats = Vec::new();

    for table in SYNC_TABLES {
        let pending: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {table} WHERE sync_status = 'pending'"
        ))
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

        let failed: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {table} WHERE sync_status = 'pending' AND sync_attempts >= 10"
        ))
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

        let max_attempts: i64 = sqlx::query_scalar(&format!(
            "SELECT COALESCE(MAX(sync_attempts), 0) FROM {table}"
        ))
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
    pub hub_configured: bool,
    pub pending_events: i64,
    pub stuck_events: i64,
    pub last_sync_at: Option<String>,
    pub last_error: Option<String>,
    pub online: bool,
    pub tables: Vec<SyncDiagTable>,
}

#[tauri::command]
pub async fn sync_diagnostics(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<SyncDiagnostics, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let worker_state = state.sync_worker.state.lock().await;
    let online = worker_state.online;
    let last_error = worker_state.last_error.clone();
    drop(worker_state);

    let hub_mode: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key='hub_mode'")
            .fetch_optional(&state.db)
            .await?
            .flatten();
    let hub_url_raw: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key='hub_url'")
            .fetch_optional(&state.db)
            .await?
            .flatten();
    let hub_url = hub_url_raw.filter(|s: &String| !s.is_empty());
    let is_hub = hub_mode.as_deref() == Some("1");
    let configured = is_hub
        || (hub_url.is_some()
            && crate::secure_store::get_secret("hub_store_token").is_some_and(|k| !k.is_empty()));

    let last_sync: Option<String> =
        sqlx::query_scalar("SELECT last_pushed_at FROM sync_watermark WHERE table_name = 'sales'")
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

    let mut tables = Vec::new();
    let mut total_pending: i64 = 0;
    let mut total_stuck: i64 = 0;

    for table in SYNC_TABLES {
        let pending: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {table} WHERE sync_status = 'pending' AND sync_attempts < 10"
        ))
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

        let stuck: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {table} WHERE sync_status = 'pending' AND sync_attempts >= 10"
        ))
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

        let max_att: i64 = sqlx::query_scalar(&format!(
            "SELECT COALESCE(MAX(sync_attempts), 0) FROM {table}"
        ))
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
        hub_configured: configured,
        pending_events: total_pending,
        stuck_events: total_stuck,
        last_sync_at: last_sync,
        last_error,
        online,
        tables,
    })
}
