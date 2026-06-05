use crate::db::repositories::ai_admin_repo;
use crate::errors::{AppError, AppResult};
use crate::secure_store;
use crate::sync_v2::client::SupabaseClient;
use serde_json::Value;
use sqlx::Column;
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

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
    /// If the inner async task panics, the supervisor waits 5 s and re-spawns it,
    /// preventing silent sync death from unexpected runtime errors.
    pub fn spawn(worker: Arc<Self>) {
        let worker_outer = worker.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let worker_inner = worker_outer.clone();
                let result = std::panic::AssertUnwindSafe(async move {
                    let mut ticker = interval(Duration::from_secs(INTERVAL_SECS));
                    loop {
                        ticker.tick().await;
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

        let mut state = self.state.lock().await;
        match (&push_result, &pull_result) {
            (Ok(_), Ok(_)) => {
                state.online = true;
                state.last_error = None;
                // Record last successful sync timestamp in sync_watermark
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
            (Err(e), _) | (_, Err(e)) => {
                state.online = false;
                state.last_error = Some(e.to_string());
                tracing::warn!("Sync v2 cycle error: {e}");
            }
        }
        drop(state);

        // Run daily pruning pass
        self.prune_old_data().await;
    }

    // ── Load client from app_config + keyring ──────────────────────────────────

    async fn load_client(&self) -> Option<SupabaseClient> {
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
                            // Skip local-only tracking columns
                            if col_name == "sync_status" || col_name == "sync_attempts" {
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
                        // Increment attempt count
                        for id in &row_ids {
                            let sql = format!(
                                "UPDATE {} SET sync_attempts = sync_attempts + 1 WHERE {} = ?",
                                table, id_col
                            );
                            let _ = sqlx::query(&sql).bind(id).execute(&self.pool).await;
                        }

                        // Stop batch on transient error, continue on permanent
                        if e.to_string().contains(TRANSIENT_TAG) {
                            return Err(e);
                        }
                        // Permanent error: break inner loop, continue to next table
                        break;
                    }
                }
            }
        }

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
                // Handle device_code collision (like old inbox)
                if let (Some(device_id), Some(branch_id), Some(device_code)) = (
                    obj.get("device_id").and_then(|v| v.as_str()),
                    obj.get("branch_id").and_then(|v| v.as_str()),
                    obj.get("device_code").and_then(|v| v.as_str()),
                ) {
                    let _ = sqlx::query(
                        "DELETE FROM devices WHERE branch_id = ? AND device_code = ? AND device_id <> ?",
                    )
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
                // Handle username collision: DELETE local stub if different user_id
                if let (Some(user_id), Some(username)) = (
                    obj.get("user_id").and_then(|v| v.as_str()),
                    obj.get("username").and_then(|v| v.as_str()),
                ) {
                    let _ = sqlx::query(
                        "DELETE FROM users WHERE username = ? AND user_id <> ?",
                    )
                    .bind(username)
                    .bind(user_id)
                    .execute(&self.pool)
                    .await;
                }
                self.apply_lww("users", "user_id", obj, &["pin_hash"]).await
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
            })
            .collect();

        if cols.is_empty() {
            return Ok(());
        }

        // Build INSERT (col1, col2, ...) VALUES (v1, v2, ...)
        let col_list = cols.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(", ");
        let val_list = cols
            .iter()
            .map(|c| json_to_sql_literal(&obj[*c]))
            .collect::<Vec<_>>()
            .join(", ");

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

/// Extract a typed value from a sqlx Row column by name.
/// Returns Value::Null for missing or null columns.
fn value_from_row_column(row: &sqlx::sqlite::SqliteRow, col: &str) -> Value {
    // Try common types in order of likelihood
    if let Ok(v) = row.try_get::<String, _>(col) {
        return Value::String(v);
    }
    if let Ok(v) = row.try_get::<i64, _>(col) {
        return Value::Number(v.into());
    }
    if let Ok(v) = row.try_get::<f64, _>(col) {
        if let Some(n) = serde_json::Number::from_f64(v) {
            return Value::Number(n);
        }
    }
    // Fallback: try as String again (sqlx might coerce ints)
    if let Ok(v) = row.try_get::<Option<String>, _>(col) {
        if let Some(s) = v {
            return Value::String(s);
        }
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
