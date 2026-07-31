use crate::commands::rbac;
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::Serialize;
use sqlx::Row;
use tauri::State;

/// Fire-and-forget a sync cycle after high-value admin mutations.
/// The local write has already committed; sync failures are surfaced by SyncChip
/// and health diagnostics instead of failing the user-facing mutation.
pub(crate) fn schedule_immediate_sync(state: &AppState) {
    let worker = state.sync_worker.clone();
    let hub = state.hub.clone();
    let pool = state.db.clone();
    tauri::async_runtime::spawn(async move {
        let origin =
            sqlx::query_scalar::<_, String>("SELECT value FROM app_config WHERE key='device_id'")
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "hub".into());
        if let Some(handle) = hub.lock().await.handle.as_ref() {
            handle.events.publish("*", &origin);
        }
        worker.run_once().await;
    });
}

/// Resolve the active device_id for THIS terminal.
/// Prefers the app_config 'device_id' key written at setup time so the correct
/// identity is returned even after other terminals' device records sync locally.
pub(crate) async fn active_device_id(state: &AppState) -> AppResult<String> {
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
    "product_barcodes",
    "suppliers",
    "purchase_orders",
    "purchase_order_lines",
    "devices",
    "roles",
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
    "product_cost_history",
    "cash_events", // was missing — cash events invisible to admin commands
];

