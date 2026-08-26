use crate::db::repositories::ai_admin_repo;
use crate::errors::{AppError, AppResult};
use crate::secure_store;
use crate::sync_v2::apply::{
    self, has_origin_device_id, pk_for_table, skip_on_wire, value_from_row_column,
    ALLOWED_CONFIG_KEYS,
};
use crate::sync_v2::client::HttpSyncClient;
use crate::sync_v2::consistency;
use crate::telemetry::{self, ZanposSpan};
use serde_json::Value;
use sqlx::Column;
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Duration;

pub const TRANSIENT_TAG: &str = "[TRANSIENT]";
const BATCH_SIZE: i64 = 50;
/// Overlap subtracted from the stored watermark when asking the hub for rows.
///
/// Terminal clocks are never exactly aligned, so a row can be committed with a
/// timestamp fractionally behind one we have already passed. The watermark only
/// moves forward, so without an overlap that row is never offered again.
const PULL_LOOKBACK_SECS: i64 = 2;

/// Parse a timestamp in either format this codebase writes.
///
/// Most code writes RFC3339 via chrono; the catalogue importer and several SQL
/// defaults write `datetime('now')`, which has a space instead of a `T` and no
/// zone. Text comparison ranks `T` (0x54) above a space (0x20), so the two
/// formats do not sort against each other correctly.
fn parse_ts(raw: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|naive| naive.and_utc())
}

/// The `since` value to send the hub: the stored watermark, less the overlap.
fn pull_since(stored: &str) -> String {
    match parse_ts(stored) {
        Some(dt) => (dt - chrono::Duration::seconds(PULL_LOOKBACK_SECS)).to_rfc3339(),
        None => stored.to_string(),
    }
}

/// True when `candidate` is a strictly later instant than `current`.
fn ts_after(candidate: &str, current: &str) -> bool {
    match (parse_ts(candidate), parse_ts(current)) {
        (Some(a), Some(b)) => a > b,
        // Unparseable on either side — fall back to text so the watermark can
        // still advance rather than stalling the table forever.
        _ => candidate > current,
    }
}

/// Terminal→hub cycle default. LAN traffic is free; 10 s gives near-real-time stock.
const DEFAULT_INTERVAL_SECS: u64 = 10;
/// Hub housekeeping cadence default (mark-synced + daily-prune check).
const DEFAULT_HUB_INTERVAL_SECS: u64 = 300;
/// Rows per hub REST call during pull. At 28k products this reduces
/// API round-trips from 280 → 56 (5×).
const PULL_PAGE_LIMIT: usize = 500;

pub(crate) const PUSH_ORDER: &[&str] = &[
    "branches",
    "categories",
    "tax_rules",
    "products",
    "product_barcodes",
    "suppliers",
    "purchase_orders",
    "purchase_order_lines",
    "po_receipts",
    "devices",
    "roles",
    "users",
    "customers",
    "loyalty_events",
    "riders",
    "shifts",
    "sales",
    "sale_items",
    "payments",
    "refunds",
    "refund_items",
    "stock_levels",
    "stock_movements",
    "audit_logs",
    "delivery_orders",
    "product_prices",
    "product_cost_history",
    "cash_events",
];

pub(crate) const PULL_ORDER: &[&str] = &[
    "branches",
    "categories",
    "tax_rules",
    "products",
    "product_barcodes",
    "suppliers",
    "purchase_orders",
    "purchase_order_lines",
    "po_receipts",
    "devices",
    "roles",
    "users",
    "customers",
    "loyalty_events",
    "riders",
    "shifts",
    "sales",
    "sale_items",
    "payments",
    "refunds",
    "refund_items",
    "stock_levels",
    "stock_movements",
    "audit_logs",
    "delivery_orders",
    "product_prices",
    "product_cost_history",
    "cash_events",
    "app_config",
];

fn next_pull_offset(current: usize, batch_count: usize, page_limit: usize) -> Option<usize> {
    (batch_count == page_limit).then(|| current.saturating_add(batch_count))
}

fn finish_pull(total_pulled: u32, errors: Vec<String>) -> AppResult<u32> {
    if errors.is_empty() {
        Ok(total_pulled)
    } else {
        Err(AppError::Internal(errors.join("; ")))
    }
}

/// What one join-wizard sync actually managed, in each direction.
///
/// Two directions reported separately because they fail for different reasons
/// and the operator's next move differs: a push that failed leaves local sales
/// still only on this till, while a pull that failed leaves the catalogue short.
/// Collapsing both into "sync incomplete" is what made the stuck terminal
/// unreadable.
#[derive(Debug, Default, Clone)]
pub struct JoinSyncOutcome {
    pub pushed: Option<u32>,
    pub pulled: Option<u32>,
    pub push_error: Option<String>,
    pub pull_error: Option<String>,
}

impl JoinSyncOutcome {
    fn failed(error: AppError) -> Self {
        let detail = error.internal_database_detail();
        Self {
            pushed: None,
            pulled: None,
            push_error: Some(detail.clone()),
            pull_error: Some(detail),
        }
    }

    /// The first thing that went wrong, preferring the push — local rows that
    /// have not reached the hub are the half that cannot be recovered by
    /// retrying later from somewhere else.
    pub fn first_error(&self) -> Option<String> {
        self.push_error.clone().or_else(|| self.pull_error.clone())
    }
}

pub(crate) fn pending_push_sql(table: &str) -> String {
    format!(
        "SELECT * FROM {} WHERE sync_status = 'pending' ORDER BY sync_attempts ASC, rowid ASC LIMIT {}",
        table, BATCH_SIZE
    )
}

// ── Shared online state ────────────────────────────────────────────────────────

#[derive(Default)]
pub struct SyncState {
    pub online: bool,
    pub last_error: Option<String>,
    /// Consecutive sync cycles that ended in error. Resets on success.
    /// Used for adaptive backoff: 3+ → 2× interval, 6+ → 5× interval.
    pub consecutive_failures: u32,
}

pub struct SyncWorker {
    pool: SqlitePool,
    pub state: Arc<Mutex<SyncState>>,
    /// Execution guard — prevents concurrent run_once() calls.
    /// Background loop, setup wizard, and sync_trigger_now can all fire simultaneously;
    /// try_lock() at the start of run_once() ensures at-most-one execution at a time.
    running: Arc<Mutex<()>>,
}

