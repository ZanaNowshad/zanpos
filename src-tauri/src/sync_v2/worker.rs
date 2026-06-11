use crate::db::repositories::ai_admin_repo;
use crate::errors::{AppError, AppResult};
use crate::secure_store;
use crate::sync_v2::apply::{self, ALLOWED_CONFIG_KEYS, has_origin_device_id, pk_for_table, should_skip_column, value_from_row_column};
use crate::sync_v2::client::HttpSyncClient;
use serde_json::Value;
use sqlx::Column;
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Duration;

pub const TRANSIENT_TAG: &str = "[TRANSIENT]";
const BATCH_SIZE: i64 = 50;
/// Terminal→hub cycle. LAN traffic is free; 10 s gives near-real-time stock.
const INTERVAL_SECS: u64 = 10;
/// Hub housekeeping cadence (mark-synced + daily-prune check).
const HUB_INTERVAL_SECS: u64 = 300;
const MAX_ATTEMPTS: i64 = 10;
/// Rows per hub REST call during pull. At 28k products this reduces
/// API round-trips from 280 → 56 (5×).
const PULL_PAGE_LIMIT: usize = 500;

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

    pub(crate) fn pool(&self) -> &SqlitePool { &self.pool }

    /// Spawn background loop with supervisor restart on panic.
    /// Uses adaptive backoff: 3+ consecutive failures → 2× interval, 6+ → 5× interval.
    pub fn spawn(worker: Arc<Self>) {
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
                            "SELECT value FROM app_config WHERE key='hub_mode'")
                            .fetch_optional(worker_inner.pool()).await.ok().flatten()
                            .map(|v| v == "1").unwrap_or(false);
                        let base = if hub_mode { HUB_INTERVAL_SECS } else { INTERVAL_SECS };
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
                        tracing::warn!("Sync v2 worker loop exited unexpectedly — restarting in 5 s");
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
                tracing::debug!("Sync v2: run_once already in progress — skipping concurrent invocation");
                return;
            }
        };

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
        let hub_mode: bool = sqlx::query_scalar::<_, String>(
            "SELECT value FROM app_config WHERE key='hub_mode'")
            .fetch_optional(&self.pool).await.ok().flatten()
            .map(|v| v == "1").unwrap_or(false);
        if hub_mode {
            for table in apply::SYNC_TABLES.iter().filter(|t| **t != "app_config") {
                let sql = format!(
                    "UPDATE {table} SET sync_status='synced' WHERE sync_status='pending'");
                let _ = sqlx::query(&sql).execute(&self.pool).await;
            }
            {
                let mut st = self.state.lock().await;
                st.online = true; st.last_error = None; st.consecutive_failures = 0;
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
        // Only mark offline if we have no Supabase client at all.
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
                    if e.to_string().contains(crate::sync_v2::client::TRANSIENT_TAG) {
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
            if let Err(e) = sqlx::query(
                "UPDATE sync_watermark SET last_pushed_at = ?",
            )
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
        let url = ai_admin_repo::get_config(&self.pool, "hub_url").await.ok().flatten()
            .filter(|u| !u.is_empty())?;
        let token = secure_store::get_secret("hub_store_token").unwrap_or_default();
        if token.is_empty() {
            tracing::error!(
                "Sync v2: hub_url is set but hub_store_token is missing from the OS \
                 credential store. Re-enter the store token in Settings -> Hub.");
            let mut st = self.state.lock().await;
            st.online = false;
            st.last_error = Some("Store token missing — re-enter it in Settings → Hub.".into());
            return None;
        }
        let device_id = sqlx::query_scalar::<_, String>(
            "SELECT value FROM app_config WHERE key='device_id'")
            .fetch_optional(&self.pool).await.ok().flatten();
        Some(HttpSyncClient::new(&url, &token, device_id.as_deref()))
    }

    /// Run one push+pull against an explicit client. Test seam: bypasses
    /// app_config/keyring lookup. Production path load_client() + run_once()
    /// delegates here.
    pub async fn run_once_with(&self, client: &HttpSyncClient, device_id: &str) {
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
        let push_order: &[&str] = &[
            "branches",   // Bug-Push-B: was missing — local branch edits never reached Supabase
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
            "stock_levels",  // Bug-Push-SL: was missing — stock_levels has sync_status but was never pushed
            "audit_logs",
            "delivery_orders",
            "product_prices",
            "cash_events",   // Bug-Push-CE: was missing — cash_events has sync_status but was never pushed
        ];

        let mut total_pushed = 0u32;

        for table in push_order {
            loop {
                let sql = format!(
                    "SELECT * FROM {} WHERE sync_status = 'pending' AND sync_attempts < {} LIMIT {}",
                    table, MAX_ATTEMPTS, BATCH_SIZE
                );
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
                            if should_skip_column(table, col_name) {
                                continue;
                            }
                            let val = value_from_row_column(row, col_name);
                            map.insert(col_name.to_string(), val);
                        }
                        // Supabase schema requires updated_at NOT NULL with no DEFAULT.
                        // Fall back to created_at or now() when the local value is missing.
                        if map.get("updated_at").map_or(true, |v| matches!(v, Value::Null)) {
                            let fallback = map.get("created_at")
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
                let row_ids: Vec<String> = rows
                    .iter()
                    .map(|r| r.get::<String, _>(id_col))
                    .collect();

                match client.upsert_rows(table, &json_rows).await {
                    Ok(()) => {
                        // Mark all rows synced atomically in one UPDATE … IN (…) statement.
                        // A single statement is crash-safe: either all are marked or none are,
                        // preventing a partial-mark state that would cause redundant re-pushes.
                        let placeholders = row_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                        let sql = format!(
                            "UPDATE {} SET sync_status = 'synced' WHERE {} IN ({})",
                            table, id_col, placeholders
                        );
                        let mut q = sqlx::query(&sql);
                        for id in &row_ids {
                            q = q.bind(id);
                        }
                        if let Err(e) = q.execute(&self.pool).await {
                            tracing::warn!("Sync v2: failed to mark {table} batch as synced: {e}");
                        }
                        total_pushed += row_ids.len() as u32;
                    }
                    Err(e) => {
                        let transient = e.to_string().contains(TRANSIENT_TAG);
                        // Increment attempt count
                        for id in &row_ids {
                            let sql = format!(
                                "UPDATE {} SET sync_attempts = sync_attempts + 1 WHERE {} = ?",
                                table, id_col
                            );
                            let _ = sqlx::query(&sql).bind(id).execute(&self.pool).await;
                        }

                        if transient {
                            tracing::warn!("Sync v2: transient error on {table}, skipping to next table: {e}");
                        } else {
                            tracing::warn!("Sync v2: permanent error on {table}, skipping to next table: {e}");
                        }
                        // Always break inner loop and continue to next table.
                        // Never abort the entire push — one stuck table must not
                        // block the remaining tables.
                        break;
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

    /// Upsert app_config rows to Supabase (key-value pairs).
    /// SECURITY FIX: only push ALLOWED_CONFIG_KEYS — never push supabase_service_key,
    /// watermarks, or any other device-local internal state to the cloud.
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
    /// Not batch-limited — designed for first-time sync to Supabase.
    pub async fn push_all_bulk(&self, client: &HttpSyncClient) -> AppResult<u32> {
        let push_order: &[&str] = &[
            "branches",   // Bug-Push-C: was missing from bulk push — Terminal 2 never pushed branch to Supabase
            "categories", "tax_rules", "products", "devices", "users", "customers",
            "shifts", "sales", "sale_items", "payments", "refunds", "refund_items",
            "stock_movements", "stock_levels",  // Bug-Push-SL: stock_levels was missing from bulk push
            "audit_logs", "delivery_orders", "product_prices",
            "cash_events",  // Bug-Push-CE: was missing from bulk push — cash events never reached Supabase
        ];

        let mut total_pushed = 0u32;

        for table in push_order {
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
                        if should_skip_column(table, col_name) {
                            continue;
                        }
                        map.insert(col_name.to_string(), value_from_row_column(row, col_name));
                    }
                    if map.get("updated_at").map_or(true, |v| matches!(v, Value::Null)) {
                        let fallback = map.get("created_at")
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

        tracing::info!("Bulk initial sync: pushed {} rows to Supabase", total_pushed);
        Ok(total_pushed)
    }

    // ── Pull changes from Supabase ────────────────────────────────────────────

    async fn pull_changes(
        &self,
        client: &HttpSyncClient,
        device_id: &str,
    ) -> AppResult<u32> {
        // Tables to pull from central (same set as push, but orderly)
        let pull_tables: &[&str] = &[
            "branches",
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
            "stock_levels",   // Fix-Pull-SL: was missing from pull_tables — remote stock changes were never applied locally
            "audit_logs",
            "delivery_orders",
            "product_prices",
            "cash_events",    // Fix-Pull-CE: was missing from pull_tables — remote cash events were never applied locally
            "app_config",
        ];

        let mut total_pulled = 0u32;
        // Collect transient pull errors per-table so ALL tables are attempted even
        // when one fails (Bug-Pull-A: old code did `return Err(e)` on first TRANSIENT,
        // which aborted every subsequent table in the same cycle).
        let mut transient_errors: Vec<String> = Vec::new();

        for table in pull_tables {
            let mut watermark = self.get_watermark(table).await.unwrap_or_default();
            if watermark.is_empty() {
                watermark = "1970-01-01T00:00:00Z".to_string();
            }

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
                        &watermark,
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
                        // BOTH transient and permanent errors skip this table and move on.
                        // Transient errors are accumulated and returned after all tables
                        // are processed — never abort the remaining tables mid-cycle.
                        if e.to_string().contains(TRANSIENT_TAG) {
                            tracing::warn!("Sync v2: transient pull error on {table}, continuing other tables: {e}");
                            transient_errors.push(e.to_string());
                        }
                        break;
                    }
                };

                if rows.is_empty() {
                    break;
                }

                let batch_count = rows.len();
                let mut max_ts = watermark.clone();
                let mut applied = 0usize;
                let mut hit_failure = false;

                for row in &rows {
                                match apply::apply_row(&self.pool, table, row).await {
                        Ok(()) => {
                            // BUG-SYNC-4: Only advance watermark past rows that were
                            // successfully applied — never skip past a failed row.
                            if let Some(ts) = row.get("updated_at").and_then(|v| v.as_str()) {
                                if ts > max_ts.as_str() {
                                    max_ts = ts.to_string();
                                }
                            }
                            applied += 1;
                        }
                        Err(e) => {
                            // DB busy (SQLITE_BUSY code 5): happens during initial bulk data
                            // load when app writes and pull writes compete for the write lock.
                            // Retry once after a short delay before halting the watermark.
                            if e.to_string().contains("database is locked") {
                                tracing::debug!(
                                    "Sync v2: DB busy on {table} apply_row, retrying in 4 s"
                                );
                                tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                    match apply::apply_row(&self.pool, table, row).await {
                                    Ok(()) => {
                                        if let Some(ts) = row.get("updated_at").and_then(|v| v.as_str()) {
                                            if ts > max_ts.as_str() {
                                                max_ts = ts.to_string();
                                            }
                                        }
                                        applied += 1;
                                        continue;
                                    }
                                    Err(e2) => {
                                        tracing::warn!(
                                            "Sync v2: apply_row error for {table} (retry): {e2} — halting watermark here"
                                        );
                                        hit_failure = true;
                                        break;
                                    }
                                }
                            } else {
                                tracing::warn!(
                                    "Sync v2: apply_row error for {table}: {e} — halting watermark here"
                                );
                                hit_failure = true;
                                break;
                            }
                        }
                    }
                }

                total_pulled += applied as u32;

                // Advance offset: if watermark changed, reset to 0; otherwise
                // skip already-processed rows via offset pagination (avoids
                // the gt-based boundary bug when >PULL_PAGE_LIMIT rows share
                // the same updated_at timestamp).
                let watermark_advanced = max_ts != watermark;
                watermark = max_ts;
                if watermark_advanced {
                    offset = 0;
                } else {
                    offset = offset.saturating_add(batch_count);
                }

                // Persist watermark
                if let Err(e) = self.set_watermark(table, &watermark).await {
                    tracing::warn!("Sync v2: failed to set watermark for {table}: {e}");
                }

                // Stop this table if we hit a failure or got a partial page
                if hit_failure || batch_count < PULL_PAGE_LIMIT {
                    break;
                }
            }
        }

        // Return accumulated transient errors after ALL tables have been attempted.
        // This surfaces the error in the UI status while ensuring no table was skipped.
        if !transient_errors.is_empty() {
            return Err(AppError::Internal(transient_errors.join("; ")));
        }

        Ok(total_pulled)
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
        if let Ok(Some(id)) = sqlx::query_scalar::<_, String>(
            "SELECT value FROM app_config WHERE key = 'device_id'",
        )
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

        let sales_cutoff =
            (chrono::Utc::now() - chrono::Duration::days(sales_days)).to_rfc3339();
        let log_cutoff =
            (chrono::Utc::now() - chrono::Duration::days(log_days)).to_rfc3339();

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
        let _ = sqlx::query(
            "DELETE FROM sales WHERE sync_status = 'synced' AND sold_at < ?",
        )
        .bind(&sales_cutoff)
        .execute(&self.pool)
        .await;

        // 3. Audit logs older than log_days
        let _ = sqlx::query(
            "DELETE FROM audit_logs WHERE created_at < ? AND sync_status = 'synced'",
        )
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