/// Maps each sync table to its primary key column.
pub fn table_pk(table: &str) -> &str {
    match table {
        "branches" => "branch_id",
        "categories" => "category_id",
        "tax_rules" => "tax_rule_id",
        "products" => "product_id",
        "product_barcodes" => "barcode",
        "suppliers" => "supplier_id",
        "purchase_orders" => "po_id",
        "purchase_order_lines" => "po_line_id",
        "devices" => "device_id",
        "roles" => "role_id",
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
        "product_cost_history" => "cost_history_id",
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
    state.sync_worker.run_once_wait().await;

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
    /// Approximate row count across key store data after the pull.
    pub rows_pulled: u64,
    pub error: Option<String>,
    pub products: u64,
    pub categories: u64,
    pub product_barcodes: u64,
    pub product_prices: u64,
    pub users: u64,
    pub devices: u64,
    pub stock_levels: u64,
    pub suppliers: u64,
    pub settings: u64,
    pub pending_sync: u64,
    pub consistency_score: u8,
    pub schema_match: bool,
    pub hub_truth_ok: bool,
    pub mismatched_tables: Vec<String>,
}

fn setup_pull_catalog_allowed(setup_done: bool, hub_url: Option<&str>) -> bool {
    !setup_done || hub_url.is_some_and(|url| !url.trim().is_empty())
}

fn join_snapshot_gate_ready(score: u8, schema_match: bool, pending_sync: u64) -> bool {
    score == 100 && schema_match && pending_sync == 0
}

async fn clear_join_replica(pool: &sqlx::SqlitePool) -> AppResult<()> {
    let child_first = [
        "cash_events",
        "product_cost_history",
        "product_prices",
        "delivery_orders",
        "audit_logs",
        "stock_levels",
        "stock_movements",
        "refund_items",
        "refunds",
        "payments",
        "sale_items",
        "sales",
        "shifts",
        "customers",
        "users",
        "roles",
        "devices",
        "purchase_order_lines",
        "purchase_orders",
        "suppliers",
        "product_barcodes",
        "products",
        "tax_rules",
        "categories",
        "branches",
    ];
    let mut tx = pool.begin().await?;
    sqlx::query("PRAGMA defer_foreign_keys = ON")
        .execute(&mut *tx)
        .await?;
    for table in child_first {
        sqlx::query(&format!("DELETE FROM {table}"))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[tauri::command]
pub async fn setup_pull_catalog(state: State<'_, AppState>) -> Result<PullSummary, AppError> {
    // Called from the JoinStore wizard before the session user is established.
    // A joined terminal is marked setup_complete by hub_join before the frontend
    // can ask for the blocking first pull. Allow that terminal-mode case, but
    // keep completed standalone/hub stores using the authenticated sync panel.
    let setup_done: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'setup_complete'")
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);
    let hub_url: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'hub_url'")
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);
    if !setup_pull_catalog_allowed(setup_done.as_deref() == Some("1"), hub_url.as_deref()) {
        return Err(AppError::Permission(
            "Setup is already complete. Use the sync panel to pull catalog updates.".into(),
        ));
    }
    let initialized: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key='join_snapshot_initialized'")
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);
    if initialized.as_deref() != Some("1") {
        clear_join_replica(&state.db).await?;
        sqlx::query(
            "INSERT INTO app_config(key,value,updated_at) VALUES ('join_snapshot_initialized','1',?)
             ON CONFLICT(key) DO UPDATE SET value='1', updated_at=excluded.updated_at",
        )
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&state.db)
        .await?;
    }
    sqlx::query("DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'")
        .execute(&state.db)
        .await?;
    let mut rows_pulled = 0u32;
    let mut pull_error: Option<String> = None;
    for attempt in 0..3 {
        match state.sync_worker.pull_only_wait().await {
            Ok(rows) => {
                rows_pulled = rows_pulled.saturating_add(rows);
                pull_error = None;
                break;
            }
            Err(error) => {
                pull_error = Some(error.internal_database_detail());
                if attempt < 2 {
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                }
            }
        }
    }

    // Count key store rows as a concrete success indicator.
    let products: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE is_active = 1")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let categories: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM categories")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let product_barcodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product_barcodes")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let product_prices: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product_prices")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_active = 1")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let devices: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE is_active = 1")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let stock_levels: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stock_levels")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let suppliers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM suppliers WHERE is_active = 1")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let settings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_config")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let pending_sync = count_pending(&state.db).await.unwrap_or(0);
    let client = state
        .sync_worker
        .load_client()
        .await
        .ok_or_else(|| AppError::Validation("Hub connection is not configured".into()))?;
    let local_snapshot = crate::sync_v2::consistency::snapshot(&state.db).await?;
    let hub_snapshot = client.hub_consistency().await?;
    let truth = compare_consistency_snapshots(local_snapshot, hub_snapshot);
    let ok = pull_error.is_none()
        && truth.ok
        && join_snapshot_gate_ready(truth.score, truth.schema_match, pending_sync as u64);
    let error = if ok {
        None
    } else if let Some(error) = pull_error {
        Some(error)
    } else {
        Some(truth.message.clone())
    };
    let mismatched_tables = truth
        .tables
        .iter()
        .filter(|table| table.status != "match")
        .map(|table| table.table.clone())
        .collect();

    Ok(PullSummary {
        ok,
        rows_pulled: rows_pulled as u64,
        error,
        products: products as u64,
        categories: categories as u64,
        product_barcodes: product_barcodes as u64,
        product_prices: product_prices as u64,
        users: users as u64,
        devices: devices as u64,
        stock_levels: stock_levels as u64,
        suppliers: suppliers as u64,
        settings: settings as u64,
        pending_sync: pending_sync as u64,
        consistency_score: truth.score,
        schema_match: truth.schema_match,
        hub_truth_ok: truth.ok,
        mismatched_tables,
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
    state.sync_worker.run_once_wait().await;

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
    pub last_push_success_at: Option<String>,
    pub last_pull_success_at: Option<String>,
    pub last_error_at: Option<String>,
    pub last_error: Option<String>,
    pub last_failed_row_id: Option<String>,
    pub retry_count: i64,
    pub table_checksum: Option<String>,
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
    pub consistency_score: Option<u8>,
    pub conflicts_open: i64,
}

#[derive(Serialize)]
pub struct HubTruthTableCompare {
    pub table: String,
    pub local_count: i64,
    pub hub_count: i64,
    pub local_checksum: String,
    pub hub_checksum: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct HubTruthCompareResult {
    pub ok: bool,
    pub compared_at: String,
    pub score: u8,
    pub schema_match: bool,
    pub local_schema_version: i64,
    pub hub_schema_version: i64,
    pub tables: Vec<HubTruthTableCompare>,
    pub message: String,
}

#[derive(Serialize)]
pub struct SyncConflictRow {
    pub conflict_id: String,
    pub conflict_type: String,
    pub table_name: String,
    pub entity_id: Option<String>,
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub status: String,
    pub created_at: String,
}

fn conflict_resolution_allowed(resolution: &str) -> bool {
    matches!(
        resolution,
        "retry" | "pull_hub_truth" | "reconcile_stock" | "dismiss"
    )
}

fn compare_consistency_snapshots(
    local: crate::sync_v2::consistency::ConsistencySnapshot,
    hub: crate::sync_v2::consistency::ConsistencySnapshot,
) -> HubTruthCompareResult {
    let mut tables = Vec::new();
    let mut mismatched = 0usize;
    for local_table in &local.tables {
        let hub_table = hub.tables.iter().find(|t| t.table == local_table.table);
        let (hub_count, hub_checksum) = hub_table
            .map(|t| (t.count, t.checksum.clone()))
            .unwrap_or((0, String::new()));
        let status = if hub_table.is_none() {
            "missing_on_hub"
        } else if local_table.count != hub_count {
            "count_mismatch"
        } else if local_table.checksum != hub_checksum {
            "checksum_mismatch"
        } else {
            "match"
        };
        if status != "match" {
            mismatched += 1;
        }
        tables.push(HubTruthTableCompare {
            table: local_table.table.clone(),
            local_count: local_table.count,
            hub_count,
            local_checksum: local_table.checksum.clone(),
            hub_checksum,
            status: status.to_string(),
        });
    }
    let schema_match = local.schema_version == hub.schema_version;
    if !schema_match {
        mismatched += 1;
    }
    let score = crate::sync_v2::consistency::consistency_score(local.tables.len() + 1, mismatched);
    let ok = schema_match && tables.iter().all(|table| table.status == "match");
    HubTruthCompareResult {
        ok,
        compared_at: chrono::Utc::now().to_rfc3339(),
        score,
        schema_match,
        local_schema_version: local.schema_version,
        hub_schema_version: hub.schema_version,
        tables,
        message: if ok {
            "This terminal matches the hub truth snapshot.".into()
        } else {
            "This terminal differs from the hub. Pull Hub Truth Now, then review conflicts if mismatches remain.".into()
        },
    }
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

        let health = sqlx::query(
            "SELECT last_push_success_at, last_pull_success_at, last_error_at,
                    last_error, last_failed_row_id, retry_count, table_checksum
             FROM sync_table_health WHERE table_name = ?",
        )
        .bind(table)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

        let has_health_issue = health
            .as_ref()
            .and_then(|r| r.get::<Option<String>, _>("last_error"))
            .is_some();

        if pending > 0 || stuck > 0 || has_health_issue {
            tables.push(SyncDiagTable {
                table: table.to_string(),
                pending,
                stuck,
                max_attempts: max_att,
                avg_attempts: format!("{:.1}", avg_att).parse().unwrap_or(0.0),
                last_push_success_at: health
                    .as_ref()
                    .and_then(|r| r.get::<Option<String>, _>("last_push_success_at")),
                last_pull_success_at: health
                    .as_ref()
                    .and_then(|r| r.get::<Option<String>, _>("last_pull_success_at")),
                last_error_at: health
                    .as_ref()
                    .and_then(|r| r.get::<Option<String>, _>("last_error_at")),
                last_error: health
                    .as_ref()
                    .and_then(|r| r.get::<Option<String>, _>("last_error")),
                last_failed_row_id: health
                    .as_ref()
                    .and_then(|r| r.get::<Option<String>, _>("last_failed_row_id")),
                retry_count: health.as_ref().map(|r| r.get("retry_count")).unwrap_or(0),
                table_checksum: health
                    .as_ref()
                    .and_then(|r| r.get::<Option<String>, _>("table_checksum")),
            });
        }

        total_pending += pending;
        total_stuck += stuck;
    }

    tables.sort_by_key(|t| -(t.pending + t.stuck));

    let conflicts_open: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sync_conflicts WHERE status = 'open'")
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

    let consistency_score = match hub_truth_compare(actor_user_id.clone(), state.clone()).await {
        Ok(r) => Some(r.score),
        Err(_) => None,
    };

    Ok(SyncDiagnostics {
        hub_configured: configured,
        pending_events: total_pending,
        stuck_events: total_stuck,
        last_sync_at: last_sync,
        last_error,
        online,
        tables,
        consistency_score,
        conflicts_open,
    })
}

#[tauri::command]
pub async fn hub_truth_compare(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<HubTruthCompareResult, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let local = crate::sync_v2::consistency::snapshot(&state.db).await?;
    let hub = match state.sync_worker.load_client().await {
        Some(client) => client.hub_consistency().await?,
        None => local.clone(),
    };

    Ok(compare_consistency_snapshots(local, hub))
}

#[tauri::command]
pub async fn hub_truth_pull(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<HubTruthCompareResult, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    sqlx::query("DELETE FROM app_config WHERE key LIKE 'sync_v2_watermark_%'")
        .execute(&state.db)
        .await?;
    state.sync_worker.run_once_wait().await;
    hub_truth_compare(actor_user_id, state).await
}

#[tauri::command]
pub async fn sync_conflicts_list(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<SyncConflictRow>, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let rows = sqlx::query(
        "SELECT conflict_id, conflict_type, table_name, entity_id, severity,
                title, detail, status, created_at
         FROM sync_conflicts
         WHERE status = 'open'
         ORDER BY created_at DESC
         LIMIT 100",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| SyncConflictRow {
            conflict_id: r.get("conflict_id"),
            conflict_type: r.get("conflict_type"),
            table_name: r.get("table_name"),
            entity_id: r.get("entity_id"),
            severity: r.get("severity"),
            title: r.get("title"),
            detail: r.get("detail"),
            status: r.get("status"),
            created_at: r.get("created_at"),
        })
        .collect())
}