impl SyncWorker {
    pub fn new(pool: SqlitePool) -> Arc<Self> {
        Arc::new(Self {
            pool,
            state: Arc::new(Mutex::new(SyncState::default())),
            running: Arc::new(Mutex::new(())),
        })
    }

    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Spawn background loop with supervisor restart on panic.
    /// Uses adaptive backoff: 3+ consecutive failures → 2× interval, 6+ → 5× interval.
    pub fn spawn(worker: Arc<Self>) {
        let live_worker = worker.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let Some(client) = live_worker.load_client().await else {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                };
                let own_device = live_worker.active_device_id().await.unwrap_or_default();
                let callback_worker = live_worker.clone();
                let result = client
                    .listen_hub_changes(move |event| {
                        let worker = callback_worker.clone();
                        let own_device = own_device.clone();
                        async move {
                            if event.origin_device_id != own_device {
                                tracing::debug!(
                                    "Hub event {} from {} changed {}; syncing now",
                                    event.event_id,
                                    event.origin_device_id,
                                    event.table
                                );
                                worker.run_once().await;
                            }
                        }
                    })
                    .await;
                if let Err(e) = result {
                    tracing::debug!("Hub live event stream disconnected: {e}");
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });

        let worker_outer = worker.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let worker_inner = worker_outer.clone();
                let result = std::panic::AssertUnwindSafe(async move {
                    loop {
                        // Adaptive backoff based on consecutive failure count
                        let consecutive_failures = {
                            let st = worker_inner.state.lock().await;
                            st.consecutive_failures
                        };
                        let hub_mode: bool = sqlx::query_scalar::<_, String>(
                            "SELECT value FROM app_config WHERE key='hub_mode'",
                        )
                        .fetch_optional(worker_inner.pool())
                        .await
                        .ok()
                        .flatten()
                        .map(|v| v == "1")
                        .unwrap_or(false);
                        let interval_key = if hub_mode {
                            "sync_interval_hub_secs"
                        } else {
                            "sync_interval_terminal_secs"
                        };
                        let base: u64 = sqlx::query_scalar(
                            "SELECT CAST(value AS INTEGER) FROM app_config WHERE key = ?",
                        )
                        .bind(interval_key)
                        .fetch_optional(worker_inner.pool())
                        .await
                        .ok()
                        .flatten()
                        .flatten()
                        .unwrap_or(if hub_mode {
                            DEFAULT_HUB_INTERVAL_SECS
                        } else {
                            DEFAULT_INTERVAL_SECS
                        }) as u64;
                        let wait_secs = if consecutive_failures >= 6 {
                            base * 5
                        } else if consecutive_failures >= 3 {
                            base * 2
                        } else {
                            base
                        };
                        tokio::time::sleep(Duration::from_secs(wait_secs)).await;
                        worker_inner.run_once().await;
                    }
                });
                let handle = tauri::async_runtime::spawn(result);
                match handle.await {
                    Ok(_) => {
                        tracing::warn!(
                            "Sync v2 worker loop exited unexpectedly — restarting in 5 s"
                        );
                    }
                    Err(e) => {
                        tracing::error!("Sync v2 worker panicked: {:?} — restarting in 5 s", e);
                    }
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
    }

    /// Run one push + pull cycle. Called by background loop and by sync_trigger_now.
    pub async fn run_once(&self) {
        // CRITICAL FIX: prevent concurrent execution.
        // Background loop tick, setup wizard spawn, and sync_trigger_now can all call
        // run_once() simultaneously. Two concurrent cycles sharing the same pool would
        // double-push pending rows and race on watermark advancement.
        let _run_guard = match self.running.try_lock() {
            Ok(g) => g,
            Err(_) => {
                tracing::debug!(
                    "Sync v2: run_once already in progress — skipping concurrent invocation"
                );
                return;
            }
        };

        self.run_once_locked().await;
    }

    /// Run one full cycle after waiting for any active cycle to finish.
    /// Initial terminal join uses this path so the post-join checklist is based
    /// on an actual completed sync, not a skipped concurrent invocation.
    pub async fn run_once_wait(&self) {
        let _run_guard = self.running.lock().await;
        self.run_once_locked().await;
    }

    /// Initial join path: pull the hub snapshot without pushing local seed rows.
    pub async fn pull_only_wait(&self) -> AppResult<u32> {
        let _run_guard = self.running.lock().await;
        let device_id = self.active_device_id().await?;
        let client = self
            .load_client()
            .await
            .ok_or_else(|| AppError::Validation("Hub connection is not configured".into()))?;
        let result = self.pull_changes(&client, &device_id).await;
        let mut state = self.state.lock().await;
        state.online = result.is_ok();
        state.last_error = result.as_ref().err().map(ToString::to_string);
        if result.is_ok() {
            state.consecutive_failures = 0;
        }
        drop(state);
        result
    }

    /// The join wizard's sync: hand over local work first, then take the snapshot.
    ///
    /// The wizard used to call [`Self::pull_only_wait`], which is correct for a
    /// terminal joining with an empty database and wrong for every other case.
    /// A till that has been selling — because it was paired before, or because
    /// the hub was unreachable and it kept working, which is the whole point of
    /// this being local-first — arrives here holding sales, payments and stock
    /// movements that exist nowhere else. Pulling alone can never move them, and
    /// the wizard's own readiness check refused to open the POS until nothing was
    /// pending. So the one button on screen could not produce the condition it
    /// demanded, and pressing it again could not help.
    ///
    /// Push runs first and its failure does not stop the pull: getting
    /// irreplaceable local rows to the hub is the more valuable half, but a till
    /// that cannot reach the hub to push still needs the catalogue it already
    /// has. Both outcomes are reported so the wizard can say which half failed
    /// instead of "sync incomplete".
    pub async fn join_sync_wait(&self) -> JoinSyncOutcome {
        let _run_guard = self.running.lock().await;
        let device_id = match self.active_device_id().await {
            Ok(id) => id,
            Err(e) => return JoinSyncOutcome::failed(e),
        };
        let Some(client) = self.load_client().await else {
            return JoinSyncOutcome::failed(AppError::Validation(
                "Hub connection is not configured".into(),
            ));
        };

        let push_result = self.push_pending(&client).await;
        let pull_result = self.pull_changes(&client, &device_id).await;

        let mut state = self.state.lock().await;
        state.online = true;
        match (&push_result, &pull_result) {
            (Ok(_), Ok(_)) => {
                state.last_error = None;
                state.consecutive_failures = 0;
            }
            (Err(e), _) | (_, Err(e)) => {
                state.last_error = Some(e.to_string());
                state.consecutive_failures = state.consecutive_failures.saturating_add(1);
            }
        }
        drop(state);

        JoinSyncOutcome {
            pushed: push_result.as_ref().copied().ok(),
            pulled: pull_result.as_ref().copied().ok(),
            push_error: push_result.err().map(|e| e.internal_database_detail()),
            pull_error: pull_result.err().map(|e| e.internal_database_detail()),
        }
    }

