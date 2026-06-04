use crate::db::repositories::ai_admin_repo;
use crate::errors::{AppError, AppResult};
use crate::secure_store;
use crate::sync::inbox;
use crate::sync::supabase_client::{PushEvent, SupabaseClient};
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

const BATCH_SIZE: i64 = 50;
const INTERVAL_SECS: u64 = 30;
const MAX_ATTEMPTS: i64 = 10;

/// Resolve the active device_id from the DB at runtime.
async fn active_device_id(pool: &SqlitePool) -> AppResult<String> {
    let row = sqlx::query(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("No active device configured".into()))?;
    Ok(row.get("device_id"))
}

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
                // catch_unwind requires the future to be UnwindSafe; wrap in AssertUnwindSafe.
                let result = std::panic::AssertUnwindSafe(async move {
                    let mut ticker = interval(Duration::from_secs(INTERVAL_SECS));
                    loop {
                        ticker.tick().await;
                        worker_inner.run_once().await;
                    }
                });
                // Use tokio's spawn_blocking to run the catch; or just detect task completion.
                // Since async panics propagate as JoinError, spawn a child task and await it.
                let handle = tauri::async_runtime::spawn(result);
                match handle.await {
                    Ok(_) => {
                        // The loop exited normally (shouldn't happen, but handle gracefully)
                        tracing::warn!("Sync worker loop exited unexpectedly — restarting in 5 s");
                    }
                    Err(e) => {
                        tracing::error!("Sync worker panicked: {:?} — restarting in 5 s", e);
                    }
                }
                // Brief back-off before restarting to avoid tight panic loops
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
    }

    /// Run daily data pruning: remove confirmed-synced rows beyond retention window.
    /// Safe to call multiple times — checks last_prune_at before acting.
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
                    return; // Too soon
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

        // Pruning thresholds (ISO strings)
        let sales_cutoff = (chrono::Utc::now() - chrono::Duration::days(sales_days)).to_rfc3339();
        let log_cutoff = (chrono::Utc::now() - chrono::Duration::days(log_days)).to_rfc3339();
        let queue_cutoff = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();

        // 1. sync_queue: delete confirmed-synced rows older than 7 days
        let _ = sqlx::query("DELETE FROM sync_queue WHERE status = 'synced' AND created_at < ?")
            .bind(&queue_cutoff)
            .execute(&self.pool)
            .await;

        // 2. sale_items, payments linked to prunable sales (pruned in correct FK order)
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

        // 3. Sales themselves (sync_status = 'synced' only — never delete unsynced)
        let _ = sqlx::query("DELETE FROM sales WHERE sync_status = 'synced' AND sold_at < ?")
            .bind(&sales_cutoff)
            .execute(&self.pool)
            .await;

        // 4. Audit logs older than log_days (append-only; safe to prune from local cache)
        let _ = sqlx::query(
            "DELETE FROM audit_logs WHERE created_at < ? AND sync_status = 'synced'",
        )
        .bind(&log_cutoff)
        .execute(&self.pool)
        .await;

        // 5. Stock movements older than log_days (Supabase holds the full ledger)
        let _ = sqlx::query(
            "DELETE FROM stock_movements WHERE created_at < ? AND sync_status = 'synced'",
        )
        .bind(&log_cutoff)
        .execute(&self.pool)
        .await;

        // 6. WAL checkpoint + VACUUM to reclaim disk space.
        // R-06: log failures here — a failing checkpoint/VACUUM usually means the
        // disk is full or the DB is locked, which silently retains stale data.
        if let Err(e) = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.pool)
            .await
        {
            tracing::warn!("prune: WAL checkpoint failed (disk full or DB locked?): {e}");
        }
        if let Err(e) = sqlx::query("VACUUM").execute(&self.pool).await {
            tracing::warn!("prune: VACUUM failed (disk full or DB locked?): {e}");
        }

        // Record prune timestamp
        let now = chrono::Utc::now().to_rfc3339();
        let _ = sqlx::query(
            "INSERT INTO app_config(key, value, updated_at) VALUES ('last_prune_at',?,?)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
        )
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await;

        tracing::info!(
            "DB prune complete — sales cutoff: {sales_cutoff}, log cutoff: {log_cutoff}"
        );
    }

    /// Run one push + pull cycle. Called by background loop and by sync_trigger_now.
    pub async fn run_once(&self) {
        // Resolve device_id from DB each cycle (handles post-setup transitions)
        let device_id = match active_device_id(&self.pool).await {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!("Sync skipped — could not resolve device_id: {e}");
                return;
            }
        };

        // Load Supabase config fresh each cycle
        let client = match self.load_client().await {
            Some(c) => c,
            None => return, // Not configured yet — skip silently
        };

        let push_result = self.push_pending(&client, &device_id).await;
        let pull_result = self.pull_new(&client, &device_id).await;

        // Detect events that exhausted their per-event handling and are stuck in
        // 'failed' (e.g. the central RPC rejected the payload). push_pending returns
        // Ok even when individual non-transient events fail, so we surface the most
        // recent failure here — otherwise the worker would report "online" while
        // data silently never reaches the cloud. (Multi-terminal diagnostics.)
        let stuck_error: Option<String> = sqlx::query_scalar(
            "SELECT last_error FROM sync_queue
             WHERE device_id = ? AND status = 'failed'
             ORDER BY last_attempt_at DESC LIMIT 1",
        )
        .bind(&device_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten();

        let mut state = self.state.lock().await;
        match (push_result, pull_result) {
            (Ok(_), Ok(_)) => {
                // Connection works, but if events are stuck in 'failed' (server
                // rejected them), the operator must see a degraded/warning state,
                // not a false "all good" while data silently never lands.
                if stuck_error.is_some() {
                    state.online = false;
                    state.last_error = stuck_error;
                } else {
                    state.online = true;
                    state.last_error = None;
                }
                // UPSERT last_successful_sync_at so it works for new devices too
                let now = chrono::Utc::now().to_rfc3339();
                let ss_id = ulid::Ulid::new().to_string();
                // I-02: log if this write fails — silently dropping it would let the
                // "last successful sync" timestamp drift from reality.
                if let Err(e) = sqlx::query(
                    "INSERT INTO sync_state (sync_state_id, device_id, last_successful_sync_at)
                     VALUES (?, ?, ?)
                     ON CONFLICT(device_id) DO UPDATE SET
                       last_successful_sync_at = excluded.last_successful_sync_at",
                )
                .bind(&ss_id)
                .bind(&device_id)
                .bind(&now)
                .execute(&self.pool)
                .await
                {
                    tracing::warn!("Sync: failed to record last_successful_sync_at: {e}");
                }
            }
            (Err(e), _) | (_, Err(e)) => {
                state.online = false;
                state.last_error = Some(e.to_string());
                tracing::warn!("Sync cycle error: {e}");
            }
        }
        drop(state); // Release lock before pruning (which is slow)

        // Run daily pruning pass (no-ops if < 24 h since last run)
        self.prune_old_data().await;
    }

    // ── Load client from app_config ────────────────────────────────────────────

    async fn load_client(&self) -> Option<SupabaseClient> {
        // I-01: distinguish a genuine DB read error (log it) from "not configured"
        // (normal — sync simply skips). Previously both collapsed to a silent None.
        let url = match ai_admin_repo::get_config(&self.pool, "supabase_url").await {
            Ok(Some(u)) => u,
            Ok(None) => return None, // not configured yet — expected, skip quietly
            Err(e) => {
                tracing::error!("Sync: failed to read supabase_url from config: {e}");
                return None;
            }
        };
        // HIGH #3: OS credential store is the PRIMARY home for the service role key
        // (it bypasses RLS, so it must not sit in plaintext SQLite long-term). But a
        // keyring read can transiently fail on Windows, which previously made sync die
        // SILENTLY — the single worst failure mode (events stay "pending" forever with
        // no signal). We now: (a) try the keyring, (b) fall back to a DB-stored key if
        // present (legacy/older installs), and (c) if a URL is configured but NO key is
        // readable from either source, LOUDLY surface it so the honest sync status and
        // the log file show exactly why nothing is syncing.
        let key = {
            let from_os = secure_store::get_secret("supabase_service_key").unwrap_or_default();
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
                "Sync: Supabase URL is configured but the service key could not be read \
                 from the OS credential store OR the DB fallback. Sync is HALTED until \
                 the key is re-entered in Back Office → Sync. (This is why events stay pending.)"
            );
            // Surface to the worker state so the UI stops showing a misleading "Online".
            let mut st = self.state.lock().await;
            st.online = false;
            st.last_error = Some(
                "Supabase key unreadable — re-enter it in Back Office → Sync.".into(),
            );
            return None;
        }
        Some(SupabaseClient::new(url, key))
    }

    // ── Push pending outbox events ─────────────────────────────────────────────

    async fn push_pending(&self, client: &SupabaseClient, device_id: &str) -> AppResult<u32> {
        let rows = sqlx::query(
            "SELECT sync_event_id, entity_type, operation, payload_json,
                    payload_hash, local_sequence, idempotency_key, attempt_count
             FROM sync_queue
             WHERE device_id = ? AND status IN ('pending', 'failed') AND attempt_count < ?
             ORDER BY local_sequence ASC
             LIMIT ?"
        )
        .bind(device_id)
        .bind(MAX_ATTEMPTS)
        .bind(BATCH_SIZE)
        .fetch_all(&self.pool)
        .await?;

        let mut pushed = 0u32;

        for row in &rows {
            let sync_event_id: String = row.get("sync_event_id");
            let entity_type: String = row.get("entity_type");
            let operation: String = row.get("operation");
            let payload_str: String = row.get("payload_json");
            let payload_hash: String = row.get("payload_hash");
            let local_sequence: i64 = row.get("local_sequence");
            let idem_key: String = row.get("idempotency_key");
            let attempts: i64 = row.get("attempt_count");

            let payload_json: serde_json::Value = match serde_json::from_str(&payload_str) {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!("Bad payload JSON for {sync_event_id}: {e}");
                    self.mark_failed(&sync_event_id, &format!("bad payload: {e}"))
                        .await;
                    continue;
                }
            };

            let event = PushEvent {
                entity_type,
                operation,
                payload_json,
                idempotency_key: idem_key,
                payload_hash,
                local_sequence,
            };

            match client.push_event(&event).await {
                Ok(()) => {
                    let now = chrono::Utc::now().to_rfc3339();
                    sqlx::query(
                        "UPDATE sync_queue SET status = 'synced', last_attempt_at = ?
                         WHERE sync_event_id = ?",
                    )
                    .bind(&now)
                    .bind(&sync_event_id)
                    .execute(&self.pool)
                    .await?;

                    // Mark parent entity sync_status = 'synced' for sale/shift/refund rows
                    self.mark_entity_synced(&event.entity_type, &payload_str)
                        .await;

                    pushed += 1;
                }
                Err(e) => {
                    let now = chrono::Utc::now().to_rfc3339();
                    sqlx::query(
                        "UPDATE sync_queue
                         SET status = 'failed', attempt_count = ?, last_attempt_at = ?, last_error = ?
                         WHERE sync_event_id = ?"
                    )
                    .bind(attempts + 1)
                    .bind(&now)
                    .bind(e.to_string())
                    .bind(&sync_event_id)
                    .execute(&self.pool)
                    .await?;

                    // R-05: Stop on first TRANSIENT error (network/5xx/429) — the
                    // remaining queue retries next cycle. Permanent errors (4xx,
                    // bad payload) fall through so one poison row can't block the batch.
                    if e.to_string().contains(crate::sync::supabase_client::TRANSIENT_TAG.trim()) {
                        return Err(e);
                    }
                }
            }
        }

        Ok(pushed)
    }

    async fn mark_entity_synced(&self, entity_type: &str, payload_str: &str) {
        let payload: serde_json::Value = match serde_json::from_str(payload_str) {
            Ok(v) => v,
            Err(_) => return,
        };

        let (table, id_col) = match entity_type {
            "sale"           => ("sales",           "sale_id"),
            "shift"          => ("shifts",           "shift_id"),
            "refund"         => ("refunds",          "refund_id"),
            "payment"        => ("payments",         "payment_id"),
            "stock_level"    => ("stock_levels",     "product_id"),
            "stock_movement" => ("stock_movements",  "movement_id"),
            "audit_log"      => ("audit_logs",       "audit_log_id"),
            _ => return,
        };

        if let Some(id) = payload.get(id_col).and_then(|v| v.as_str()) {
            let sql = format!("UPDATE {table} SET sync_status = 'synced' WHERE {id_col} = ?");
            // I-02: log failure — if this UPDATE fails the row stays 'pending' and
            // will be re-pushed next cycle (server dedups via idempotency key), but
            // a persistent failure here needs to be visible, not silent.
            if let Err(e) = sqlx::query(&sql).bind(id).execute(&self.pool).await {
                tracing::warn!("Sync: failed to mark {table} {id} as synced: {e}");
            }
        }
    }

    async fn mark_failed(&self, sync_event_id: &str, error: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        let _ = sqlx::query(
            "UPDATE sync_queue SET status = 'failed', last_attempt_at = ?, last_error = ?
             WHERE sync_event_id = ?",
        )
        .bind(&now)
        .bind(error)
        .bind(sync_event_id)
        .execute(&self.pool)
        .await;
    }

    // ── Pull new events from central ───────────────────────────────────────────

    async fn pull_new(&self, client: &SupabaseClient, device_id: &str) -> AppResult<u32> {
        let mut watermark: i64 = sqlx::query_scalar(
            "SELECT last_pulled_central_sequence FROM sync_state WHERE device_id = ?",
        )
        .bind(device_id)
        .fetch_optional(&self.pool)
        .await
        .unwrap_or(None)
        .flatten()
        .unwrap_or(0);

        let mut total_count = 0u32;

        loop {
            let events = client.pull_events(watermark, device_id).await?;
            if events.is_empty() {
                break;
            }

            // CRITICAL (multi-terminal correctness): the watermark may ONLY advance
            // past events that applied successfully. Previously it advanced to the
            // last sequence regardless of apply errors, so any row that failed to
            // apply locally (e.g. a transient FK/constraint issue) was permanently
            // skipped and never retried — leaving a joining terminal missing users,
            // products, etc. forever. We now stop advancing at the first failure;
            // the same range is re-pulled next cycle until it applies.
            let mut applied_through = watermark;
            let mut hit_failure = false;
            let mut applied_this_batch = 0u32;
            for event in &events {
                match inbox::apply_event(&self.pool, event).await {
                    Ok(()) => {
                        if event.global_sequence > applied_through {
                            applied_through = event.global_sequence;
                        }
                        applied_this_batch += 1;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "inbox apply_event error ({}:{} seq {}): {e} — halting watermark here; will retry next cycle",
                            event.entity_type,
                            event.entity_id,
                            event.global_sequence
                        );
                        hit_failure = true;
                        break; // do NOT advance past the failed event
                    }
                }
            }

            total_count += applied_this_batch;
            watermark = applied_through;

            // Stop this cycle if we hit a failure (retry the failed event next cycle)
            // or if the server returned a partial page (exhausted).
            if hit_failure || events.len() < 100 {
                break;
            }
        }

        // Bug B fix: UPSERT instead of UPDATE so the watermark is always persisted,
        // even for a brand-new device (POS 2) that has no sync_state row yet.
        // Without this, pull_new starts from sequence 0 on every sync cycle.
        let new_sync_state_id = ulid::Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO sync_state (sync_state_id, device_id, last_pulled_central_sequence)
             VALUES (?, ?, ?)
             ON CONFLICT(device_id) DO UPDATE SET
               last_pulled_central_sequence = excluded.last_pulled_central_sequence",
        )
        .bind(&new_sync_state_id)
        .bind(device_id)
        .bind(watermark)
        .execute(&self.pool)
        .await?;

        Ok(total_count)
    }
}
