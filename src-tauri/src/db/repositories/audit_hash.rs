use crate::errors::AppResult;
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
use ulid::Ulid;

// ── Hash computation ───────────────────────────────────────────────────────────

/// Fields passed to the hash function for one audit_log row.
pub struct AuditHashInput<'a> {
    pub audit_log_id: &'a str,
    pub event_type: &'a str,
    pub entity_type: &'a str,
    pub entity_id: &'a str,
    pub actor_user_id: &'a str,
    pub actor_type: &'a str, // "user" or "ai"
    pub created_at: &'a str,
    pub before_json: Option<&'a str>, // pre-mutation snapshot
    pub after_json: Option<&'a str>,
    pub reason: Option<&'a str>, // human-readable reason
    pub previous_hash: &'a str,
}

/// Compute the SHA-256 chain hash for one audit_log row.
///
/// Inputs are NUL-separated so that different fields cannot be spliced together
/// to produce the same canonical string.
pub fn compute_audit_hash(i: &AuditHashInput<'_>) -> String {
    let mut h = Sha256::new();
    h.update(i.audit_log_id.as_bytes());
    h.update(b"\x00");
    h.update(i.event_type.as_bytes());
    h.update(b"\x00");
    h.update(i.entity_type.as_bytes());
    h.update(b"\x00");
    h.update(i.entity_id.as_bytes());
    h.update(b"\x00");
    h.update(i.actor_user_id.as_bytes());
    h.update(b"\x00");
    h.update(i.actor_type.as_bytes());
    h.update(b"\x00");
    h.update(i.created_at.as_bytes());
    h.update(b"\x00");
    h.update(i.before_json.unwrap_or("").as_bytes());
    h.update(b"\x00");
    h.update(i.after_json.unwrap_or("").as_bytes());
    h.update(b"\x00");
    h.update(i.reason.unwrap_or("").as_bytes());
    h.update(b"\x00");
    h.update(i.previous_hash.as_bytes());
    hex::encode(h.finalize())
}

// ── Consolidated insert helper (M16/M17/H8) ──────────────────────────────────

/// Write one audit_log row with a proper SHA-256 hash chain.
///
/// Replaces the 3-copy boilerplate that existed in cash_commands.rs (M16) and
/// the 4-copy pattern in pos_commands.rs (M17).  Also used to add audit trail
/// entries for product / category / tax_rule / user CRUD (H8).
///
/// `actor_type` should be `"user"` for human-initiated actions or `"ai"` for
/// AI-agent-initiated mutations.  `before_json` is the pre-mutation snapshot
/// (pass `None` for creates).  `reason` is a human-readable description of why
/// the mutation happened (pass `None` if not available).
#[allow(clippy::too_many_arguments)]
pub async fn insert_audit_entry(
    pool: &SqlitePool,
    event_type: &str,
    entity_type: &str,
    entity_id: &str,
    actor_user_id: &str,
    actor_type: &str,
    device_id: &str,
    branch_id: &str,
    before_json: Option<&str>,
    after_json: Option<&str>,
    reason: Option<&str>,
) -> AppResult<()> {
    insert_audit_entry_full(
        pool,
        event_type,
        entity_type,
        entity_id,
        actor_user_id,
        actor_type,
        device_id,
        branch_id,
        before_json,
        after_json,
        reason,
        false,
    )
    .await
}

/// Reserved for manager-override audit events. Not yet wired into the call path.
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub async fn insert_audit_entry_override(
    pool: &SqlitePool,
    event_type: &str,
    entity_type: &str,
    entity_id: &str,
    actor_user_id: &str,
    actor_type: &str,
    device_id: &str,
    branch_id: &str,
    before_json: Option<&str>,
    after_json: Option<&str>,
    reason: Option<&str>,
    override_used: bool,
) -> AppResult<()> {
    insert_audit_entry_full(
        pool,
        event_type,
        entity_type,
        entity_id,
        actor_user_id,
        actor_type,
        device_id,
        branch_id,
        before_json,
        after_json,
        reason,
        override_used,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn insert_audit_entry_full(
    pool: &SqlitePool,
    event_type: &str,
    entity_type: &str,
    entity_id: &str,
    actor_user_id: &str,
    actor_type: &str,
    device_id: &str,
    branch_id: &str,
    before_json: Option<&str>,
    after_json: Option<&str>,
    reason: Option<&str>,
    override_used: bool,
) -> AppResult<()> {
    let audit_log_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let prev_hash = fetch_last_hash(pool, device_id).await.unwrap_or_default();
    let hash = compute_audit_hash(&AuditHashInput {
        audit_log_id: &audit_log_id,
        event_type,
        entity_type,
        entity_id,
        actor_user_id,
        actor_type,
        created_at: &now,
        before_json,
        after_json,
        reason,
        previous_hash: &prev_hash,
    });

    sqlx::query(
        "INSERT INTO audit_logs
           (audit_log_id, event_type, entity_type, entity_id,
            actor_user_id, actor_type, device_id, origin_device_id, branch_id,
            before_json, after_json, reason,
            created_at, hash, previous_hash, override_used)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&audit_log_id)
    .bind(event_type)
    .bind(entity_type)
    .bind(entity_id)
    .bind(actor_user_id)
    .bind(actor_type)
    .bind(device_id)
    .bind(device_id)
    .bind(branch_id)
    .bind(before_json)
    .bind(after_json)
    .bind(reason)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash.clone())
    })
    .bind(override_used as i64)
    .execute(pool)
    .await?;

    Ok(())
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
         LIMIT 1",
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    Ok(h.unwrap_or_default())
}

