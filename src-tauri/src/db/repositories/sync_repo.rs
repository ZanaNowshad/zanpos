use crate::errors::AppResult;
use serde::Serialize;
use sqlx::{Row, SqlitePool};

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

/// Tables that participate in sync.
const SYNC_TABLES: &[&str] = &[
    "branches",          // was missing — branch edits not counted in sync status
    "categories", "tax_rules", "products", "devices", "users", "customers",
    "shifts", "sales", "sale_items", "payments", "refunds", "refund_items",
    "stock_movements", "stock_levels", "audit_logs", "delivery_orders", "product_prices",
    "cash_events",       // was missing — cash events never counted in sync status
];

pub async fn get_sync_status(pool: &SqlitePool, device_id: &str) -> AppResult<SyncStatus> {
    // Count pending rows across all syncable tables
    let mut pending: i64 = 0;
    for table in SYNC_TABLES {
        let sql = format!(
            "SELECT COUNT(*) FROM {} WHERE sync_status = 'pending'",
            table
        );
        let n: i64 = sqlx::query_scalar(&sql).fetch_one(pool).await.unwrap_or(0);
        pending += n;
    }

    // Read last successful sync from watermark
    let last_sync: Option<String> = sqlx::query_scalar(
        "SELECT last_pushed_at FROM sync_watermark WHERE table_name = 'sales'",
    )
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
