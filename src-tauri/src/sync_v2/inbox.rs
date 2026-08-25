//! A record of what arrived, so idempotency is something you can look at.
//!
//! Replay protection already worked: append-only inserts collided with a unique
//! index and the collision was interpreted. But it left no trace. "Did this sale
//! ever reach this terminal" had no answer, a deliberate replay was impossible,
//! and a row that quietly went missing offered nothing to investigate.
//!
//! The event id is derived from the payload rather than issued with it, because
//! rows cross the wire without one — ZANPOS syncs row state, not an event
//! stream. Hashing the canonical payload means an identical redelivery produces
//! an identical id and is recognised as the duplicate it is, while a genuine
//! edit to the same row produces a different id and is processed. That is the
//! property an event id would have given us, obtained from the row itself.
//!
//! Two statements per applied row: one to claim, one to confirm. On a steady
//! -state pull that is nothing; on a first sync of a large catalogue it is a
//! measurable but small cost, and it buys the ability to answer where a row went.

use crate::errors::AppResult;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

/// Whether this payload has been seen before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Not seen. The caller should apply it and then call [`confirm`].
    Apply,
    /// Byte-identical to something already processed. Applying again would be
    /// harmless for LWW and caught by a constraint for append-only, but doing
    /// nothing is cheaper and leaves a clearer record.
    AlreadySeen,
}

/// Deterministic id for a row payload.
///
/// `serde_json::Map` orders its keys, so the serialisation is canonical and two
/// terminals hashing the same row agree — the same property the parity
/// fingerprint relies on.
pub fn event_id(table: &str, entity_id: &str, payload: &Map<String, Value>) -> (String, String) {
    let canonical = serde_json::to_string(payload).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    (format!("{table}:{entity_id}:{}", &hash[..16]), hash)
}

/// Claim an incoming row, or report that it has already been handled.
///
/// The insert *is* the check: `ON CONFLICT DO NOTHING` affects no rows when the
/// id is already present, so one statement both records the arrival and answers
/// whether it is new. Doing it as a separate `SELECT` then `INSERT` would leave
/// a window where two pulls could claim the same event.
pub async fn claim(
    pool: &SqlitePool,
    table: &str,
    entity_id: &str,
    payload: &Map<String, Value>,
) -> AppResult<Decision> {
    let (event_id, payload_hash) = event_id(table, entity_id, payload);
    let source_device = payload
        .get("origin_device_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());

    let claimed = sqlx::query(
        "INSERT INTO sync_inbox
           (event_id, table_name, entity_id, payload_hash, source_device, received_at, status)
         VALUES (?, ?, ?, ?, ?, ?, 'received')
         ON CONFLICT(event_id) DO NOTHING",
    )
    .bind(&event_id)
    .bind(table)
    .bind(entity_id)
    .bind(&payload_hash)
    .bind(source_device)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;

    if claimed.rows_affected() == 1 {
        return Ok(Decision::Apply);
    }

    // The id was already present, which is *not* the same as already done.
    //
    // Only `applied` means the row landed. A `received` row was claimed by a
    // run that died before confirming, and a `failed` row is one the caller is
    // still retrying — reporting either as handled would make a row that never
    // applied look successful, and the retry that would eventually quarantine
    // it would never happen.
    let status: String = sqlx::query_scalar("SELECT status FROM sync_inbox WHERE event_id = ?")
        .bind(&event_id)
        .fetch_optional(pool)
        .await?
        .unwrap_or_else(|| "received".to_string());

    if status == "applied" {
        let _ = sqlx::query("UPDATE sync_inbox SET status = 'duplicate' WHERE event_id = ?")
            .bind(&event_id)
            .execute(pool)
            .await;
        return Ok(Decision::AlreadySeen);
    }
    if status == "duplicate" {
        return Ok(Decision::AlreadySeen);
    }
    Ok(Decision::Apply)
}

/// Mark a claimed event as applied.
pub async fn confirm(pool: &SqlitePool, table: &str, entity_id: &str, payload: &Map<String, Value>) {
    let (event_id, _) = event_id(table, entity_id, payload);
    let _ = sqlx::query(
        "UPDATE sync_inbox SET status = 'applied', processed_at = ? WHERE event_id = ?",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(&event_id)
    .execute(pool)
    .await;
}

/// Mark a claimed event as failed, and return how many times it has now failed.
///
/// The count is what the dead-letter threshold reads, so it lives with the
/// arrival record rather than in the worker's memory — a terminal that restarts
/// mid-retry must not forget that a row has already failed four times.
pub async fn record_failure(
    pool: &SqlitePool,
    table: &str,
    entity_id: &str,
    payload: &Map<String, Value>,
    error: &str,
) -> i64 {
    let (event_id, _) = event_id(table, entity_id, payload);
    let _ = sqlx::query(
        "UPDATE sync_inbox
            SET status = 'failed', attempts = attempts + 1, last_error = ?
          WHERE event_id = ?",
    )
    .bind(error)
    .bind(&event_id)
    .execute(pool)
    .await;

    sqlx::query_scalar("SELECT attempts FROM sync_inbox WHERE event_id = ?")
        .bind(&event_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(0)
}

/// Forget events that have been dealt with.
///
/// `failed` rows are never pruned: they are the ones somebody still has to look
/// at, and an inbox that quietly discards its own evidence is worse than none.
pub async fn prune(pool: &SqlitePool, older_than_days: i64) -> AppResult<u64> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(older_than_days)).to_rfc3339();
    let result = sqlx::query(
        "DELETE FROM sync_inbox
          WHERE status IN ('applied', 'duplicate') AND received_at < ?",
    )
    .bind(cutoff)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests;
