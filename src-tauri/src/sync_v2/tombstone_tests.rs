//! Deletion has to travel, in both directions, and not come back on its own.
//!
//! `deleted_at` was in `should_skip_column`, which is used for three different
//! things: the parity checksum, the JSON pushed to the hub, and the JSON the hub
//! serves on pull. So a deletion was stripped at every boundary — it never left
//! the terminal that made it, and the checksum could not see that the terminals
//! disagreed.
//!
//! `products` survived by accident: `soft_delete_product` also clears
//! `is_active`, that column does sync, and every catalogue read filters on both.
//! `customers` and `shifts` have no such flag. A customer deleted at the back
//! office stayed on every till, permanently, and parity reported 100%.
//!
//! The four directions below are the contract. The fifth test is the one that
//! matters most in a shop: an old edit arriving late must not raise the dead.

use crate::sync_v2::apply::apply_row;
use serde_json::{json, Value};
use sqlx::SqlitePool;

async fn pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

/// A customer row as the hub would serve it.
fn customer(name: &str, updated_at: &str, deleted_at: Option<&str>) -> Value {
    json!({
        "customer_id": "cus_1",
        "branch_id": "br-1",
        "origin_device_id": "dev-other",
        "name": name,
        "phone": "+97333000111",
        "loyalty_points": 0,
        "created_at": "2026-08-01T00:00:00Z",
        "updated_at": updated_at,
        "deleted_at": deleted_at,
    })
}

async fn live_customers(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM customers WHERE deleted_at IS NULL")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn deleted_at_of(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar("SELECT deleted_at FROM customers WHERE customer_id='cus_1'")
        .fetch_one(pool)
        .await
        .unwrap()
}

// ── 1. hub delete → terminal ────────────────────────────────────────────────

/// The case that was completely broken: a customer removed at the back office
/// stayed live on every till, for good.
#[tokio::test]
async fn a_deletion_arriving_from_the_hub_removes_the_row_locally() {
    let pool = pool().await;
    apply_row(
        &pool,
        "customers",
        &customer("Ahmed", "2026-08-01T10:00:00Z", None),
    )
    .await
    .unwrap();
    assert_eq!(live_customers(&pool).await, 1);

    apply_row(
        &pool,
        "customers",
        &customer(
            "Ahmed",
            "2026-08-02T10:00:00Z",
            Some("2026-08-02T10:00:00Z"),
        ),
    )
    .await
    .unwrap();

    assert_eq!(live_customers(&pool).await, 0, "the deletion did not land");
    assert!(deleted_at_of(&pool).await.is_some());
}

// ── 2. hub restore → terminal ───────────────────────────────────────────────

/// Deletion must not be a one-way door. Nulls are skipped everywhere else so a
/// partial row cannot blank a column, and that rule silently made restores
/// impossible.
#[tokio::test]
async fn a_restore_arriving_from_the_hub_brings_the_row_back() {
    let pool = pool().await;
    apply_row(
        &pool,
        "customers",
        &customer(
            "Ahmed",
            "2026-08-02T10:00:00Z",
            Some("2026-08-02T10:00:00Z"),
        ),
    )
    .await
    .unwrap();
    assert_eq!(live_customers(&pool).await, 0);

    apply_row(
        &pool,
        "customers",
        &customer("Ahmed", "2026-08-03T10:00:00Z", None),
    )
    .await
    .unwrap();

    assert_eq!(live_customers(&pool).await, 1, "the restore did not land");
    assert!(deleted_at_of(&pool).await.is_none());
}

// ── 3 & 4. terminal delete / restore → hub ──────────────────────────────────

/// The tombstone has to survive both filters, and they are now separate
/// questions: `skip_on_wire` decides what the hub ever learns, and
/// `skip_in_fingerprint` decides what two terminals must agree about. The
/// original bug needed only one of them to drop `deleted_at` to become
/// invisible, so both are asserted.
#[tokio::test]
async fn a_local_deletion_and_restore_are_both_included_in_what_is_pushed() {
    use crate::sync_v2::apply::{skip_in_fingerprint, skip_on_wire};

    for table in ["customers", "shifts", "products"] {
        assert!(
            !skip_on_wire(table, "deleted_at"),
            "{table} would push without its tombstone"
        );
        assert!(
            !skip_in_fingerprint(table, "deleted_at"),
            "{table} parity could not see a one-sided deletion"
        );
    }
    // The genuinely device-local columns stay stripped from the wire.
    for column in ["sync_status", "sync_attempts"] {
        assert!(skip_on_wire("customers", column), "{column}");
    }
    // `version` is the exception, and deliberately so: it is the only signal
    // that distinguishes a concurrent edit from an ordinary stale write, so it
    // travels — but it stays out of the fingerprint, because moving a checksum
    // makes every terminal report divergence at once.
    assert!(!skip_on_wire("customers", "version"));
    for column in ["sync_status", "sync_attempts", "version"] {
        assert!(skip_in_fingerprint("customers", column), "{column}");
    }
}

