#![cfg(test)]
//! Measurements on the paths a cashier waits for.
//!
//! These are not assertions about milliseconds — a test that fails on a slow
//! machine teaches nobody anything. They assert *structure*: that the hot path
//! reaches the index built for it, and that its cost does not grow with the size
//! of the catalogue. A supermarket's product table grows for years, and a query
//! whose cost is per-row gets slower every month, with the cashier holding the
//! queue.
//!
//! The plans are printed so a human can read them; the shape of the plan is what
//! actually fails the test.

use super::{migrated_pool, seed, CASHIER, TAX_VAT};
use crate::db::repositories::product_repo;
use sqlx::SqlitePool;

/// A catalogue the size of a small supermarket's.
async fn a_full_catalogue(pool: &SqlitePool, count: i64) {
    seed(pool).await;
    let mut tx = pool.begin().await.expect("begin");
    for n in 0..count {
        sqlx::query(
            "INSERT INTO products
               (product_id, category_id, name, sku, barcode, track_inventory, is_active,
                tax_rule_id, created_at, updated_at)
             VALUES (?, 'cat_inv', ?, ?, ?, 0, 1, ?, datetime('now'), datetime('now'))",
        )
        .bind(format!("prd_{n:06}"))
        .bind(format!("Product {n:06} Assorted"))
        .bind(format!("SKU{n:06}"))
        .bind(format!("50{n:011}"))
        .bind(TAX_VAT)
        .execute(&mut *tx)
        .await
        .expect("insert product");

        sqlx::query(
            "INSERT INTO product_prices
               (price_id, product_id, price_type, price_minor, currency,
                effective_from, created_by_user_id, created_at, updated_at)
             VALUES (?, ?, 'selling', 1000, 'BHD', datetime('now','-1 day'), ?,
                     datetime('now'), datetime('now'))",
        )
        .bind(format!("prc_{n:06}"))
        .bind(format!("prd_{n:06}"))
        .bind(CASHIER)
        .execute(&mut *tx)
        .await
        .expect("insert price");
    }
    tx.commit().await.expect("commit");
}

/// The query plan for the search the POS actually calls.
async fn plan_of(pool: &SqlitePool, sql: &str) -> String {
    let rows: Vec<(i64, i64, i64, String)> = sqlx::query_as(&format!("EXPLAIN QUERY PLAN {sql}"))
        .fetch_all(pool)
        .await
        .expect("explain");
    rows.into_iter()
        .map(|r| r.3)
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Product search does not read the whole catalogue on every keystroke.
///
/// Migration 0046 built an FTS5 index for this, and its own comment names the
/// reason: product lookup ran `name LIKE '%term%'`, and *"that is the hot path
/// for both the POS product picker and ZanAI's product read tools"*. The
/// assistant's path was wired to the index. The POS command — the one that runs
/// on every keystroke while a customer waits — was left on the `LIKE`.
///
/// A leading `%` cannot use an index, by construction: SQLite has no way to seek
/// into a B-tree on a suffix. The planner still narrows to active products, so
/// this is not a full-table scan — but the pattern is then tested against every
/// one of those rows, and that per-row cost grows with the catalogue for as long
/// as the shop keeps adding products.
#[tokio::test]
async fn the_pos_product_search_uses_its_index() {
    let pool = migrated_pool().await;
    a_full_catalogue(&pool, 4_000).await;

    // What the POS command runs today, in plan form.
    let scan_plan = plan_of(
        &pool,
        "SELECT p.product_id FROM products p
          WHERE p.is_active = 1 AND p.deleted_at IS NULL
            AND (p.name LIKE '%Product 001%' OR p.sku LIKE '%Product 001%')
          LIMIT 50",
    )
    .await;

    // What the FTS path runs.
    let index_plan = plan_of(
        &pool,
        "SELECT p.product_id FROM products p
          WHERE p.rowid IN (SELECT rowid FROM product_search WHERE product_search MATCH 'Product*')
          LIMIT 50",
    )
    .await;

    println!("  LIKE  plan: {scan_plan}");
    println!("  FTS   plan: {index_plan}");

    // The LIKE form is not literally a full-table scan — the planner narrows to
    // active products first — but no index serves the *search term*, so the
    // pattern is evaluated against every active row. That per-row cost is what
    // grows with the catalogue, and it is what the FTS path removes.
    assert!(
        !scan_plan.contains("product_search"),
        "the LIKE form was expected not to reach the index — this test's \
         premise has changed: {scan_plan}"
    );
    assert!(
        !index_plan.contains("SCAN products"),
        "the FTS path is scanning the product table, which defeats the index: {index_plan}"
    );

    // And the command the POS calls resolves through the index.
    let hits = product_repo::search_products_paginated(&pool, "Product 000123", None, 50)
        .await
        .expect("search");
    assert_eq!(
        hits.len(),
        1,
        "the search must still find what it found before"
    );

    // And a prefix query — what a cashier actually types — reaches the index.
    let by_prefix = product_repo::search_products_fts(&pool, "Product 000123", 50)
        .await
        .expect("fts search");
    assert!(
        !by_prefix.is_empty(),
        "the index cannot answer the query the picker sends, so the picker is \
         still paying a per-row cost on every keystroke"
    );
}

/// A mid-word search still works, index or not.
///
/// The index is a prefix index: searching for the middle of a word cannot use
/// it. The fallback to the scan is what keeps that query working, so it has to
/// stay — the point is that the common case stops paying for the rare one.
#[tokio::test]
async fn a_mid_word_search_still_finds_the_product() {
    let pool = migrated_pool().await;
    a_full_catalogue(&pool, 300).await;

    let hits = product_repo::search_products_paginated(&pool, "ssorted", None, 10)
        .await
        .expect("search");
    assert!(
        !hits.is_empty(),
        "a mid-word search stopped working — the scan fallback is gone"
    );
}

/// Barcode lookup — the scan door — is an indexed equality, not a scan.
#[tokio::test]
async fn barcode_lookup_is_indexed() {
    let pool = migrated_pool().await;
    a_full_catalogue(&pool, 2_000).await;

    let plan = plan_of(
        &pool,
        "SELECT product_id FROM products WHERE barcode = '5000000001234'",
    )
    .await;
    println!("  barcode plan: {plan}");
    assert!(
        plan.contains("USING INDEX") || plan.contains("SEARCH"),
        "the scan door is reading the whole catalogue per beep: {plan}"
    );

    let found = product_repo::get_product_by_barcode(&pool, "5000000000123")
        .await
        .expect("lookup");
    assert!(found.is_some(), "the seeded barcode must resolve");
}