// ── Chain verification ────────────────────────────────────────────────────────
//
// F-MED-09 — Known limitation (intentional, security-correct):
// Audit rows written before the SHA-256 hash chain was introduced carry a
// placeholder hash (< 64 chars). These `legacy_rows` are counted but NOT walked
// in the chain verification. This is deliberate: a tamper-evident hash chain
// derives each row's hash from the previous row's hash at WRITE time. Retroactively
// "backfilling" hashes for historical rows would produce a chain that validates but
// proves nothing — it cannot attest to data that existed before the chain began.
// The honest behaviour is therefore to surface legacy_rows as a distinct,
// unverifiable bucket rather than fake their integrity. New rows (post-upgrade)
// form a complete, verifiable chain from their genesis point.

#[derive(Debug, serde::Serialize)]
pub struct ChainVerifyResult {
    pub total_rows: i64,
    pub legacy_rows: i64, // placeholder hashes (< 64 chars) — unverifiable by design (F-MED-09)
    pub verified: i64,    // SHA-256 rows with correct hash + correct prev link
    pub broken_hash: i64, // hash recomputation mismatch
    pub broken_link: i64, // previous_hash doesn't match prior row's hash
    pub ok: bool,
}

/// Walk all audit_log rows for `device_id` (oldest first) and verify the chain.
/// Rows with legacy hashes are counted but skipped in the chain walk.
pub async fn verify_chain(pool: &SqlitePool, device_id: &str) -> AppResult<ChainVerifyResult> {
    struct AuditRow {
        audit_log_id: String,
        event_type: String,
        entity_type: String,
        entity_id: String,
        actor_user_id: String,
        actor_type: String,
        created_at: String,
        before_json: Option<String>,
        after_json: Option<String>,
        reason: Option<String>,
        hash: String,
        previous_hash: Option<String>,
    }

    let raw = sqlx::query(
        "SELECT audit_log_id, event_type, entity_type, entity_id,
                COALESCE(actor_user_id, '') AS actor_user_id,
                COALESCE(actor_type, 'user') AS actor_type,
                created_at, before_json, after_json, reason, hash, previous_hash
         FROM audit_logs
         WHERE device_id = ?
         ORDER BY created_at ASC, audit_log_id ASC",
    )
    .bind(device_id)
    .fetch_all(pool)
    .await?;

    let rows: Vec<AuditRow> = raw
        .iter()
        .map(|r| AuditRow {
            audit_log_id: r.get("audit_log_id"),
            event_type: r.get("event_type"),
            entity_type: r.get("entity_type"),
            entity_id: r.get("entity_id"),
            actor_user_id: r.get("actor_user_id"),
            actor_type: r.get("actor_type"),
            created_at: r.get("created_at"),
            before_json: r.get("before_json"),
            after_json: r.get("after_json"),
            reason: r.get("reason"),
            hash: r.get("hash"),
            previous_hash: r.get("previous_hash"),
        })
        .collect();

    let total_rows = rows.len() as i64;
    let mut verified = 0i64;
    let mut broken_hash = 0i64;
    let mut broken_link = 0i64;

    // Only SHA-256 rows participate in the chain walk
    let chain_rows: Vec<&AuditRow> = rows.iter().filter(|r| r.hash.len() == 64).collect();

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
            audit_log_id: &row.audit_log_id,
            event_type: &row.event_type,
            entity_type: &row.entity_type,
            entity_id: &row.entity_id,
            actor_user_id: &row.actor_user_id,
            actor_type: &row.actor_type,
            created_at: &row.created_at,
            before_json: row.before_json.as_deref(),
            after_json: row.after_json.as_deref(),
            reason: row.reason.as_deref(),
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
    Ok(ChainVerifyResult {
        total_rows,
        legacy_rows,
        verified,
        broken_hash,
        broken_link,
        ok,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn inp<'a>(
        id: &'a str,
        et: &'a str,
        eid: &'a str,
        aj: Option<&'a str>,
        ph: &'a str,
    ) -> AuditHashInput<'a> {
        AuditHashInput {
            audit_log_id: id,
            event_type: et,
            entity_type: "sale",
            entity_id: eid,
            actor_user_id: "U1",
            actor_type: "user",
            created_at: "t",
            before_json: None,
            after_json: aj,
            reason: None,
            previous_hash: ph,
        }
    }

    #[test]
    fn hash_is_deterministic() {
        let i = AuditHashInput {
            audit_log_id: "id1",
            event_type: "sale.created",
            entity_type: "sale",
            entity_id: "S1",
            actor_user_id: "U1",
            actor_type: "user",
            created_at: "2024-01-01T00:00:00Z",
            before_json: None,
            after_json: Some(r#"{"k":"v"}"#),
            reason: None,
            previous_hash: "",
        };
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
        assert_ne!(
            base,
            compute_audit_hash(&inp("id2", "sale.created", "S1", None, ""))
        );
        assert_ne!(
            base,
            compute_audit_hash(&inp("id1", "sale.voided", "S1", None, ""))
        );
        assert_ne!(
            base,
            compute_audit_hash(&inp("id1", "sale.created", "S2", None, ""))
        );
    }
}