/// End to end through the same path a pull takes: delete here, and a hub
/// applying our row sees the tombstone; restore here, and it sees it cleared.
#[tokio::test]
async fn a_terminal_delete_then_restore_round_trips_to_another_node() {
    let terminal = pool().await;
    let hub = pool().await;

    apply_row(
        &terminal,
        "customers",
        &customer("Ahmed", "2026-08-01T10:00:00Z", None),
    )
    .await
    .unwrap();

    // Terminal deletes, exactly as `customer_delete` does.
    sqlx::query("UPDATE customers SET deleted_at=?, updated_at=? WHERE customer_id='cus_1'")
        .bind("2026-08-02T10:00:00Z")
        .bind("2026-08-02T10:00:00Z")
        .execute(&terminal)
        .await
        .unwrap();

    // What the push would carry, replayed into the other node.
    apply_row(
        &hub,
        "customers",
        &customer(
            "Ahmed",
            "2026-08-02T10:00:00Z",
            Some("2026-08-02T10:00:00Z"),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        live_customers(&hub).await,
        0,
        "delete did not reach the hub"
    );

    // And the reverse: restored here, cleared there.
    apply_row(
        &hub,
        "customers",
        &customer("Ahmed", "2026-08-03T10:00:00Z", None),
    )
    .await
    .unwrap();
    assert_eq!(
        live_customers(&hub).await,
        1,
        "restore did not reach the hub"
    );
}

// ── 5. The one that matters in a shop ───────────────────────────────────────

/// A till that was offline pushes an edit it made *before* the deletion. Without
/// the freshness guard on `updated_at`, that stale row would clear the tombstone
/// and the customer would silently come back.
#[tokio::test]
async fn a_stale_edit_arriving_late_cannot_resurrect_a_deleted_row() {
    let pool = pool().await;
    apply_row(
        &pool,
        "customers",
        &customer("Ahmed", "2026-08-01T10:00:00Z", None),
    )
    .await
    .unwrap();
    apply_row(
        &pool,
        "customers",
        &customer(
            "Ahmed",
            "2026-08-05T10:00:00Z",
            Some("2026-08-05T10:00:00Z"),
        ),
    )
    .await
    .unwrap();
    assert_eq!(live_customers(&pool).await, 0);

    // An edit made on 2 August, arriving after the 5 August deletion.
    apply_row(
        &pool,
        "customers",
        &customer("Ahmed Renamed", "2026-08-02T09:00:00Z", None),
    )
    .await
    .unwrap();

    assert_eq!(
        live_customers(&pool).await,
        0,
        "a stale edit raised a deleted customer"
    );
    // And it did not smuggle its other fields in either.
    let name: String = sqlx::query_scalar("SELECT name FROM customers WHERE customer_id='cus_1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name, "Ahmed");
}

/// The mirror: a genuinely newer restore must still win, or the guard has
/// simply replaced one broken direction with another.
#[tokio::test]
async fn a_newer_restore_still_wins_over_an_older_deletion() {
    let pool = pool().await;
    apply_row(
        &pool,
        "customers",
        &customer(
            "Ahmed",
            "2026-08-05T10:00:00Z",
            Some("2026-08-05T10:00:00Z"),
        ),
    )
    .await
    .unwrap();

    apply_row(
        &pool,
        "customers",
        &customer("Ahmed", "2026-08-06T10:00:00Z", None),
    )
    .await
    .unwrap();

    assert_eq!(live_customers(&pool).await, 1);
}

// ── Parity can now see it ───────────────────────────────────────────────────

/// The reason this was invisible: the checksum used the same skip list. A
/// deleted-here-not-there row had identical counts and identical checksums.
#[tokio::test]
async fn parity_now_reports_a_row_deleted_on_only_one_side() {
    use crate::sync_v2::parity::{bucket_digests, mismatched_buckets, DEFAULT_BUCKETS};

    let local = pool().await;
    let hub = pool().await;
    for node in [&local, &hub] {
        apply_row(
            node,
            "customers",
            &customer("Ahmed", "2026-08-01T10:00:00Z", None),
        )
        .await
        .unwrap();
    }
    assert!(mismatched_buckets(
        &bucket_digests(&local, "customers", DEFAULT_BUCKETS)
            .await
            .unwrap(),
        &bucket_digests(&hub, "customers", DEFAULT_BUCKETS)
            .await
            .unwrap(),
    )
    .is_empty());

    sqlx::query("UPDATE customers SET deleted_at='2026-08-02T10:00:00Z' WHERE customer_id='cus_1'")
        .execute(&local)
        .await
        .unwrap();

    assert!(
        !mismatched_buckets(
            &bucket_digests(&local, "customers", DEFAULT_BUCKETS)
                .await
                .unwrap(),
            &bucket_digests(&hub, "customers", DEFAULT_BUCKETS)
                .await
                .unwrap(),
        )
        .is_empty(),
        "a one-sided deletion still reads as identical"
    );
}
