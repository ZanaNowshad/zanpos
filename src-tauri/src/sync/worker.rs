use sqlx::{SqlitePool, Row};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use crate::db::repositories::ai_admin_repo;
use crate::errors::AppResult;
use crate::sync::inbox;
use crate::sync::supabase_client::{PushEvent, SupabaseClient};

const DEVICE_ID: &str = "01JDEVICE0000000000000001";
const BATCH_SIZE: i64 = 50;
const INTERVAL_SECS: u64 = 30;
const MAX_ATTEMPTS: i64 = 10;

// ── Shared online state ────────────────────────────────────────────────────────

#[derive(Default)]
pub struct SyncState {
    pub online: bool,
    pub last_error: Option<String>,
}

pub struct SyncWorker {
    pool:  SqlitePool,
    pub state: Arc<Mutex<SyncState>>,
}

impl SyncWorker {
    pub fn new(pool: SqlitePool) -> Arc<Self> {
        Arc::new(Self {
            pool,
            state: Arc::new(Mutex::new(SyncState::default())),
        })
    }

    /// Spawn background loop. Takes a clone of the Arc so it runs independently.
    pub fn spawn(worker: Arc<Self>) {
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(INTERVAL_SECS));
            loop {
                ticker.tick().await;
                worker.run_once().await;
            }
        });
    }

    /// Run one push + pull cycle. Called by background loop and by sync_trigger_now.
    pub async fn run_once(&self) {
        // Load Supabase config fresh each cycle
        let client = match self.load_client().await {
            Some(c) => c,
            None => return,  // Not configured yet — skip silently
        };

        let push_result = self.push_pending(&client).await;
        let pull_result = self.pull_new(&client).await;

        let mut state = self.state.lock().await;
        match (push_result, pull_result) {
            (Ok(_), Ok(_)) => {
                state.online = true;
                state.last_error = None;
                // Update last_successful_sync_at
                let now = chrono::Utc::now().to_rfc3339();
                let _ = sqlx::query(
                    "UPDATE sync_state SET last_successful_sync_at = ? WHERE device_id = ?"
                )
                .bind(&now)
                .bind(DEVICE_ID)
                .execute(&self.pool)
                .await;
            }
            (Err(e), _) | (_, Err(e)) => {
                state.online = false;
                state.last_error = Some(e.to_string());
                tracing::warn!("Sync cycle error: {e}");
            }
        }
    }

    // ── Load client from app_config ────────────────────────────────────────────

    async fn load_client(&self) -> Option<SupabaseClient> {
        let url = ai_admin_repo::get_config(&self.pool, "supabase_url").await.ok()??;
        let key = ai_admin_repo::get_config(&self.pool, "supabase_service_key").await.ok()??;
        if url.is_empty() || key.is_empty() {
            return None;
        }
        Some(SupabaseClient::new(url, key))
    }

    // ── Push pending outbox events ─────────────────────────────────────────────

    async fn push_pending(&self, client: &SupabaseClient) -> AppResult<u32> {
        let rows = sqlx::query(
            "SELECT sync_event_id, entity_type, operation, payload_json, idempotency_key, attempt_count
             FROM sync_queue
             WHERE device_id = ? AND status IN ('pending', 'failed') AND attempt_count < ?
             ORDER BY local_sequence ASC
             LIMIT ?"
        )
        .bind(DEVICE_ID)
        .bind(MAX_ATTEMPTS)
        .bind(BATCH_SIZE)
        .fetch_all(&self.pool)
        .await?;

        let mut pushed = 0u32;

        for row in &rows {
            let sync_event_id: String = row.get("sync_event_id");
            let entity_type:   String = row.get("entity_type");
            let operation:     String = row.get("operation");
            let payload_str:   String = row.get("payload_json");
            let idem_key:      String = row.get("idempotency_key");
            let attempts:      i64    = row.get("attempt_count");

            let payload_json: serde_json::Value = match serde_json::from_str(&payload_str) {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!("Bad payload JSON for {sync_event_id}: {e}");
                    self.mark_failed(&sync_event_id, &format!("bad payload: {e}")).await;
                    continue;
                }
            };

            let event = PushEvent { entity_type, operation, payload_json, idempotency_key: idem_key };

            match client.push_event(&event).await {
                Ok(()) => {
                    let now = chrono::Utc::now().to_rfc3339();
                    sqlx::query(
                        "UPDATE sync_queue SET status = 'synced', last_attempt_at = ?
                         WHERE sync_event_id = ?"
                    )
                    .bind(&now)
                    .bind(&sync_event_id)
                    .execute(&self.pool)
                    .await?;

                    // Mark parent entity sync_status = 'synced' for sale/shift/refund rows
                    self.mark_entity_synced(&event.entity_type, &payload_str).await;

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
                    .bind(&e.to_string())
                    .bind(&sync_event_id)
                    .execute(&self.pool)
                    .await?;

                    // Stop on first network error — remaining will retry next cycle
                    if e.to_string().contains("connect") || e.to_string().contains("timeout") {
                        return Err(e);
                    }
                }
            }
        }

        Ok(pushed)
    }

    async fn mark_entity_synced(&self, entity_type: &str, payload_str: &str) {
        let payload: serde_json::Value = match serde_json::from_str(payload_str) {
            Ok(v) => v, Err(_) => return,
        };

        let (table, id_col) = match entity_type {
            "sale"    => ("sales",    "sale_id"),
            "shift"   => ("shifts",   "shift_id"),
            "refund"  => ("refunds",  "refund_id"),
            "payment" => ("payments", "payment_id"),
            _         => return,
        };

        if let Some(id) = payload.get(id_col).and_then(|v| v.as_str()) {
            let sql = format!("UPDATE {table} SET sync_status = 'synced' WHERE {id_col} = ?");
            let _ = sqlx::query(&sql).bind(id).execute(&self.pool).await;
        }
    }

    async fn mark_failed(&self, sync_event_id: &str, error: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        let _ = sqlx::query(
            "UPDATE sync_queue SET status = 'failed', last_attempt_at = ?, last_error = ?
             WHERE sync_event_id = ?"
        )
        .bind(&now)
        .bind(error)
        .bind(sync_event_id)
        .execute(&self.pool)
        .await;
    }

    // ── Pull new events from central ───────────────────────────────────────────

    async fn pull_new(&self, client: &SupabaseClient) -> AppResult<u32> {
        let watermark: i64 = sqlx::query_scalar(
            "SELECT last_pulled_central_sequence FROM sync_state WHERE device_id = ?"
        )
        .bind(DEVICE_ID)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        let events = client.pull_events(watermark, DEVICE_ID).await?;
        let count = events.len() as u32;

        if events.is_empty() {
            return Ok(0);
        }

        let mut last_seq = watermark;
        for event in &events {
            if let Err(e) = inbox::apply_event(&self.pool, event).await {
                tracing::warn!("inbox apply_event error ({}:{}): {e}",
                    event.entity_type, event.entity_id);
                // Continue — a single bad event should not block the rest
            }
            if event.global_sequence > last_seq {
                last_seq = event.global_sequence;
            }
        }

        // Update watermark
        sqlx::query(
            "UPDATE sync_state SET last_pulled_central_sequence = ? WHERE device_id = ?"
        )
        .bind(last_seq)
        .bind(DEVICE_ID)
        .execute(&self.pool)
        .await?;

        Ok(count)
    }
}
