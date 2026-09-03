//! Hash-chain tests, split from `audit_hash.rs` to keep it under the 500-line
//! rule — the same `foo.rs` + `foo/tests.rs` shape used by `registry`,
//! `device_state` and the rest of this codebase.

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

// ── Verification against a chain that no longer starts at genesis ─────────
//
// Retention pruning removes aged audit rows, so the oldest surviving row on
// any long-running install points at a predecessor that is gone. That is
// routine housekeeping, not tampering, and the walk has to tell the two
// apart — deleting from the middle must still be caught.

async fn chained_pool(device: &str, entries: usize) -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    for index in 0..entries {
        insert_audit_entry(
            &pool,
            "SALE_COMPLETED",
            "sale",
            &format!("S{index}"),
            "U1",
            "user",
            device,
            "BR1",
            None,
            None,
            None,
        )
        .await
        .unwrap();
    }
    pool
}

async fn ids_oldest_first(pool: &SqlitePool, device: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT audit_log_id FROM audit_logs WHERE device_id = ?
         ORDER BY created_at ASC, audit_log_id ASC",
    )
    .bind(device)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn an_untouched_chain_verifies() {
    let pool = chained_pool("DEV1", 4).await;
    let result = verify_chain(&pool, "DEV1").await.unwrap();
    assert_eq!(result.verified, 4);
    assert_eq!(result.broken_link, 0);
    assert!(result.ok);
}

/// The regression this exists for: pruning the oldest entries is what the
/// retention setting is *for*, and it used to make every subsequent
/// verification report a broken link — on every healthy shop, for ever.
#[tokio::test]
async fn pruning_the_oldest_entries_does_not_read_as_tampering() {
    let pool = chained_pool("DEV1", 5).await;
    let ids = ids_oldest_first(&pool, "DEV1").await;

    sqlx::query("DELETE FROM audit_logs WHERE audit_log_id IN (?, ?)")
        .bind(&ids[0])
        .bind(&ids[1])
        .execute(&pool)
        .await
        .unwrap();

    let result = verify_chain(&pool, "DEV1").await.unwrap();
    assert_eq!(result.total_rows, 3);
    assert_eq!(result.verified, 3);
    assert_eq!(result.broken_link, 0, "aged-out entries are not tampering");
    assert!(result.ok);
}

/// And the other half, which is the whole point of keeping the check: a row
/// removed from the middle leaves a gap the following row cannot explain.
#[tokio::test]
async fn removing_an_interior_entry_still_breaks_the_chain() {
    let pool = chained_pool("DEV1", 5).await;
    let ids = ids_oldest_first(&pool, "DEV1").await;

    sqlx::query("DELETE FROM audit_logs WHERE audit_log_id = ?")
        .bind(&ids[2])
        .execute(&pool)
        .await
        .unwrap();

    let result = verify_chain(&pool, "DEV1").await.unwrap();
    assert_eq!(result.broken_link, 1, "an interior deletion must be caught");
    assert!(!result.ok);
}

/// Editing a retained row changes its hash, which the recomputation catches
/// regardless of where the chain now begins.
#[tokio::test]
async fn editing_a_retained_row_is_still_caught_after_pruning() {
    let pool = chained_pool("DEV1", 4).await;
    let ids = ids_oldest_first(&pool, "DEV1").await;

    sqlx::query("DELETE FROM audit_logs WHERE audit_log_id = ?")
        .bind(&ids[0])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE audit_logs SET entity_id = 'TAMPERED' WHERE audit_log_id = ?")
        .bind(&ids[2])
        .execute(&pool)
        .await
        .unwrap();

    let result = verify_chain(&pool, "DEV1").await.unwrap();
    assert_eq!(result.broken_hash, 1);
    assert!(!result.ok);
}