    async fn run_once_locked(&self) {
        // Guard: do not sync while first-run wizard is still in progress.
        // The wizard's Cloud step spawns run_once() to test connectivity, but
        // the subsequent pull can deactivate the local seed device (device_code
        // collision guard in apply_row/devices) before setup_wizard_complete
        // runs its reactivation check — leaving app_config_load with no device.
        // Skipping sync until setup is complete eliminates this race entirely.
        let setup_done: bool = sqlx::query_scalar(
            "SELECT COUNT(*) FROM app_config WHERE key = 'setup_complete' AND value = '1'",
        )
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .map(|n: i64| n > 0)
        .unwrap_or(false);
        if !setup_done {
            return; // Wizard not finished — skip silently
        }

        // Hub mode: this device IS the source of truth. It never pushes/pulls.
        // Its own UI writes land directly in the served DB; mark them synced so
        // the SyncChip shows clean and terminals (which pull by updated_at, not
        // sync_status) are unaffected.
        let hub_mode: bool =
            sqlx::query_scalar::<_, String>("SELECT value FROM app_config WHERE key='hub_mode'")
                .fetch_optional(&self.pool)
                .await
                .ok()
                .flatten()
                .map(|v| v == "1")
                .unwrap_or(false);
        if hub_mode {
            for table in apply::SYNC_TABLES.iter().filter(|t| **t != "app_config") {
                let exists_sql =
                    format!("SELECT 1 FROM {table} WHERE sync_status='pending' LIMIT 1");
                let has_pending = sqlx::query_scalar::<_, i64>(&exists_sql)
                    .fetch_optional(&self.pool)
                    .await
                    .ok()
                    .flatten()
                    .is_some();
                if !has_pending {
                    continue;
                }
                loop {
                    let sql = format!(
                        "UPDATE {table}
                         SET sync_status='synced'
                         WHERE rowid IN (
                           SELECT rowid FROM {table}
                           WHERE sync_status='pending'
                           LIMIT 500
                         )"
                    );
                    let affected = sqlx::query(&sql)
                        .execute(&self.pool)
                        .await
                        .map(|r| r.rows_affected())
                        .unwrap_or(0);
                    if affected < 500 {
                        break;
                    }
                }
            }
            {
                let mut st = self.state.lock().await;
                st.online = true;
                st.last_error = None;
                st.consecutive_failures = 0;
            }
            self.prune_old_data().await;
            return;
        }

        // Resolve device_id from DB each cycle
        let device_id = match self.active_device_id().await {
            Ok(id) => id,
            Err(_) => {
                // No active device yet — skip silently
                return;
            }
        };

        // Load Hub client
        let client = match self.load_client().await {
            Some(c) => c,
            None => return, // Not configured — skip silently
        };

        let push_result = self.push_pending(&client).await;
        let pull_result = self.pull_changes(&client, &device_id).await;

        // Update online status — individual table errors don't mean we're offline.
        // Only mark offline if we have no hub client at all.
        {
            let mut state = self.state.lock().await;
            state.online = true;
            match (&push_result, &pull_result) {
                (Ok(_), Ok(_)) => {
                    state.last_error = None;
                    state.consecutive_failures = 0;
                }
                (Err(e), _) | (_, Err(e)) => {
                    state.last_error = Some(e.to_string());
                    state.consecutive_failures = state.consecutive_failures.saturating_add(1);
                    // Only log transient errors as warnings; permanent errors are normal
                    // (schema mismatches, 404s on unmigrated tables, etc.)
                    if e.to_string()
                        .contains(crate::sync_v2::client::TRANSIENT_TAG)
                    {
                        tracing::warn!("Sync v2 transient error: {e}");
                    } else {
                        tracing::info!("Sync v2 cycle finished with table-level errors");
                    }
                }
            }
        }

        // DB write outside the lock — no contention with sync_status reads
        // BUG-SYNC-7: update last_pushed_at for ALL tables, not just 'sales'.
        if push_result.is_ok() && pull_result.is_ok() {
            let now = chrono::Utc::now().to_rfc3339();
            if let Err(e) = sqlx::query("UPDATE sync_watermark SET last_pushed_at = ?")
                .bind(&now)
                .execute(&self.pool)
                .await
            {
                tracing::warn!("Sync v2: failed to record last_pushed_at: {e}");
            }
        }

        // Run daily pruning pass
        self.prune_old_data().await;
    }

    // ── Load client from app_config + keyring ──────────────────────────────────

    /// Terminal mode: hub_url (app_config) + hub_store_token (OS credential store).
    /// Returns None when this device is the hub or not yet joined.
    pub async fn load_client(&self) -> Option<HttpSyncClient> {
        let url = ai_admin_repo::get_config(&self.pool, "hub_url")
            .await
            .ok()
            .flatten()
            .filter(|u| !u.is_empty())?;
        let token = secure_store::get_secret("hub_store_token").unwrap_or_default();
        if token.is_empty() {
            tracing::error!(
                "Sync v2: hub_url is set but hub_store_token is missing from the OS \
                 credential store. Re-enter the store token in Settings -> Hub."
            );
            let mut st = self.state.lock().await;
            st.online = false;
            st.last_error = Some("Store token missing — re-enter it in Settings → Hub.".into());
            return None;
        }
        let device_id =
            sqlx::query_scalar::<_, String>("SELECT value FROM app_config WHERE key='device_id'")
                .fetch_optional(&self.pool)
                .await
                .ok()
                .flatten();
        Some(HttpSyncClient::new(&url, &token, device_id.as_deref()))
    }

