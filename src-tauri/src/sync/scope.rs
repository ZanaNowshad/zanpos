//! Report device-scope filtering.
//!
//! Cross-device sales, refunds, and shifts sync between POS devices (see
//! migration 0032). The reports have always shown ALL data for the branch,
//! which now means all devices. This module provides a scope filter so the
//! manager dashboard can switch between:
//!
//! - `'origin'` (default) — show only this device's data. Preserves the
//!   cashier-facing single-device expectation; no surprises.
//! - `'all'` — show every device's data. The manager can see the whole
//!   store in one report.
//!
//! The scope is stored in `app_config.reports_device_scope` and synced to
//! other devices via the existing app_config sync whitelist (see
//! `inbox.rs` `SYNCABLE`).
//!
//! SQL pattern used by every report query:
//! ```sql
//! AND (? = 'all' OR s.origin_device_id = ?)
//! ```
//! The first bind is the scope string; when it's 'all' the OR short-circuits
//! to true. When it's 'origin', only rows whose origin_device_id matches
//! the second bind are included.

use sqlx::SqlitePool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceScope {
    /// Show only this device's events.
    Origin,
    /// Show all devices' events for this branch.
    All,
}

impl DeviceScope {
    pub fn from_str(s: &str) -> Self {
        if s.eq_ignore_ascii_case("all") {
            Self::All
        } else {
            Self::Origin
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Origin => "origin",
            Self::All => "all",
        }
    }
}

/// Read the configured scope and the local device id. Always returns a
/// device id (falls back to empty string if the local device is not yet
/// registered, which makes the `? = 'all'` predicate the only true path).
pub async fn report_scope(pool: &SqlitePool) -> (DeviceScope, String) {
    let scope_str: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'reports_device_scope'")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .flatten();
    let scope = scope_str
        .as_deref()
        .map(DeviceScope::from_str)
        .unwrap_or(DeviceScope::Origin);

    let device_id: String = sqlx::query_scalar(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_default();

    (scope, device_id)
}
