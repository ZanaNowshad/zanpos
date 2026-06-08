use crate::db::repositories::ai_admin_repo;
use crate::errors::{AppError, AppResult};
use crate::secure_store;
use crate::sync_v2::client::SupabaseClient;
use serde_json::Value;
use sqlx::Column;
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Duration;

pub const TRANSIENT_TAG: &str = "[TRANSIENT]";
const BATCH_SIZE: i64 = 50;
const INTERVAL_SECS: u64 = 30;
const MAX_ATTEMPTS: i64 = 10;
const PULL_PAGE_LIMIT: usize = 100;

/// app_config keys that are allowed to sync across devices.
const ALLOWED_CONFIG_KEYS: &[&str] = &[
    "flag_allow_negative_stock",
    "flag_require_discount_reason",
    "flag_cashier_can_discount",
    "flag_auto_print_receipt",
    "whatsapp_benefit_number",
    "reports_device_scope",
];

const STOCK_DRIFT_TOLERANCE: f64 = 0.001;

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
}

impl SyncWorker {
    pub fn new(pool: SqlitePool) -> Arc<Self> {
        Arc::new(Self {
            pool,
            state: Arc::new(Mutex::new(SyncState::default())),
        })
    }

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
                        let wait_secs = if consecutive_failures >= 6 {
                            INTERVAL_SECS * 5
                        } else if consecutive_failures >= 3 {
                            INTERVAL_SECS * 2
                        } else {
                            INTERVAL_SECS
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

        // Resolve device_id from DB each cycle
        let device_id = match self.active_device_id().await {
            Ok(id) => id,
            Err(_) => {
                // No active device yet — skip silently
                return;
            }
        };

        // Load Supabase client
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
        if push_result.is_ok() && pull_result.is_ok() {
            let now = chrono::Utc::now().to_rfc3339();
            if let Err(e) = sqlx::query(
                "UPDATE sync_watermark SET last_pushed_at = ? WHERE table_name = 'sales'",
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

    pub async fn load_client(&self) -> Option<SupabaseClient> {
        let url = match ai_admin_repo::get_config(&self.pool, "supabase_url").await {
            Ok(Some(u)) => u,
            Ok(None) => return None,
            Err(e) => {
                tracing::error!("Sync v2: failed to read supabase_url from config: {e}");
                return None;
            }
        };

        // Two-phase key lookup: OS credential store first, then DB fallback
        let key = {
            let from_os = secure_store::get_secret("supabase_service_key")
                .unwrap_or_default();
            if !from_os.is_empty() {
                from_os
            } else {
                ai_admin_repo::get_config(&self.pool, "supabase_service_key")
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_default()
            }
        };

        if url.is_empty() {
            return None;
        }
        if key.is_empty() {
            tracing::error!(
                "Sync v2: Supabase URL is configured but the service key could not be read \
                 from the OS credential store OR the DB fallback. Sync is HALTED until \
                 the key is re-entered in Back Office -> Sync."
            );
            let mut st = self.state.lock().await;
            st.online = false;
            st.last_error = Some(
                "Supabase key unreadable — re-enter it in Back Office -> Sync.".into(),
            );
            return None;
        }
        Some(SupabaseClient::new(&url, &key))
    }

    // ── Push pending rows table-by-table in FK-safe order ─────────────────────

    async fn push_pending(&self, client: &SupabaseClient) -> AppResult<u32> {
        // FK-safe push order:
        // 1. Master data (no transaction FKs): categories, tax_rules, products, devices, customers
        // 2. Transactions: shifts, sales, sale_items, payments, refunds, refund_items,
        //    stock_movements, audit_logs, delivery_orders, product_prices
        let push_order: &[&str] = &[
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
            "audit_logs",
            "delivery_orders",
            "product_prices",
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
                        // Mark rows as synced
                        for id in &row_ids {
                            let sql = format!(
                                "UPDATE {} SET sync_status = 'synced' WHERE {} = ?",
                                table, id_col
                            );
                            if let Err(e) =
                                sqlx::query(&sql).bind(id).execute(&self.pool).await
                            {
                                tracing::warn!(
                                    "Sync v2: failed to mark {table} {id} as synced: {e}"
                                );
                            }
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

        // Push app_config (business settings, flags) — has no sync columns
        if let Err(e) = self.push_app_config(client).await {
            if e.to_string().contains(TRANSIENT_TAG) {
                return Err(e);
            }
        }

        Ok(total_pushed)
    }

    /// Upsert ALL app_config rows to Supabase (key-value pairs).
    /// app_config has no sync_status/sync_attempts — always push all.
    async fn push_app_config(&self, client: &SupabaseClient) -> AppResult<u32> {
        let rows = sqlx::query("SELECT key, value, updated_at FROM app_config")
            .fetch_all(&self.pool)
            .await?;

        if rows.is_empty() {
            return Ok(0);
        }

        let json_rows: Vec<Value> = rows
            .iter()
            .map(|r| {
                let key: String = r.get("key");
                let value: String = r.get("value");
                let updated_at: String = r.get("updated_at");
                serde_json::json!({"key": key, "value": value, "updated_at": updated_at})
            })
            .collect();

        client.upsert_rows("app_config", &json_rows).await?;
        Ok(json_rows.len() as u32)
    }

    /// Bulk push ALL data from all tables during initial setup.
    /// Not batch-limited — designed for first-time sync to Supabase.
    pub async fn push_all_bulk(&self, client: &SupabaseClient) -> AppResult<u32> {
        let push_order: &[&str] = &[
            "categories", "tax_rules", "products", "devices", "users", "customers",
            "shifts", "sales", "sale_items", "payments", "refunds", "refund_items",
            "stock_movements", "audit_logs", "delivery_orders", "product_prices",
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
        // Branches pushed separately via upsert_branch in lib.rs

        tracing::info!("Bulk initial sync: pushed {} rows to Supabase", total_pushed);
        Ok(total_pushed)
    }

    // ── Pull changes from Supabase ────────────────────────────────────────────

    async fn pull_changes(
        &self,
        client: &SupabaseClient,
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
            "audit_logs",
            "delivery_orders",
            "product_prices",
            "app_config",
        ];

        let mut total_pulled = 0u32;

        for table in pull_tables {
            let mut watermark = self.get_watermark(table).await.unwrap_or_default();
            if watermark.is_empty() {
                watermark = "1970-01-01T00:00:00Z".to_string();
            }

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
                    )
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        if e.to_string().contains(TRANSIENT_TAG) {
                            return Err(e);
                        }
                        // Permanent pull error: skip table
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
                    // Track max updated_at for watermark advance
                    if let Some(ts) = row.get("updated_at").and_then(|v| v.as_str()) {
                        if ts > max_ts.as_str() {
                            max_ts = ts.to_string();
                        }
                    }

                    match self.apply_row(table, row).await {
                        Ok(()) => {
                            applied += 1;
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Sync v2: apply_row error for {table}: {e} — halting watermark here"
                            );
                            hit_failure = true;
                            break;
                        }
                    }
                }

                total_pulled += applied as u32;
                watermark = max_ts;

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

        Ok(total_pulled)
    }

    // ── Apply a single row to the local database ──────────────────────────────

    async fn apply_row(&self, table: &str, row: &Value) -> AppResult<()> {
        let obj = row.as_object().ok_or_else(|| {
            AppError::Internal("apply_row: row is not a JSON object".into())
        })?;

        match table {
            // ── Mutable master data (LWW by updated_at) ──────────────────────
            "categories" => {
                self.apply_lww("categories", "category_id", obj, &[]).await
            }
            "tax_rules" => {
                self.apply_lww("tax_rules", "tax_rule_id", obj, &[]).await
            }
            "products" => {
                self.apply_lww("products", "product_id", obj, &[]).await
            }
            "devices" => {
                // Handle device_code collision: soft-deactivate local stub so
                // we don't orphan any references. Hard-DELETE would lose data.
                if let (Some(device_id), Some(branch_id), Some(device_code)) = (
                    obj.get("device_id").and_then(|v| v.as_str()),
                    obj.get("branch_id").and_then(|v| v.as_str()),
                    obj.get("device_code").and_then(|v| v.as_str()),
                ) {
                    let now = chrono::Utc::now().to_rfc3339();
                    let _ = sqlx::query(
                        "UPDATE devices SET is_active = 0, deleted_at = ? WHERE branch_id = ? AND device_code = ? AND device_id <> ?",
                    )
                    .bind(&now)
                    .bind(branch_id)
                    .bind(device_code)
                    .bind(device_id)
                    .execute(&self.pool)
                    .await;
                }
                self.apply_lww("devices", "device_id", obj, &[]).await
            }
            "branches" => {
                // branches lacks sync_status/sync_attempts; apply as LWW directly.
                self.apply_lww("branches", "branch_id", obj, &[]).await
            }
            "shifts" => {
                self.apply_lww("shifts", "shift_id", obj, &[]).await
            }
            "delivery_orders" => {
                self.apply_lww("delivery_orders", "delivery_id", obj, &[]).await
            }

            // ── Users (LWW but exclude pin_hash) ─────────────────────────────
            "users" => {
                // Handle username collision: soft-deactivate local stub so
                // FK references (refunds, sales, audit) survive. Hard-DELETE
                // would orphan receipts and break refund lookups.
                if let (Some(user_id), Some(username)) = (
                    obj.get("user_id").and_then(|v| v.as_str()),
                    obj.get("username").and_then(|v| v.as_str()),
                ) {
                    let now = chrono::Utc::now().to_rfc3339();
                    let _ = sqlx::query(
                        "UPDATE users SET is_active = 0, deleted_at = ? WHERE username = ? AND user_id <> ?",
                    )
                    .bind(&now)
                    .bind(username)
                    .bind(user_id)
                    .execute(&self.pool)
                    .await;
                }

                // Normalise the incoming row before any INSERT:
                // 1. branch_id: NOT NULL locally but absent from central Supabase schema.
                //    Provide fallback from the active branch so fresh INSERTs don't fail.
                // 2. pin_hash: NOT NULL locally but absent/null in Supabase (local-only
                //    secret). Use a sentinel that cannot match any real argon2id hash so
                //    the row can be inserted for FK integrity without granting login access.
                //    The ON CONFLICT path excludes pin_hash from SET so existing local
                //    credentials are always preserved.
                let mut obj_norm = obj.clone();

                if !obj_norm.contains_key("branch_id") {
                    let fallback_branch: Option<String> = sqlx::query_scalar(
                        "SELECT branch_id FROM branches WHERE is_active=1 LIMIT 1",
                    )
                    .fetch_optional(&self.pool)
                    .await
                    .ok()
                    .flatten();
                    if let Some(b) = fallback_branch {
                        obj_norm.insert("branch_id".to_string(), Value::String(b));
                    }
                }

                if !obj_norm.contains_key("pin_hash")
                    || obj_norm.get("pin_hash") == Some(&Value::Null)
                {
                    // Sentinel: valid UTF-8, not a real argon2id hash — cannot match any PIN.
                    obj_norm.insert(
                        "pin_hash".to_string(),
                        Value::String("*REMOTE-ONLY*".to_string()),
                    );
                }

                self.apply_lww("users", "user_id", &obj_norm, &["pin_hash"]).await
            }

            // ── Customers (LWW + loyalty_points GREATEST) ────────────────────
            "customers" => {
                self.apply_customer(obj).await
            }

            // ── Append-only (INSERT OR IGNORE) ──────────────────────────────
            "sales" | "sale_items" | "payments" | "refunds"
            | "refund_items" | "stock_movements" | "audit_logs"
            | "product_prices" => {
                self.apply_append_only(table, obj).await
            }

            // ── app_config (whitelisted keys only) ──────────────────────────
            "app_config" => {
                let key = obj.get("key").and_then(|v| v.as_str()).unwrap_or("");
                if ALLOWED_CONFIG_KEYS.contains(&key) {
                    let value = obj.get("value").and_then(|v| v.as_str()).unwrap_or("");
                    let now = chrono::Utc::now().to_rfc3339();
                    sqlx::query(
                        "INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)
                         ON CONFLICT(key) DO UPDATE SET
                           value      = excluded.value,
                           updated_at = excluded.updated_at",
                    )
                    .bind(key)
                    .bind(value)
                    .bind(&now)
                    .execute(&self.pool)
                    .await?;
                }
                Ok(())
            }

            other => {
                tracing::warn!("Sync v2: unknown table '{}', skipping apply", other);
                Ok(())
            }
        }
    }

    /// Apply a last-write-wins row: INSERT ON CONFLICT(id) DO UPDATE SET ...
    /// WHERE updated_at < excluded.updated_at
    ///
    /// `exclude_cols` lists columns to skip in the SET clause (e.g. pin_hash on users).
    async fn apply_lww(
        &self,
        table: &str,
        pk: &str,
        obj: &serde_json::Map<String, Value>,
        exclude_cols: &[&str],
    ) -> AppResult<()> {
        let cols: Vec<&String> = obj
            .keys()
            .filter(|k| {
                *k != "sync_status" && *k != "sync_attempts"
                    && !matches!(obj.get(*k), Some(Value::Null))
            })
            .collect();

        if cols.is_empty() {
            return Ok(());
        }

        // Build INSERT (col1, col2, ..., sync_status) VALUES (v1, v2, ..., 'synced')
        let col_list = format!("{}, sync_status", cols.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(", "));
        let val_list = format!("{}, 'synced'", cols
            .iter()
            .map(|c| json_to_sql_literal(&obj[*c]))
            .collect::<Vec<_>>()
            .join(", "));

        // Build SET clause: col = excluded.col, ...
        let set_parts: Vec<String> = cols
            .iter()
            .filter(|c| **c != pk && !exclude_cols.contains(&c.as_str()))
            .map(|c| format!("{} = excluded.{}", c, c))
            .collect();

        let set_clause = set_parts.join(", ");

        let has_updated_at = obj.contains_key("updated_at");

        // Always set sync_status = 'synced' on pulled rows so they aren't
        // picked up as pending on the next push cycle.
        let set_with_sync = if set_clause.is_empty() {
            "sync_status = 'synced'".to_string()
        } else {
            format!("{}, sync_status = 'synced'", set_clause)
        };

        let sql = if has_updated_at && !set_with_sync.is_empty() {
            format!(
                "INSERT INTO {} ({}) VALUES ({})
                 ON CONFLICT({}) DO UPDATE SET {}
                 WHERE datetime({0}.updated_at) < datetime(excluded.updated_at)",
                table, col_list, val_list, pk, set_with_sync,
            )
        } else {
            format!(
                "INSERT INTO {} ({}) VALUES ({})
                 ON CONFLICT({}) DO UPDATE SET {}",
                table, col_list, val_list, pk, set_with_sync,
            )
        };

        sqlx::query(&sql).execute(&self.pool).await?;
        Ok(())
    }

    /// Apply a customer row with LWW but GREATEST for loyalty_points.
    async fn apply_customer(
        &self,
        obj: &serde_json::Map<String, Value>,
    ) -> AppResult<()> {
        let cols: Vec<&String> = obj
            .keys()
            .filter(|k| {
                *k != "sync_status" && *k != "sync_attempts"
                    && !matches!(obj.get(*k), Some(Value::Null))
            })
            .collect();

        if cols.is_empty() {
            return Ok(());
        }

        let col_list = cols.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(", ");
        let val_list = cols
            .iter()
            .map(|c| json_to_sql_literal(&obj[*c]))
            .collect::<Vec<_>>()
            .join(", ");

        // Build SET clause, with special handling for loyalty_points
        let set_parts: Vec<String> = cols
            .iter()
            .filter(|c| **c != "customer_id")
            .map(|c| {
                if c.as_str() == "loyalty_points" {
                    format!(
                        "{} = MAX(customers.loyalty_points, excluded.loyalty_points)",
                        c
                    )
                } else {
                    format!("{} = excluded.{}", c, c)
                }
            })
            .collect();

        let set_clause = format!("{}, sync_status = 'synced'", set_parts.join(", "));

        let has_updated_at = obj.contains_key("updated_at");

        let sql = if has_updated_at && !set_clause.is_empty() {
            format!(
                "INSERT INTO customers ({}) VALUES ({})
                 ON CONFLICT(customer_id) DO UPDATE SET {}
                 WHERE datetime(customers.updated_at) < datetime(excluded.updated_at)",
                col_list, val_list, set_clause,
            )
        } else {
            format!(
                "INSERT INTO customers ({}) VALUES ({})
                 ON CONFLICT(customer_id) DO UPDATE SET {}",
                col_list, val_list, set_clause,
            )
        };

        sqlx::query(&sql).execute(&self.pool).await?;
        Ok(())
    }

    /// Apply an append-only row: INSERT OR IGNORE.
    async fn apply_append_only(
        &self,
        table: &str,
        obj: &serde_json::Map<String, Value>,
    ) -> AppResult<()> {
        let cols: Vec<&String> = obj
            .keys()
            .filter(|k| {
                *k != "sync_status" && *k != "sync_attempts"
                    && !matches!(obj.get(*k), Some(Value::Null))
            })
            .collect();

        if cols.is_empty() {
            return Ok(());
        }

        let col_list = cols.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(", ");
        let val_list = cols
            .iter()
            .map(|c| json_to_sql_literal(&obj[*c]))
            .collect::<Vec<_>>()
            .join(", ");

        // Pulled rows are already on Supabase — mark them synced so they
        // aren't pushed back on the next cycle.
        let sql = format!(
            "INSERT OR IGNORE INTO {} ({}, sync_status) VALUES ({}, 'synced')",
            table, col_list, val_list,
        );

        sqlx::query(&sql).execute(&self.pool).await?;

        // For stock_movements: recompute stock_levels after applying
        if table == "stock_movements" {
            if let (Some(product_id), Some(branch_id), Some(created_at)) = (
                obj.get("product_id").and_then(|v| v.as_str()),
                obj.get("branch_id").and_then(|v| v.as_str()),
                obj.get("created_at").and_then(|v| v.as_str()),
            ) {
                let _ = recompute_stock_level(
                    &self.pool,
                    product_id,
                    branch_id,
                    created_at,
                )
                .await;
            }
        }

        Ok(())
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

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Whether a table has an `origin_device_id` column on Supabase.
/// Pull queries add a `neq` filter for tables that have this column.
fn has_origin_device_id(table: &str) -> bool {
    matches!(
        table,
        "shifts"
            | "sales"
            | "sale_items"
            | "payments"
            | "refunds"
            | "refund_items"
            | "stock_movements"
            | "audit_logs"
            | "delivery_orders"
    )
}

/// Return the primary key column name for a table.
fn pk_for_table(table: &str) -> &str {
    match table {
        "categories" => "category_id",
        "tax_rules" => "tax_rule_id",
        "products" => "product_id",
        "devices" => "device_id",
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
        "users" => "user_id",
        _ => "id",
    }
}

/// Returns true if a column is local-only and must not be included
/// in JSON payloads sent to the central Supabase schema.
fn should_skip_column(table: &str, col_name: &str) -> bool {
    // Global: never sync these to central
    if col_name == "sync_status" || col_name == "sync_attempts" || col_name == "pin_hash" || col_name == "deleted_at" {
        return true;
    }
    match (table, col_name) {
        ("users", "failed_pin_attempts" | "locked_until" | "last_login_at") => true,
        ("devices", "next_receipt_seq" | "last_seen_at" | "version") => true,
        ("customers", "origin_device_id" | "version") => true,
        ("shifts", "expected_cash_minor" | "cash_difference_minor" | "business_date" | "created_at" | "version") => true,
        ("audit_logs", "override_used") => true,
        _ => false,
    }
}

/// Extract a typed value from a sqlx Row column by name.
/// Returns Value::Null for missing or null columns.
fn value_from_row_column(row: &sqlx::sqlite::SqliteRow, col: &str) -> Value {
    // Try nullable integer first — empty/NULL → Value::Null
    if let Ok(v) = row.try_get::<Option<i64>, _>(col) {
        return match v {
            Some(n) => Value::Number(n.into()),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(col) {
        return match v {
            Some(n) => {
                if let Some(num) = serde_json::Number::from_f64(n) {
                    Value::Number(num)
                } else {
                    Value::Null
                }
            }
            None => Value::Null,
        };
    }
    // Try non-nullable int/float fallback
    if let Ok(v) = row.try_get::<i64, _>(col) {
        return Value::Number(v.into());
    }
    if let Ok(v) = row.try_get::<f64, _>(col) {
        if let Some(n) = serde_json::Number::from_f64(v) {
            return Value::Number(n);
        }
    }
    // Strings last — but convert empty to null (Supabase BIGINT rejects "")
    if let Ok(v) = row.try_get::<Option<String>, _>(col) {
        return match v {
            Some(s) if !s.is_empty() => Value::String(s),
            _ => Value::Null,
        };
    }
    if let Ok(v) = row.try_get::<String, _>(col) {
        if v.is_empty() {
            return Value::Null;
        }
        return Value::String(v);
    }
    Value::Null
}

/// Convert a serde_json Value to a SQLite-safe literal string for embedding in SQL.
fn json_to_sql_literal(v: &Value) -> String {
    match v {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => (if *b { "1" } else { "0" }).to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        Value::Array(_) | Value::Object(_) => {
            let s = serde_json::to_string(v).unwrap_or_default();
            format!("'{}'", s.replace('\'', "''"))
        }
    }
}

/// Recompute stock_levels quantity_on_hand from the stock_movements ledger
/// for a given (product, branch). Called after applying a remote movement.
async fn recompute_stock_level(
    pool: &SqlitePool,
    product_id: &str,
    branch_id: &str,
    applied_at: &str,
) -> AppResult<()> {
    let ledger_sum: Option<f64> = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(CAST(quantity_delta AS REAL)), 0) AS REAL)
         FROM stock_movements
         WHERE product_id = ? AND branch_id = ?",
    )
    .bind(product_id)
    .bind(branch_id)
    .fetch_one(pool)
    .await?;

    let ledger = ledger_sum.unwrap_or(0.0);

    let stock_level_id = format!("SL-{}-{}", product_id, branch_id);
    let qty_str = format!("{:.3}", ledger)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string();

    sqlx::query(
        "INSERT INTO stock_levels
            (stock_level_id, product_id, branch_id, quantity_on_hand, last_movement_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(product_id, branch_id) DO UPDATE SET
            quantity_on_hand = excluded.quantity_on_hand,
            last_movement_at = excluded.last_movement_at,
            updated_at       = excluded.updated_at",
    )
    .bind(&stock_level_id)
    .bind(product_id)
    .bind(branch_id)
    .bind(&qty_str)
    .bind(applied_at)
    .bind(applied_at)
    .bind(applied_at)
    .execute(pool)
    .await?;

    // Drift detection
    let cached: Option<f64> = sqlx::query_scalar(
        "SELECT CAST(quantity_on_hand AS REAL) FROM stock_levels
         WHERE product_id = ? AND branch_id = ?",
    )
    .bind(product_id)
    .bind(branch_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    if let Some(c) = cached {
        if (ledger - c).abs() > STOCK_DRIFT_TOLERANCE {
            tracing::warn!(
                "stock drift: product={product_id} branch={branch_id} ledger={ledger} cached={c}"
            );
        }
    }

    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_to_sql_literal() {
        assert_eq!(json_to_sql_literal(&Value::Null), "NULL");
        assert_eq!(json_to_sql_literal(&Value::Bool(true)), "1");
        assert_eq!(json_to_sql_literal(&Value::Bool(false)), "0");
        assert_eq!(json_to_sql_literal(&serde_json::json!(42)), "42");
        assert_eq!(
            json_to_sql_literal(&Value::String("hello".into())),
            "'hello'"
        );
        assert_eq!(
            json_to_sql_literal(&Value::String("it's".into())),
            "'it''s'"
        );
    }

    #[test]
    fn test_pk_for_table() {
        assert_eq!(pk_for_table("products"), "product_id");
        assert_eq!(pk_for_table("sales"), "sale_id");
        assert_eq!(pk_for_table("unknown"), "id");
    }
}