#[tauri::command]
pub async fn sync_conflict_resolve(
    actor_user_id: String,
    conflict_id: String,
    resolution: String,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    if !conflict_resolution_allowed(&resolution) {
        return Err(AppError::Validation(
            "Unsupported conflict resolution".into(),
        ));
    }
    let conflict = sqlx::query(
        "SELECT table_name, entity_id FROM sync_conflicts
         WHERE conflict_id=? AND status='open'",
    )
    .bind(&conflict_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Open sync conflict not found".into()))?;
    let table: String = conflict.get("table_name");
    let entity_id: Option<String> = conflict.get("entity_id");

    match resolution.as_str() {
        "retry" => {
            if !SYNC_TABLES.contains(&table.as_str()) {
                return Err(AppError::Validation(
                    "Conflict table is not syncable".into(),
                ));
            }
            if let Some(entity_id) = entity_id.as_deref() {
                let pk = table_pk(&table);
                sqlx::query(&format!(
                    "UPDATE {table} SET sync_status='pending', sync_attempts=0 WHERE {pk}=?"
                ))
                .bind(entity_id)
                .execute(&state.db)
                .await?;
            }
            state.sync_worker.run_once_wait().await;
        }
        "pull_hub_truth" => {
            sqlx::query("DELETE FROM app_config WHERE key=?")
                .bind(format!("sync_v2_watermark_{table}"))
                .execute(&state.db)
                .await?;
            state.sync_worker.pull_only_wait().await?;
        }
        "reconcile_stock" => {
            crate::inventory::movements::reconcile_stock_drift(&state.db).await?;
            schedule_immediate_sync(&state);
        }
        "dismiss" => {}
        _ => unreachable!(),
    }

    sqlx::query("UPDATE sync_conflicts SET status='resolved', resolved_at=? WHERE conflict_id=?")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(&conflict_id)
        .execute(&state.db)
        .await?;
    Ok(format!("Conflict resolved using {resolution}."))
}

#[tauri::command]
pub async fn sync_stock_drift_report(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<crate::inventory::movements::StockDriftRow>, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    crate::inventory::movements::stock_drift_report(&state.db).await
}

#[tauri::command]
pub async fn sync_stock_drift_reconcile(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<u64, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let repaired = crate::inventory::movements::reconcile_stock_drift(&state.db).await?;
    if repaired > 0 {
        schedule_immediate_sync(&state);
    }
    Ok(repaired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_pull_is_allowed_for_completed_joined_terminal() {
        assert!(setup_pull_catalog_allowed(
            true,
            Some("http://192.168.1.50:8923")
        ));
    }

    #[test]
    fn setup_pull_stays_blocked_for_completed_standalone_store() {
        assert!(!setup_pull_catalog_allowed(true, None));
    }

    #[test]
    fn join_snapshot_gate_requires_exact_hub_truth() {
        assert!(join_snapshot_gate_ready(100, true, 0));
        assert!(!join_snapshot_gate_ready(99, true, 0));
        assert!(!join_snapshot_gate_ready(100, false, 0));
        assert!(!join_snapshot_gate_ready(100, true, 1));
    }

    #[test]
    fn conflict_resolution_actions_are_explicitly_allowlisted() {
        for action in ["retry", "pull_hub_truth", "reconcile_stock", "dismiss"] {
            assert!(conflict_resolution_allowed(action));
        }
        assert!(!conflict_resolution_allowed("delete_remote_rows"));
    }

    #[test]
    fn sync_command_tables_cover_all_retryable_sync_domains() {
        for table in [
            "roles",
            "suppliers",
            "purchase_orders",
            "purchase_order_lines",
            "product_cost_history",
        ] {
            assert!(
                SYNC_TABLES.contains(&table),
                "{table} missing from sync commands"
            );
            assert_ne!(
                table_pk(table),
                "id",
                "{table} must have an explicit primary key"
            );
        }
    }
}
