//! Quarantine for rows that will never apply, so one of them cannot stop a table forever.
//!
//! The pull loop advances a table's watermark only past rows it actually
//! applied — deliberately, so a failure cannot skip a sale. But a row that can
//! *never* apply turns that safety into a trap: an order line whose product was
//! hard-deleted upstream, a foreign key that will not resolve. The watermark
//! stops just below that row's timestamp, the next cycle re-fetches from the
//! same point, and it fails again. That table never syncs again, and no amount
//! of waiting fixes it.
//!
//! So after a row has failed repeatedly — not once — it is set aside verbatim
//! and the table is allowed to move on.
//!
//! This weakens a guarantee that was previously absolute, so it is deliberately
//! loud:
//!
//! * the payload is kept in full, so quarantine is a pause and not a deletion
//! * a `sync_conflicts` row is written at `error`, into the table the existing
//!   conflict UI already reads
//! * [`pending_count`] is non-zero while anything is quarantined, so a terminal
//!   holding a set-aside sale cannot report itself healthy

use crate::errors::AppResult;
use serde_json::Value;
use sqlx::SqlitePool;

/// How many times a row must fail before it is set aside.
///
/// Not one: transient causes look identical to permanent ones at the moment
/// they happen, and a dependency that has not arrived yet usually arrives.
/// Failing five cycles is the difference between "out of order" and "never".
pub const QUARANTINE_AFTER_ATTEMPTS: i64 = 5;

/// Set a row aside and let the table continue.
pub async fn quarantine(
    pool: &SqlitePool,
    table: &str,
    entity_id: &str,
    payload: &Value,
    reason: &str,
    attempts: i64,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    tracing::error!(
        table,
        entity_id,
        attempts,
        reason,
        "Sync: row quarantined after repeated failures — the table can advance, \
         but this record is NOT applied"
    );

    sqlx::query(
        "INSERT INTO sync_dead_letter
           (dead_letter_id, table_name, entity_id, payload_json, reason, attempts,
            first_failed_at, last_failed_at, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'quarantined')
         ON CONFLICT(table_name, entity_id, status) DO UPDATE SET
            attempts       = excluded.attempts,
            reason         = excluded.reason,
            payload_json   = excluded.payload_json,
            last_failed_at = excluded.last_failed_at",
    )
    .bind(ulid::Ulid::new().to_string())
    .bind(table)
    .bind(entity_id)
    .bind(payload.to_string())
    .bind(reason)
    .bind(attempts)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    // Into the same table the conflict UI already reads. A quarantine that only
    // appeared in the log would be invisible to the person who needs it.
    sqlx::query(
        "INSERT INTO sync_conflicts
           (conflict_id, conflict_type, table_name, entity_id, severity, title, detail,
            status, created_at)
         VALUES (?, 'quarantined', ?, ?, 'error', ?, ?, 'open', ?)",
    )
    .bind(ulid::Ulid::new().to_string())
    .bind(table)
    .bind(entity_id)
    .bind(format!(
        "{table} row set aside after {attempts} failed attempts"
    ))
    .bind(format!(
        "This record could not be applied and has been set aside so the rest of \
         {table} can keep syncing. It is stored in full and can be replayed once \
         the cause is fixed. Last error: {reason}"
    ))
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(())
}

/// How many rows are currently set aside.
///
/// Anything above zero means this terminal is knowingly missing data, which no
/// health or parity report may describe as healthy.
pub async fn pending_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM sync_dead_letter WHERE status = 'quarantined'")
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}

/// Quarantined rows, newest failure first, for inspection and replay.
pub async fn pending(pool: &SqlitePool, limit: i64) -> AppResult<Vec<(String, String, String)>> {
    Ok(sqlx::query_as(
        "SELECT table_name, entity_id, reason FROM sync_dead_letter
          WHERE status = 'quarantined' ORDER BY last_failed_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

#[cfg(test)]
mod tests;
