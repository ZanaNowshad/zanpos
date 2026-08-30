pub mod apply;
#[cfg(test)]
mod barcode_tests;
pub mod client;
mod client_parity;
pub mod consistency;
pub mod dead_letter;
pub mod inbox;
pub mod parity;
pub mod reconcile;
pub mod registry;
pub mod repair;
#[cfg(test)]
mod tombstone_tests;
pub mod worker;

pub use worker::SyncWorker;

/// Parse a timestamp in either format this codebase writes.
///
/// Most code writes RFC3339 via chrono; the catalogue importer and several SQL
/// defaults write `datetime('now')`, which has a space instead of a `T` and no
/// zone. Text comparison ranks `T` (0x54) above a space (0x20), so the two
/// formats do not sort against each other correctly — both the watermark math
/// and the concurrent-edit detector need the instant, not the string.
pub(crate) fn parse_ts(raw: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|naive| naive.and_utc())
}
