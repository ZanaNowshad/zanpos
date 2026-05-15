use sqlx::{SqlitePool, Row};
use serde::Serialize;
use crate::errors::AppResult;

#[derive(Debug, Serialize)]
pub struct SyncStatus {
    pub online: bool,
    pub pending_events: i64,
    pub last_successful_sync_at: Option<String>,
    pub last_error: Option<String>,
    pub device_id: String,
}

pub async fn get_sync_status(pool: &SqlitePool, device_id: &str) -> AppResult<SyncStatus> {
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sync_queue WHERE device_id = ? AND status IN ('pending', 'sending', 'failed')"
    )
    .bind(device_id)
    .fetch_one(pool)
    .await?;

    let last_sync: Option<String> = sqlx::query_scalar(
        "SELECT last_successful_sync_at FROM sync_state WHERE device_id = ?"
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    let last_error_row = sqlx::query(
        "SELECT last_error FROM sync_queue WHERE device_id = ? AND status = 'failed' ORDER BY last_attempt_at DESC LIMIT 1"
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await?;

    let last_error: Option<String> = last_error_row.as_ref().map(|r| r.get("last_error"));

    Ok(SyncStatus {
        online: false,  // Phase 0: always offline
        pending_events: pending,
        last_successful_sync_at: last_sync,
        last_error,
        device_id: device_id.to_string(),
    })
}