    /// Run one push+pull against an explicit client. Test seam: bypasses
    /// app_config/keyring lookup. Production path load_client() + run_once()
    /// delegates here.
    pub async fn run_once_with(&self, client: &HttpSyncClient, device_id: &str) {
        let _span = telemetry::instrument(ZanposSpan::SyncBatch { table_count: 0 });
        let _ = self.push_pending(client).await;
        let _ = self.pull_changes(client, device_id).await;
    }

    // ── Push pending rows table-by-table in FK-safe order ─────────────────────

    async fn push_pending(&self, client: &HttpSyncClient) -> AppResult<u32> {
        // FK-safe push order:
        // 0. Branches first — all other tables have a branch_id FK
        // 1. Master data (no transaction FKs): categories, tax_rules, products, devices, customers
        // 2. Transactions: shifts, sales, sale_items, payments, refunds, refund_items,
        //    stock_movements, audit_logs, delivery_orders, product_prices
        let mut total_pushed = 0u32;

        for table in PUSH_ORDER {
            loop {
                let sql = pending_push_sql(table);
                let rows = sqlx::query(&sql).fetch_all(&self.pool).await?;
                if rows.is_empty() {
                    break;
                }

                // Convert rows to JSON values, stripping local-only columns
                let json_rows: Vec<Value> = rows
                    .iter()
                    .map(|row| {
                        let mut map = serde_json::Map::new();
                        for col in row.columns() {
                            let col_name = col.name();
                            // Skip local-only columns that must never sync
                            if skip_on_wire(table, col_name) {
                                continue;
                            }
                            let val = value_from_row_column(row, col_name);
                            map.insert(col_name.to_string(), val);
                        }
                        // Hub schema requires updated_at NOT NULL with no DEFAULT.
                        // Fall back to created_at or now() when the local value is missing.
                        if map
                            .get("updated_at")
                            .map_or(true, |v| matches!(v, Value::Null))
                        {
                            let fallback = map
                                .get("created_at")
                                .cloned()
                                .filter(|v| !matches!(v, Value::Null))
                                .unwrap_or_else(|| Value::String(chrono::Utc::now().to_rfc3339()));
                            map.insert("updated_at".to_string(), fallback);
                        }
                        Value::Object(map)
                    })
                    .collect();

                // Collect row IDs for marking
                let id_col = pk_for_table(table);
                let row_ids: Vec<String> =
                    rows.iter().map(|r| r.get::<String, _>(id_col)).collect();

                match client.upsert_rows(table, &json_rows).await {
                    Ok(()) => {
                        self.mark_rows_synced(table, id_col, &row_ids).await;
                        self.record_table_health(table, "push", true, None, None)
                            .await;
                        total_pushed += row_ids.len() as u32;
                    }
                    Err(e) => {
                        let transient = e.to_string().contains(TRANSIENT_TAG);
                        if transient {
                            self.increment_attempts(table, id_col, &row_ids).await;
                            self.record_table_health(
                                table,
                                "push",
                                false,
                                row_ids.first().map(String::as_str),
                                Some(&e.to_string()),
                            )
                            .await;
                            tracing::warn!(
                                "Sync v2: transient error on {table}, skipping to next table: {e}"
                            );
                            // Keep network/service failures at batch level so one outage does
                            // not fan out into 50 full HTTP retry loops.
                            break;
                        } else {
                            tracing::warn!(
                                "Sync v2: permanent batch error on {table}, isolating rows: {e}"
                            );
                            let mut recovered = 0u32;
                            let mut failed_ids: Vec<String> = Vec::new();
                            for (idx, id) in row_ids.iter().enumerate() {
                                match client.upsert_rows(table, &json_rows[idx..=idx]).await {
                                    Ok(()) => {
                                        self.mark_rows_synced(
                                            table,
                                            id_col,
                                            std::slice::from_ref(id),
                                        )
                                        .await;
                                        recovered += 1;
                                    }
                                    Err(row_err) => {
                                        failed_ids.push(id.clone());
                                        self.record_sync_conflict(
                                            "sync_push_failed",
                                            table,
                                            Some(id),
                                            "warning",
                                            "Sync push row failed",
                                            &row_err.to_string(),
                                        )
                                        .await;
                                        tracing::warn!(
                                            "Sync v2: row-level push failed for {table}:{id}: {row_err}"
                                        );
                                    }
                                }
                            }
                            if !failed_ids.is_empty() {
                                self.increment_attempts(table, id_col, &failed_ids).await;
                                self.record_table_health(
                                    table,
                                    "push",
                                    false,
                                    failed_ids.first().map(String::as_str),
                                    Some(&e.to_string()),
                                )
                                .await;
                            }
                            total_pushed += recovered;
                            // Continue the table after isolating this batch; ordering by
                            // sync_attempts pushes repeatedly failing rows behind fresher work.
                            if recovered == 0 && failed_ids.len() == row_ids.len() {
                                break;
                            }
                            continue;
                        }
                    }
                }
            }
        }

        // Push app_config (business settings, flags) — has no sync columns.
        // Best-effort: a transient failure here must NOT mark the whole push cycle
        // as failed (which would block watermark advancement and trigger adaptive
        // backoff). Log and continue — the flags will retry next cycle.
        if let Err(e) = self.push_app_config(client).await {
            tracing::warn!("Sync v2: app_config push failed (non-fatal): {e}");
        }

        Ok(total_pushed)
    }

