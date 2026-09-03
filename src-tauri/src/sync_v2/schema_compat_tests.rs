//! Applying a row written by a build that is not this build.
//!
//! A fleet is normally mid-upgrade. The release system hands the new installer
//! to whichever till asks first, so for a stretch of hours or days the hub and
//! the terminals run different schema versions — and either side may be the
//! newer one. That is the designed behaviour of the rollout, not an edge case.
//!
//! Until this was fixed, any release that added a column to a synced table broke
//! sync for that table across the whole partly-upgraded fleet:
//!
//! * newer terminal → older hub: `push_table` answered 500 on the first row, and
//!   `upsert_rows` reads 5xx as transient, so the terminal retried the same
//!   batch for ever and that table never advanced again
//! * newer hub → older terminal: every row failed to apply, was retried five
//!   times, and was then quarantined — the whole table, row by row
//!
//! Neither is recoverable by waiting, and both land exactly when a release is
//! halfway through a shop.
//!
//! The rule these pin is the ordinary forward-compatibility rule for a
//! replicating system: write what you understand, ignore what you do not, never
//! fail the row. Nothing is dropped from the sender — the column it knows about
//! stays intact on its own node and arrives here when this node upgrades.

use super::apply::apply_row;
use serde_json::json;
use sqlx::SqlitePool;

/// A column name no release will ever add, standing in for one a future release
/// does add. Using a real future column would date this test the moment it lands.
const FROM_A_LATER_RELEASE: &str = "a_column_this_build_has_never_heard_of";

async fn migrated_pool() -> SqlitePool {
    let path = std::env::temp_dir().join(format!("zanpos_schema_{}.db", ulid::Ulid::new()));
    let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", path.display()))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

async fn seed_catalogue(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO categories (category_id, name, created_at, updated_at)
         VALUES ('cat_fwd','Grocery','2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// The last-writer-wins path, which carries the catalogue — the tables a release
/// is most likely to add a column to.
#[tokio::test]
async fn a_row_from_a_newer_build_applies_without_its_unknown_column() {
    let pool = migrated_pool().await;
    seed_catalogue(&pool).await;

    let row = json!({
        "product_id": "prd_fwd",
        "category_id": "cat_fwd",
        "name": "Rice 5kg",
        "is_active": 1,
        "created_at": "2026-08-01T00:00:00Z",
        "updated_at": "2026-08-02T00:00:00Z",
        FROM_A_LATER_RELEASE: "whatever the next release decided to store here",
    });

    apply_row(&pool, "products", &row)
        .await
        .expect("a column this build does not know must not fail the row");

    let name: String = sqlx::query_scalar("SELECT name FROM products WHERE product_id='prd_fwd'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        name, "Rice 5kg",
        "the columns this build does know were lost"
    );
}

/// The same rule on the append-only path, which carries the money. A sale that
/// cannot be applied is a sale this terminal does not have.
#[tokio::test]
async fn a_sale_from_a_newer_build_still_applies() {
    let pool = migrated_pool().await;
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();

    let row = json!({
        "sale_id": "SALE-FWD",
        "branch_id": branch,
        "device_id": "DEV-OTHER",
        "origin_device_id": "DEV-OTHER",
        "receipt_number": "FWD-0001",
        "shift_id": "SHIFT-FWD",
        "cashier_user_id": "01JUSER000000000000ADMIN1",
        "status": "completed",
        "gross_total_minor": 1500,
        "net_total_minor": 1500,
        "business_date": "2026-08-02",
        "idempotency_key": "SALE-FWD-ik",
        "sold_at": "2026-08-02T10:00:00Z",
        "created_at": "2026-08-02T10:00:00Z",
        "updated_at": "2026-08-02T10:00:00Z",
        FROM_A_LATER_RELEASE: 42,
    });

    apply_row(&pool, "sales", &row)
        .await
        .expect("a sale from a newer till must not be refused over a column name");

    let total: i64 =
        sqlx::query_scalar("SELECT net_total_minor FROM sales WHERE sale_id='SALE-FWD'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(total, 1500);
}

/// Ignoring the unknown must not become ignoring the unfamiliar. Every column
/// this build *does* have has to be written, or the fix would quietly narrow
/// replication instead of widening what it tolerates.
#[tokio::test]
async fn every_column_this_build_does_know_is_still_written() {
    let pool = migrated_pool().await;
    seed_catalogue(&pool).await;

    let row = json!({
        "product_id": "prd_full",
        "category_id": "cat_fwd",
        "name": "Sugar 1kg",
        "sku": "SKU-1",
        "barcode": "6291000000009",
        "description": "Fine white sugar",
        "reorder_point": 7,
        "cost_minor": 250,
        "is_active": 1,
        "created_at": "2026-08-01T00:00:00Z",
        "updated_at": "2026-08-02T00:00:00Z",
        FROM_A_LATER_RELEASE: "ignored",
    });

    apply_row(&pool, "products", &row).await.unwrap();

    let (sku, barcode, description, reorder, cost): (String, String, String, i64, i64) =
        sqlx::query_as(
            "SELECT sku, barcode, description, reorder_point, cost_minor
           FROM products WHERE product_id='prd_full'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(sku, "SKU-1");
    assert_eq!(barcode, "6291000000009");
    assert_eq!(description, "Fine white sugar");
    assert_eq!(reorder, 7);
    assert_eq!(cost, 250);
}

/// An update from a newer build must still update. The unknown column is dropped
/// from the statement, not the statement from the cycle.
#[tokio::test]
async fn an_update_from_a_newer_build_still_wins_on_the_columns_it_shares() {
    let pool = migrated_pool().await;
    seed_catalogue(&pool).await;

    apply_row(
        &pool,
        "products",
        &json!({
            "product_id": "prd_upd", "category_id": "cat_fwd", "name": "Old name",
            "is_active": 1,
            "created_at": "2026-08-01T00:00:00Z", "updated_at": "2026-08-01T00:00:00Z",
        }),
    )
    .await
    .unwrap();

    apply_row(
        &pool,
        "products",
        &json!({
            "product_id": "prd_upd", "category_id": "cat_fwd", "name": "New name",
            "is_active": 1,
            "created_at": "2026-08-01T00:00:00Z", "updated_at": "2026-08-05T00:00:00Z",
            FROM_A_LATER_RELEASE: true,
        }),
    )
    .await
    .unwrap();

    let name: String = sqlx::query_scalar("SELECT name FROM products WHERE product_id='prd_upd'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name, "New name");
}

/// A row that is *only* unknown columns has no primary key either, and is
/// refused rather than ignored.
///
/// Tolerating an unknown *column* and tolerating an unknown *identity* are not
/// the same concession. Without a key the row cannot be matched by
/// `ON CONFLICT`, and since SQLite lets a TEXT primary key be NULL and treats
/// NULLs as distinct, applying it would insert a fresh duplicate every cycle
/// rather than updating anything. Refusing surfaces it — retried, then
/// quarantined in full — instead of silently multiplying it or silently
/// discarding it.
#[tokio::test]
async fn a_row_with_no_identity_is_refused_rather_than_multiplied() {
    let pool = migrated_pool().await;
    let row = json!({ FROM_A_LATER_RELEASE: "entirely from the future" });

    let outcome = apply_row(&pool, "products", &row).await;
    assert!(
        outcome.is_err(),
        "a row with no primary key was accepted; applying it repeatedly would          insert a new NULL-keyed row each time"
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "the refused row still wrote something");
}
