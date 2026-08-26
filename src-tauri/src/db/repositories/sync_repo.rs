use crate::errors::AppResult;
use serde::Serialize;
use sqlx::SqlitePool;

#[derive(Debug, Serialize)]
pub struct SyncStatus {
    pub online: bool,
    pub hub_configured: bool,
    pub pending_events: i64,
    pub last_successful_sync_at: Option<String>,
    pub days_since_last_sync: Option<i64>,
    pub last_error: Option<String>,
    pub device_id: String,
}

/// Tables that record outbound work per row.
///
/// Derived from the sync registry. This copy was missing `product_barcodes`,
/// so unsent barcode changes were not counted in the sync status an operator
/// reads — on the screen they consult precisely when they suspect the terminal
/// is holding something.
static SYNC_TABLES: std::sync::LazyLock<Vec<&'static str>> =
    std::sync::LazyLock::new(crate::sync_v2::registry::row_queued);

pub async fn get_sync_status(pool: &SqlitePool, device_id: &str) -> AppResult<SyncStatus> {
    // Count pending rows across all syncable tables
    let mut pending: i64 = 0;
    for table in SYNC_TABLES.iter() {
        let sql = format!(
            "SELECT COUNT(*) FROM {} WHERE sync_status = 'pending'",
            table
        );
        let n: i64 = sqlx::query_scalar(&sql).fetch_one(pool).await.unwrap_or(0);
        pending += n;
    }

    // Read last successful sync from watermark
    let last_sync: Option<String> =
        sqlx::query_scalar("SELECT last_pushed_at FROM sync_watermark WHERE table_name = 'sales'")
            .fetch_optional(pool)
            .await?
            .flatten();

    // Hub configuration check: this device IS the hub, or it has a hub_url
    // plus the store token in the OS credential store.
    let hub_mode: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'hub_mode'")
            .fetch_optional(pool)
            .await?
            .flatten();
    let hub_url: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'hub_url'")
            .fetch_optional(pool)
            .await?
            .flatten();
    let token = crate::secure_store::get_secret("hub_store_token").unwrap_or_default();
    let hub_configured = hub_mode.as_deref() == Some("1")
        || (hub_url.as_deref().is_some_and(|u| !u.is_empty()) && !token.is_empty());

    let last_sync = watermark_or_never(last_sync);
    let days_since_last_sync = last_sync.as_deref().and_then(|ts| {
        chrono::DateTime::parse_from_rfc3339(ts)
            .ok()
            .map(|t| chrono::Utc::now().signed_duration_since(t).num_days())
    });

    Ok(SyncStatus {
        online: false,
        hub_configured,
        pending_events: pending,
        last_successful_sync_at: last_sync,
        days_since_last_sync,
        last_error: None,
        device_id: device_id.to_string(),
    })
}

/// The sync watermark is seeded `NOT NULL DEFAULT '1970-01-01T00:00:00Z'`, so a
/// store that has never synced still returns a parseable timestamp rather than
/// NULL. Every consumer then rendered "1 Jan 1970, 03:00" — an epoch sentinel
/// displayed as though it were a real sync — and `days_since_last_sync` came
/// out around 20,000.
///
/// Normalising here means the "never synced" case is expressed as absence, which
/// is what the UI's existing null-guards already handle correctly.
pub(crate) fn watermark_or_never(ts: Option<String>) -> Option<String> {
    ts.filter(|t| {
        chrono::DateTime::parse_from_rfc3339(t)
            .map(|d| d.timestamp() > 0)
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod watermark_tests {
    use super::watermark_or_never;

    #[test]
    fn the_seeded_epoch_sentinel_means_never_synced() {
        // migrations/0001_initial.sql seeds this exact value for every table.
        assert_eq!(
            watermark_or_never(Some("1970-01-01T00:00:00Z".into())),
            None
        );
        assert_eq!(
            watermark_or_never(Some("1970-01-01T00:00:00+00:00".into())),
            None
        );
        // Same instant expressed in Bahrain time, which is how it reached the UI.
        assert_eq!(
            watermark_or_never(Some("1970-01-01T03:00:00+03:00".into())),
            None
        );
    }

    #[test]
    fn a_real_sync_time_is_preserved() {
        let real = "2026-08-12T09:15:00Z".to_string();
        assert_eq!(watermark_or_never(Some(real.clone())), Some(real));
    }

    #[test]
    fn absent_or_unparseable_stays_absent() {
        assert_eq!(watermark_or_never(None), None);
        // Garbage must not be presented as a sync time either.
        assert_eq!(watermark_or_never(Some("not a date".into())), None);
        assert_eq!(watermark_or_never(Some(String::new())), None);
    }
}