    async fn mark_rows_synced(&self, table: &str, id_col: &str, row_ids: &[String]) {
        if row_ids.is_empty() {
            return;
        }
        let placeholders = row_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "UPDATE {} SET sync_status = 'synced' WHERE {} IN ({})",
            table, id_col, placeholders
        );
        let mut q = sqlx::query(&sql);
        for id in row_ids {
            q = q.bind(id);
        }
        if let Err(e) = q.execute(&self.pool).await {
            tracing::warn!("Sync v2: failed to mark {table} rows as synced: {e}");
        }
    }

    async fn increment_attempts(&self, table: &str, id_col: &str, row_ids: &[String]) {
        for id in row_ids {
            let sql = format!(
                "UPDATE {} SET sync_attempts = sync_attempts + 1 WHERE {} = ?",
                table, id_col
            );
            let _ = sqlx::query(&sql).bind(id).execute(&self.pool).await;
        }
    }

    async fn record_table_health(
        &self,
        table: &str,
        direction: &str,
        ok: bool,
        row_id: Option<&str>,
        error: Option<&str>,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        let checksum = consistency::table_snapshot(&self.pool, table)
            .await
            .ok()
            .map(|s| s.checksum);
        let (push_at, pull_at) = match (direction, ok) {
            ("push", true) => (Some(now.as_str()), None),
            ("pull", true) => (None, Some(now.as_str())),
            _ => (None, None),
        };

        let result = sqlx::query(
            "INSERT INTO sync_table_health
               (table_name, last_push_success_at, last_pull_success_at, last_error_at,
                last_error, last_failed_row_id, retry_count, table_checksum, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(table_name) DO UPDATE SET
               last_push_success_at = COALESCE(excluded.last_push_success_at, sync_table_health.last_push_success_at),
               last_pull_success_at = COALESCE(excluded.last_pull_success_at, sync_table_health.last_pull_success_at),
               last_error_at = excluded.last_error_at,
               last_error = excluded.last_error,
               last_failed_row_id = excluded.last_failed_row_id,
               retry_count = CASE WHEN excluded.last_error IS NULL THEN 0 ELSE sync_table_health.retry_count + 1 END,
               table_checksum = COALESCE(excluded.table_checksum, sync_table_health.table_checksum),
               updated_at = excluded.updated_at",
        )
        .bind(table)
        .bind(push_at)
        .bind(pull_at)
        .bind(if ok { None } else { Some(now.as_str()) })
        .bind(if ok { None } else { error })
        .bind(if ok { None } else { row_id })
        .bind(if ok { 0 } else { 1 })
        .bind(checksum)
        .bind(&now)
        .execute(&self.pool)
        .await;
        if let Err(e) = result {
            tracing::debug!("Sync v2: sync_table_health update failed for {table}: {e}");
        }
    }

    /// Set a row aside if it has failed too many times, so its table can advance.
    ///
    /// Returns true when the row was quarantined and the caller should treat it
    /// as dealt with. The attempt count lives in the inbox rather than in this
    /// worker, so a terminal that restarts mid-retry does not forget that a row
    /// has already failed four times and start again from zero.
    ///
    /// Deliberately *not* first-failure behaviour: a dependency arriving out of
    /// order looks exactly like one that will never arrive, and most of them
    /// arrive.
    async fn quarantine_if_hopeless(&self, table: &str, row: &Value, reason: &str) -> bool {
        let Some(obj) = row.as_object() else {
            return false;
        };
        let entity_id = obj
            .get(pk_for_table(table))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if entity_id.is_empty() {
            return false;
        }

        let attempts =
            crate::sync_v2::inbox::record_failure(&self.pool, table, entity_id, obj, reason).await;
        if attempts < crate::sync_v2::dead_letter::QUARANTINE_AFTER_ATTEMPTS {
            return false;
        }

        crate::sync_v2::dead_letter::quarantine(
            &self.pool,
            table,
            entity_id,
            row,
            reason,
            attempts,
        )
        .await
        .is_ok()
    }

    async fn record_sync_conflict(
        &self,
        conflict_type: &str,
        table: &str,
        entity_id: Option<&str>,
        severity: &str,
        title: &str,
        detail: &str,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        let id_seed = format!(
            "{conflict_type}:{table}:{}:{detail}",
            entity_id.unwrap_or("")
        );
        let conflict_id = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(id_seed.as_bytes());
            hex::encode(h.finalize())
        };
        let result = sqlx::query(
            "INSERT INTO sync_conflicts
               (conflict_id, conflict_type, table_name, entity_id, severity, title, detail, status, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'open', ?)
             ON CONFLICT(conflict_id) DO UPDATE SET
               detail = excluded.detail,
               severity = excluded.severity,
               status = 'open'",
        )
        .bind(conflict_id)
        .bind(conflict_type)
        .bind(table)
        .bind(entity_id)
        .bind(severity)
        .bind(title)
        .bind(detail)
        .bind(now)
        .execute(&self.pool)
        .await;
        if let Err(e) = result {
            tracing::debug!("Sync v2: sync_conflicts insert failed for {table}: {e}");
        }
    }

    /// Upsert app_config rows to the hub (key-value pairs).
    /// SECURITY FIX: only push ALLOWED_CONFIG_KEYS — never push supabase_service_key,
    /// watermarks, or any other device-local internal state to the hub.
    async fn push_app_config(&self, client: &HttpSyncClient) -> AppResult<u32> {
        let rows = sqlx::query("SELECT key, value, updated_at FROM app_config")
            .fetch_all(&self.pool)
            .await?;

        if rows.is_empty() {
            return Ok(0);
        }

        let json_rows: Vec<Value> = rows
            .iter()
            .filter_map(|r| {
                let key: String = r.get("key");
                // FIX: same allowlist used for inbound filtering — must also apply at push
                if !ALLOWED_CONFIG_KEYS.contains(&key.as_str()) {
                    return None; // never push service keys, watermarks, setup flags, etc.
                }
                let value: String = r.get("value");
                let updated_at: String = r.get("updated_at");
                Some(serde_json::json!({"key": key, "value": value, "updated_at": updated_at}))
            })
            .collect();

        if json_rows.is_empty() {
            return Ok(0);
        }

        client.upsert_rows("app_config", &json_rows).await?;
        Ok(json_rows.len() as u32)
    }

    /// Bulk push ALL data from all tables during initial setup.
    /// Not batch-limited — designed for first-time sync to the hub.
    pub async fn push_all_bulk(&self, client: &HttpSyncClient) -> AppResult<u32> {
        let mut total_pushed = 0u32;

        for table in PUSH_ORDER {
            let sql = format!("SELECT * FROM {table}");
            let rows = sqlx::query(&sql).fetch_all(&self.pool).await?;
            if rows.is_empty() {
                continue;
            }

            let json_rows: Vec<Value> = rows
                .iter()
                .map(|row| {
                    let mut map = serde_json::Map::new();
                    for col in row.columns() {
                        let col_name = col.name();
                        if skip_on_wire(table, col_name) {
                            continue;
                        }
                        map.insert(col_name.to_string(), value_from_row_column(row, col_name));
                    }
                    if map
                        .get("updated_at")
                        .map_or(true, |v| matches!(v, Value::Null))
                    {
                        let fallback = map
                            .get("created_at")
                            .cloned()
                            .filter(|v| !matches!(v, Value::Null))
                            .unwrap_or_else(|| Value::String(chrono::Utc::now().to_rfc3339()));
                        map.insert("updated_at".to_string(), fallback);
                    }
                    Value::Object(map)
                })
                .collect();

            client.upsert_rows(table, &json_rows).await?;

            // Mark rows as synced
            let pk = pk_for_table(table);
            for row in &rows {
                let id: String = row.get(pk);
                let sql = format!("UPDATE {table} SET sync_status = 'synced' WHERE {pk} = ?");
                let _ = sqlx::query(&sql).bind(&id).execute(&self.pool).await;
            }
            total_pushed += json_rows.len() as u32;
        }

        // Push app_config + branches
        self.push_app_config(client).await?;
        // Branches now sync normally via the hub push/pull path.

        tracing::info!("Bulk initial sync: pushed {} rows to hub", total_pushed);
        Ok(total_pushed)
    }

    // ── Pull changes from hub ────────────────────────────────────────────────

    async fn pull_changes(&self, client: &HttpSyncClient, device_id: &str) -> AppResult<u32> {
        // Tables to pull from central (same set as push, but orderly)
        let mut total_pulled = 0u32;
        // Collect transient pull errors per-table so ALL tables are attempted even
        // when one fails (Bug-Pull-A: old code did `return Err(e)` on first TRANSIENT,
        // which aborted every subsequent table in the same cycle).
        let mut pull_errors: Vec<String> = Vec::new();

        for table in PULL_ORDER {
            let mut table_error: Option<String> = None;
            let mut table_failed_row: Option<String> = None;
            let mut stored_watermark = self.get_watermark(table).await.unwrap_or_default();
            if stored_watermark.is_empty() {
                stored_watermark = "1970-01-01T00:00:00Z".to_string();
            }
            // Ask for a little before the watermark. Two terminals' clocks are
            // never exactly aligned, and a row can be committed with a timestamp
            // fractionally behind one we have already passed; without an overlap
            // it would never be offered again. Re-delivery is safe — LWW ignores
            // an older row and the append-only path treats a repeat as a replay.
            let query_watermark = pull_since(&stored_watermark);
            // Anchored to the *stored* value, never the reduced one, or the
            // watermark would walk backwards by the overlap on every idle cycle.
            let mut max_applied_ts = stored_watermark.clone();
            let mut table_completed = false;

            // Offset tracking for pagination within same-timestamp rows.
            // When watermark advances (new updated_at), offset resets to 0.
            // When watermark stays the same (all rows in a full page shared
            // the same timestamp), offset increments by the batch count so the
            // next query skips already-processed rows instead of re-fetching them.
            let mut offset: usize = 0;
            let pk_col = pk_for_table(table);

            // Retry-loop: pull batches until exhausted or error
            loop {
                let rows = match client
                    .pull_rows(
                        table,
                        &query_watermark,
                        if has_origin_device_id(table) {
                            Some(device_id)
                        } else {
                            None
                        },
                        PULL_PAGE_LIMIT,
                        offset,
                        if pk_col != "id" { Some(pk_col) } else { None },
                    )
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        table_error = Some(e.to_string());
                        // BOTH transient and permanent errors skip this table and move on.
                        // Transient errors are accumulated and returned after all tables
                        // are processed — never abort the remaining tables mid-cycle.
                        if e.to_string().contains(TRANSIENT_TAG) {
                            tracing::warn!("Sync v2: transient pull error on {table}, continuing other tables: {e}");
                        }
                        break;
                    }
                };

                if rows.is_empty() {
                    table_completed = true;
                    break;
                }

                let batch_count = rows.len();
                let mut applied = 0usize;
                let mut hit_failure = false;

                for row in &rows {
                    match apply::apply_row(&self.pool, table, row).await {
                        Ok(()) => {
                            // BUG-SYNC-4: Only advance watermark past rows that were
                            // successfully applied — never skip past a failed row.
                            if let Some(ts) = row.get("updated_at").and_then(|v| v.as_str()) {
                                if ts_after(ts, &max_applied_ts) {
                                    max_applied_ts = ts.to_string();
                                }
                            }
                            applied += 1;
                        }
                        Err(e) => {
                            let row_id =
                                row.get(pk_col).and_then(|v| v.as_str()).map(str::to_string);
                            if *table == "products" && e.is_duplicate_barcode_constraint() {
                                let detail = e.internal_database_detail();
                                self.record_sync_conflict(
                                    "duplicate_barcode",
                                    table,
                                    row_id.as_deref(),
                                    "warning",
                                    "Duplicate product barcode skipped",
                                    &detail,
                                )
                                .await;
                                tracing::warn!(
                                    "Sync v2: skipped duplicate product barcode from hub; use Duplicate Products to merge catalog rows"
                                );
                                applied += 1;
                                if let Some(ts) = row.get("updated_at").and_then(|v| v.as_str()) {
                                    if ts_after(ts, &max_applied_ts) {
                                        max_applied_ts = ts.to_string();
                                    }
                                }
                                continue;
                            }
                            // DB busy (SQLITE_BUSY code 5): happens during initial bulk data
                            // load when app writes and pull writes compete for the write lock.
                            // Retry once after a short delay before halting the watermark.
                            if e.is_database_busy() {
                                tracing::debug!(
                                    "Sync v2: DB busy on {table} apply_row, retrying in 4 s"
                                );
                                tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                                match apply::apply_row(&self.pool, table, row).await {
                                    Ok(()) => {
                                        if let Some(ts) =
                                            row.get("updated_at").and_then(|v| v.as_str())
                                        {
                                            if ts_after(ts, &max_applied_ts) {
                                                max_applied_ts = ts.to_string();
                                            }
                                        }
                                        applied += 1;
                                        continue;
                                    }
                                    Err(e2) => {
                                        let retry_error = e2.internal_database_detail();
                                        if *table == "products"
                                            && e2.is_duplicate_barcode_constraint()
                                        {
                                            self.record_sync_conflict(
                                                "duplicate_barcode",
                                                table,
                                                row_id.as_deref(),
                                                "warning",
                                                "Duplicate product barcode skipped",
                                                &retry_error,
                                            )
                                            .await;
                                            tracing::warn!("Sync v2: skipped duplicate product barcode from hub after retry");
                                            if let Some(ts) =
                                                row.get("updated_at").and_then(|v| v.as_str())
                                            {
                                                if ts_after(ts, &max_applied_ts) {
                                                    max_applied_ts = ts.to_string();
                                                }
                                            }
                                            applied += 1;
                                            continue;
                                        }
                                        tracing::warn!(
                                            "Sync v2: apply_row error for {table} (retry): {e2:?} — halting watermark here"
                                        );
                                        table_error = Some(retry_error);
                                        table_failed_row = row_id.clone();
                                        self.record_sync_conflict(
                                            "sync_pull_failed",
                                            table,
                                            row_id.as_deref(),
                                            "warning",
                                            "Sync pull row failed",
                                            table_error.as_deref().unwrap_or("pull failed"),
                                        )
                                        .await;
                                        hit_failure = true;
                                        break;
                                    }
                                }
                            } else {
                                let internal_error = e.internal_database_detail();

                                // A row that has failed this many cycles is not
                                // waiting on something; it is never going to
                                // apply. Halting the watermark for it means the
                                // next cycle re-fetches it and fails again, so
                                // the table never syncs again — the row is set
                                // aside instead, in full, and the table moves on.
                                if self
                                    .quarantine_if_hopeless(table, row, &internal_error)
                                    .await
                                {
                                    if let Some(ts) =
                                        row.get("updated_at").and_then(|v| v.as_str())
                                    {
                                        if ts_after(ts, &max_applied_ts) {
                                            max_applied_ts = ts.to_string();
                                        }
                                    }
                                    applied += 1;
                                    continue;
                                }

                                let (conflict_type, conflict_title) =
                                    if e.is_foreign_key_constraint() {
                                        ("missing_dependency", "Missing synced dependency")
                                    } else {
                                        ("sync_pull_failed", "Sync pull row failed")
                                    };
                                tracing::warn!(
                                    "Sync v2: apply_row error for {table}: {e:?} — halting watermark here"
                                );
                                table_error = Some(internal_error);
                                table_failed_row = row_id.clone();
                                self.record_sync_conflict(
                                    conflict_type,
                                    table,
                                    row_id.as_deref(),
                                    "warning",
                                    conflict_title,
                                    table_error.as_deref().unwrap_or("pull failed"),
                                )
                                .await;
                                hit_failure = true;
                                break;
                            }
                        }
                    }
                }

                total_pulled += applied as u32;

                if hit_failure {
                    break;
                }
                match next_pull_offset(offset, batch_count, PULL_PAGE_LIMIT) {
                    Some(next) => offset = next,
                    None => {
                        table_completed = true;
                        break;
                    }
                }
            }
            if table_completed && table_error.is_none() {
                if let Err(e) = self.set_watermark(table, &max_applied_ts).await {
                    tracing::warn!("Sync v2: failed to set watermark for {table}: {e}");
                    table_error = Some(e.internal_database_detail());
                }
            }
            self.record_table_health(
                table,
                "pull",
                table_error.is_none(),
                table_failed_row.as_deref(),
                table_error.as_deref(),
            )
            .await;
            if let Some(error) = table_error {
                pull_errors.push(format!("{table}: {error}"));
            }
        }

        finish_pull(total_pulled, pull_errors)
    }

    // ── Watermark helpers ─────────────────────────────────────────────────────

    /// Get the sync watermark for a table (stored in app_config).
    async fn get_watermark(&self, table: &str) -> AppResult<String> {
        let key = format!("sync_v2_watermark_{table}");
        let val = ai_admin_repo::get_config(&self.pool, &key).await?;
        Ok(val.unwrap_or_default())
    }

    /// Set the sync watermark for a table.
    async fn set_watermark(&self, table: &str, ts: &str) -> AppResult<()> {
        let key = format!("sync_v2_watermark_{table}");
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(&key)
        .bind(ts)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ── Active device ─────────────────────────────────────────────────────────

    async fn active_device_id(&self) -> AppResult<String> {
        // Prefer the identity key written during setup — guaranteed to be THIS terminal's
        // device_id regardless of how many other devices sync into the local `devices` table.
        // ORDER BY device_code falls back for terminals set up before this key was introduced.
        if let Ok(Some(id)) =
            sqlx::query_scalar::<_, String>("SELECT value FROM app_config WHERE key = 'device_id'")
                .fetch_optional(&self.pool)
                .await
        {
            if !id.is_empty() {
                return Ok(id);
            }
        }
        let row = sqlx::query(
            "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("No active device configured".into()))?;
        Ok(row.get("device_id"))
    }

    // ── Daily data pruning ────────────────────────────────────────────────────

    async fn prune_old_data(&self) {
        // Only prune once per 24 hours
        let last_prune: Option<String> =
            sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'last_prune_at'")
                .fetch_optional(&self.pool)
                .await
                .ok()
                .flatten()
                .flatten();

        if let Some(ts) = last_prune.as_deref().filter(|s| !s.is_empty()) {
            if let Ok(t) = chrono::DateTime::parse_from_rfc3339(ts) {
                let elapsed = chrono::Utc::now().signed_duration_since(t);
                if elapsed.num_hours() < 24 {
                    return;
                }
            }
        }

        // Read retention config (defaults: 90 days sales, 30 days logs)
        let sales_days: i64 = sqlx::query_scalar(
            "SELECT CAST(value AS INTEGER) FROM app_config WHERE key = 'retention_days_sales'",
        )
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .flatten()
        .unwrap_or(90);

        let log_days: i64 = sqlx::query_scalar(
            "SELECT CAST(value AS INTEGER) FROM app_config WHERE key = 'retention_days_logs'",
        )
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .flatten()
        .unwrap_or(30);

        let sales_cutoff = (chrono::Utc::now() - chrono::Duration::days(sales_days)).to_rfc3339();
        let log_cutoff = (chrono::Utc::now() - chrono::Duration::days(log_days)).to_rfc3339();

        // 1. FK-safe delete: sale_items + payments before sales
        let _ = sqlx::query(
            "DELETE FROM sale_items WHERE sale_id IN (
               SELECT sale_id FROM sales WHERE sync_status = 'synced' AND sold_at < ?
             )",
        )
        .bind(&sales_cutoff)
        .execute(&self.pool)
        .await;

        let _ = sqlx::query(
            "DELETE FROM payments WHERE sale_id IN (
               SELECT sale_id FROM sales WHERE sync_status = 'synced' AND sold_at < ?
             )",
        )
        .bind(&sales_cutoff)
        .execute(&self.pool)
        .await;

        // 2. Sales themselves (only synced rows)
        let _ = sqlx::query("DELETE FROM sales WHERE sync_status = 'synced' AND sold_at < ?")
            .bind(&sales_cutoff)
            .execute(&self.pool)
            .await;

        // 3. Audit logs older than log_days
        let _ =
            sqlx::query("DELETE FROM audit_logs WHERE created_at < ? AND sync_status = 'synced'")
                .bind(&log_cutoff)
                .execute(&self.pool)
                .await;

        // 4. Stock movements older than log_days
        let _ = sqlx::query(
            "DELETE FROM stock_movements WHERE created_at < ? AND sync_status = 'synced'",
        )
        .bind(&log_cutoff)
        .execute(&self.pool)
        .await;

        // 5. WAL checkpoint + VACUUM
        if let Err(e) = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.pool)
            .await
        {
            tracing::warn!("prune v2: WAL checkpoint failed (disk full or DB locked?): {e}");
        }
        if let Err(e) = sqlx::query("VACUUM").execute(&self.pool).await {
            tracing::warn!("prune v2: VACUUM failed (disk full or DB locked?): {e}");
        }

        // Record prune timestamp
        let now = chrono::Utc::now().to_rfc3339();
        let _ = sqlx::query(
            "INSERT INTO app_config(key, value, updated_at) VALUES ('last_prune_at', ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await;

        tracing::info!(
            "DB prune v2 complete — sales cutoff: {sales_cutoff}, log cutoff: {log_cutoff}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{
        finish_pull, next_pull_offset, parse_ts, pending_push_sql, pull_since, ts_after,
        PULL_LOOKBACK_SECS, PULL_ORDER, PUSH_ORDER,
    };

    #[test]
    fn pending_push_query_keeps_retrying_high_attempt_rows() {
        let sql = pending_push_sql("sales");
        assert!(sql.contains("sync_status = 'pending'"));
        assert!(
            !sql.contains("sync_attempts <"),
            "high-attempt rows must remain eligible after the root cause is fixed"
        );
    }

    #[test]
    fn role_rows_are_covered_by_sync_v2_tables() {
        assert!(crate::sync_v2::apply::SYNC_TABLES.contains(&"roles"));
        assert_eq!(crate::sync_v2::apply::pk_for_table("roles"), "role_id");
    }

    #[test]
    fn full_pull_page_keeps_paging_even_when_watermark_timestamp_changes() {
        assert_eq!(next_pull_offset(0, 500, 500), Some(500));
        assert_eq!(next_pull_offset(500, 500, 500), Some(1000));
        assert_eq!(next_pull_offset(1000, 37, 500), None);
    }

    #[test]
    fn product_barcodes_are_transferred_with_the_catalog() {
        assert!(PUSH_ORDER.contains(&"product_barcodes"));
        assert!(PULL_ORDER.contains(&"product_barcodes"));
        assert!(
            PULL_ORDER
                .iter()
                .position(|table| *table == "product_barcodes")
                > PULL_ORDER.iter().position(|table| *table == "products")
        );
        assert!(
            PULL_ORDER.iter().position(|table| *table == "stock_levels")
                < PULL_ORDER
                    .iter()
                    .position(|table| *table == "stock_movements"),
            "the canonical stock row must exist before movement replay updates it"
        );
    }

    #[test]
    fn any_table_apply_failure_fails_the_pull_cycle() {
        assert_eq!(finish_pull(42, Vec::new()).unwrap(), 42);
        let error = finish_pull(42, vec!["products: missing category".to_string()])
            .unwrap_err()
            .to_string();
        assert!(error.contains("products: missing category"));
    }

    // The two timestamp formats in the schema do not sort against each other as
    // text: 'T' (0x54) outranks the space (0x20), so an RFC3339 row always looks
    // later than an importer-format one regardless of the actual instant.
    #[test]
    fn watermark_advances_by_instant_not_by_text() {
        // Same instant, both formats — neither is "after" the other.
        assert!(!ts_after("2026-01-01 10:00:00", "2026-01-01T10:00:00Z"));
        assert!(!ts_after("2026-01-01T10:00:00Z", "2026-01-01 10:00:00"));

        // An importer row an hour later must win, though text ranks it lower.
        assert!(ts_after("2026-01-01 11:00:00", "2026-01-01T10:00:00Z"));
        assert!(
            "2026-01-01 11:00:00" < "2026-01-01T10:00:00Z",
            "text disagrees"
        );

        // And an RFC3339 row an hour earlier must lose, though text ranks it higher.
        assert!(!ts_after("2026-01-01T09:00:00Z", "2026-01-01 10:00:00"));
    }

    #[test]
    fn ts_after_is_strict() {
        assert!(!ts_after("2026-01-01T10:00:00Z", "2026-01-01T10:00:00Z"));
        assert!(ts_after(
            "2026-01-01T10:00:00.500Z",
            "2026-01-01T10:00:00.400Z"
        ));
    }

    // The overlap must reach back, and must never be applied to the value we
    // store — anchoring max_applied_ts to the reduced figure would walk the
    // watermark backwards by two seconds on every idle cycle.
    #[test]
    fn pull_since_reaches_back_without_moving_the_stored_watermark() {
        let stored = "2026-01-01T10:00:10Z";
        let since = pull_since(stored);
        assert!(
            ts_after(stored, &since),
            "the query must start before the stored watermark"
        );
        let gap = parse_ts(stored).unwrap() - parse_ts(&since).unwrap();
        assert_eq!(gap.num_seconds(), PULL_LOOKBACK_SECS);
    }

    #[test]
    fn pull_since_handles_both_formats_and_passes_through_junk() {
        assert!(parse_ts(&pull_since("2026-01-01 10:00:10")).is_some());
        assert_eq!(pull_since("not-a-timestamp"), "not-a-timestamp");
    }
}
