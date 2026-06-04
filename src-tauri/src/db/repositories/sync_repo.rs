use crate::errors::AppResult;
use serde::Serialize;
use sqlx::{Row, SqlitePool};

#[derive(Debug, Serialize)]
pub struct SyncStatus {
    pub online: bool,
    /// True when Supabase URL + service key are both non-empty in app_config.
    pub supabase_configured: bool,
    pub pending_events: i64,
    pub last_successful_sync_at: Option<String>,
    /// Days elapsed since last successful sync. None if never synced or not configured.
    pub days_since_last_sync: Option<i64>,
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

    let last_sync: Option<String> =
        sqlx::query_scalar("SELECT last_successful_sync_at FROM sync_state WHERE device_id = ?")
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

    // Determine Supabase configuration state.
    // The service key lives in the OS credential store (HIGH #3 security fix wrote
    // an empty string to app_config). So we must check the keyring FIRST, falling
    // back to the legacy plaintext DB value only for older installs. Checking only
    // app_config here was the bug that made the AI page show "Not synced" while the
    // worker (which reads the keyring) was actually syncing fine.
    let supabase_url: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'supabase_url'")
            .fetch_optional(pool)
            .await?
            .flatten();
    let key_from_os = crate::secure_store::get_secret("supabase_service_key").unwrap_or_default();
    let supabase_key = if !key_from_os.is_empty() {
        key_from_os
    } else {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT value FROM app_config WHERE key = 'supabase_service_key'",
        )
        .fetch_optional(pool)
        .await?
        .flatten()
        .unwrap_or_default()
    };
    let supabase_configured = supabase_url.as_deref().is_some_and(|u| !u.is_empty())
        && !supabase_key.is_empty();

    // Days since last successful sync
    let days_since_last_sync = last_sync.as_deref().and_then(|ts| {
        chrono::DateTime::parse_from_rfc3339(ts)
            .ok()
            .map(|t| chrono::Utc::now().signed_duration_since(t).num_days())
    });

    Ok(SyncStatus {
        online: false, // Set to true by SyncWorker on successful push+pull
        supabase_configured,
        pending_events: pending,
        last_successful_sync_at: last_sync,
        days_since_last_sync,
        last_error,
        device_id: device_id.to_string(),
    })
}
