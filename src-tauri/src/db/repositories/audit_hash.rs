/// Audit hash-chain utilities.
///
/// Every audit_log row carries two fields:
///   `hash`          — SHA-256 of the row's canonical content + previous hash
///   `previous_hash` — the hash of the immediately preceding row for this device
///
/// Chain integrity can be verified offline or on demand; broken links indicate
/// tampering or data loss.
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use crate::errors::AppResult;

// ── Hash computation ───────────────────────────────────────────────────────────

/// Fields passed to the hash function for one audit_log row.
pub struct AuditHashInput<'a> {
    pub audit_log_id:  &'a str,
    pub event_type:    &'a str,
    pub entity_type:   &'a str,
    pub entity_id:     &'a str,
    pub actor_user_id: &'a str,
    pub created_at:    &'a str,
    pub after_json:    Option<&'a str>,
    pub previous_hash: &'a str,
}

/// Compute the SHA-256 chain hash for one audit_log row.
///
/// Inputs are NUL-separated so that different fields cannot be spliced together
/// to produce the same canonical string.
pub fn compute_audit_hash(i: &AuditHashInput<'_>) -> String {
    let mut h = Sha256::new();
    h.update(i.audit_log_id.as_bytes());  h.update(b"\x00");
    h.update(i.event_type.as_bytes());    h.update(b"\x00");
    h.update(i.entity_type.as_bytes());   h.update(b"\x00");
    h.update(i.entity_id.as_bytes());     h.update(b"\x00");
    h.update(i.actor_user_id.as_bytes()); h.update(b"\x00");
    h.update(i.created_at.as_bytes());    h.update(b"\x00");
    h.update(i.after_json.unwrap_or("").as_bytes()); h.update(b"\x00");
    h.update(i.previous_hash.as_bytes());
    hex::encode(h.finalize())
}

// ── Chain tip ─────────────────────────────────────────────────────────────────

/// Fetch the hash of the most recent audit_log entry for the given device.
/// Returns an empty string if no entry exists yet (genesis state).
///
/// We only look at rows whose `hash` is exactly 64 hex chars (SHA-256 length)
/// to ignore legacy placeholder hashes.
pub async fn fetch_last_hash(pool: &SqlitePool, device_id: &str) -> AppResult<String> {
    let h: Option<String> = sqlx::query_scalar(
        "SELECT hash FROM audit_logs
         WHERE device_id = ? AND length(hash) = 64
         ORDER BY created_at DESC, audit_log_id DESC
         LIMIT 1"
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    Ok(h.unwrap_or_default())
}

// ── Chain verification ────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct ChainVerifyResult {
    pub total_rows:   i64,
    pub legacy_rows:  i64,   // placeholder hashes (< 64 chars) — not verifiable
    pub verified:     i64,   // SHA-256 rows with correct hash + correct prev link
    pub broken_hash:  i64,   // hash recomputation mismatch
    pub broken_link:  i64,   // previous_hash doesn't match prior row's hash
    pub ok:           bool,
}

/// Walk all audit_log rows for `device_id` (oldest first) and verify the chain.
/// Rows with legacy hashes are counted but skipped in the chain walk.
pub async fn verify_chain(pool: &SqlitePool, device_id: &str) -> AppResult<ChainVerifyResult> {
    struct AuditRow {
        audit_log_id:  String,
        event_type:    String,
        entity_type:   String,
        entity_id:     String,
        actor_user_id: String,
        created_at:    String,
        after_json:    Option<String>,
        hash:          String,
        previous_hash: Option<String>,
    }

    let raw = sqlx::query(
        "SELECT audit_log_id, event_type, entity_type, entity_id,
                COALESCE(actor_user_id, '') AS actor_user_id,
                created_at, after_json, hash, previous_hash
         FROM audit_logs
         WHERE device_id = ?
         ORDER BY created_at ASC, audit_log_id ASC"
    )
    .bind(device_id)
    .fetch_all(pool)
    .await?;

    let rows: Vec<AuditRow> = raw.iter().map(|r| AuditRow {
        audit_log_id:  r.get("audit_log_id"),
        event_type:    r.get("event_type"),
        entity_type:   r.get("entity_type"),
        entity_id:     r.get("entity_id"),
        actor_user_id: r.get("actor_user_id"),
        created_at:    r.get("created_at"),
        after_json:    r.get("after_json"),
        hash:          r.get("hash"),
        previous_hash: r.get("previous_hash"),
    }).collect();

    let total_rows = rows.len() as i64;
    let mut verified      = 0i64;
    let mut broken_hash   = 0i64;
    let mut broken_link   = 0i64;

    // Only SHA-256 rows participate in the chain walk
    let chain_rows: Vec<&AuditRow> = rows.iter()
        .filter(|r| r.hash.len() == 64)
        .collect();

    let legacy_rows = total_rows - chain_rows.len() as i64;

    let mut prev_hash = String::new(); // genesis

    for row in &chain_rows {
        // Verify stored previous_hash matches our running prev_hash
        let stored_prev = row.previous_hash.as_deref().unwrap_or("");
        if stored_prev != prev_hash {
            broken_link += 1;
            // Keep walking — don't reset prev_hash so chain stays comparable
        }

        // Recompute hash and compare
        let expected = compute_audit_hash(&AuditHashInput {
            audit_log_id:  &row.audit_log_id,
            event_type:    &row.event_type,
            entity_type:   &row.entity_type,
            entity_id:     &row.entity_id,
            actor_user_id: &row.actor_user_id,
            created_at:    &row.created_at,
            after_json:    row.after_json.as_deref(),
            previous_hash: stored_prev,
        });
        if expected != row.hash {
            broken_hash += 1;
        } else {
            verified += 1;
        }

        prev_hash = row.hash.clone();
    }

    let ok = broken_hash == 0 && broken_link == 0;
    Ok(ChainVerifyResult { total_rows, legacy_rows, verified, broken_hash, broken_link, ok })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn inp<'a>(id: &'a str, et: &'a str, eid: &'a str, aj: Option<&'a str>, ph: &'a str) -> AuditHashInput<'a> {
        AuditHashInput { audit_log_id: id, event_type: et, entity_type: "sale",
            entity_id: eid, actor_user_id: "U1", created_at: "t", after_json: aj, previous_hash: ph }
    }

    #[test]
    fn hash_is_deterministic() {
        let i = AuditHashInput { audit_log_id: "id1", event_type: "sale.created",
            entity_type: "sale", entity_id: "S1", actor_user_id: "U1",
            created_at: "2024-01-01T00:00:00Z", after_json: Some(r#"{"k":"v"}"#), previous_hash: "" };
        let h1 = compute_audit_hash(&i);
        let h2 = compute_audit_hash(&i);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn hash_changes_with_previous_hash() {
        let h1 = compute_audit_hash(&inp("id1", "sale.created", "S1", None, ""));
        let h2 = compute_audit_hash(&inp("id1", "sale.created", "S1", None, &h1));
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_changes_with_any_field() {
        let base = compute_audit_hash(&inp("id1", "sale.created", "S1", None, ""));
        assert_ne!(base, compute_audit_hash(&inp("id2", "sale.created", "S1", None, "")));
        assert_ne!(base, compute_audit_hash(&inp("id1", "sale.voided",  "S1", None, "")));
        assert_ne!(base, compute_audit_hash(&inp("id1", "sale.created", "S2", None, "")));
    }
}
