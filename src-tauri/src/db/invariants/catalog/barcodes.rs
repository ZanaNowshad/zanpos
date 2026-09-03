#![cfg(test)]
//! Barcodes.
//!
//! A barcode is the shop's only physical handle on an item, so one code
//! must mean one product — across both places a product can claim one —
//! while a retired code stays free to be reused.
//!
//! Part of the product-master suite; the shared fixtures live in
//! [`super`](super).

use super::super::{migrated_pool, seed, TAX_VAT};
use crate::db::repositories::product_repo;
use sqlx::SqlitePool;

// ── Barcodes ─────────────────────────────────────────────────────────────────

/// A second product, so a barcode can be fought over.
async fn second_product(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO products
           (product_id, category_id, name, track_inventory, is_active, tax_rule_id,
            created_at, updated_at)
         VALUES ('prd_two','cat_inv','Water 500ml', 0, 1, ?, datetime('now'), datetime('now'))",
    )
    .bind(TAX_VAT)
    .execute(pool)
    .await
    .unwrap();
}

/// One product can carry several barcodes, and all of them find it.
///
/// A case of six and a single tin have different codes; both are the same
/// product as far as the till is concerned.
#[tokio::test]
async fn a_product_can_have_more_than_one_barcode() {
    let pool = migrated_pool().await;
    seed(&pool).await;
    sqlx::query("UPDATE products SET barcode = '1111111111111' WHERE product_id = 'prd_inv'")
        .execute(&pool)
        .await
        .unwrap();
    for (id, code) in [("bc_a", "2222222222222"), ("bc_b", "3333333333333")] {
        sqlx::query(
            "INSERT INTO product_barcodes (barcode_id, product_id, barcode, created_at, updated_at)
             VALUES (?, 'prd_inv', ?, datetime('now'), datetime('now'))",
        )
        .bind(id)
        .bind(code)
        .execute(&pool)
        .await
        .unwrap();
    }

    for code in ["1111111111111", "2222222222222", "3333333333333"] {
        let found = product_repo::get_product_by_barcode(&pool, code)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("scanning {code} found nothing"));
        assert_eq!(found.product.product_id, "prd_inv");
    }
}

/// The same barcode cannot be live on two products at once.
///
/// A barcode is the shop's only physical handle on an item. If two products
/// claim one, a scan resolves to whichever the query reaches first — so the
/// cashier rings up an arbitrary one of them, and which one can change between
/// scans. `products.barcode` and `product_barcodes` are two different places to
/// make that claim, each with its own unique index, and an index cannot see
/// across tables.
#[tokio::test]
async fn one_barcode_cannot_be_live_on_two_products() {
    let pool = migrated_pool().await;
    seed(&pool).await;
    second_product(&pool).await;

    sqlx::query("UPDATE products SET barcode = '4444444444444' WHERE product_id = 'prd_inv'")
        .execute(&pool)
        .await
        .unwrap();

    // The same code, claimed by the other product through the alias table.
    let clash = sqlx::query(
        "INSERT INTO product_barcodes (barcode_id, product_id, barcode, created_at, updated_at)
         VALUES ('bc_clash', 'prd_two', '4444444444444', datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await;

    assert!(
        clash.is_err(),
        "a barcode already live on one product was accepted onto another;          a scan of it now resolves to whichever row the query reaches first"
    );
}

/// Retiring a barcode frees it for the product it actually belongs to.
///
/// Codes get reused: a supplier drops a line and the code comes back on
/// something else. The unique index covers live rows only so a retired code can
/// be reassigned — and the old owner must stop answering to it.
#[tokio::test]
async fn a_retired_barcode_can_be_reassigned_to_another_product() {
    let pool = migrated_pool().await;
    seed(&pool).await;
    second_product(&pool).await;

    sqlx::query(
        "INSERT INTO product_barcodes (barcode_id, product_id, barcode, created_at, updated_at)
         VALUES ('bc_old', 'prd_inv', '5555555555555', datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        product_repo::get_product_by_barcode(&pool, "5555555555555")
            .await
            .unwrap()
            .unwrap()
            .product
            .product_id,
        "prd_inv"
    );

    sqlx::query(
        "UPDATE product_barcodes SET deleted_at = datetime('now') WHERE barcode_id = 'bc_old'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO product_barcodes (barcode_id, product_id, barcode, created_at, updated_at)
         VALUES ('bc_new', 'prd_two', '5555555555555', datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .expect("a retired code must be free to reassign");

    let found = product_repo::get_product_by_barcode(&pool, "5555555555555")
        .await
        .unwrap()
        .expect("the reassigned code still scans");
    assert_eq!(
        found.product.product_id, "prd_two",
        "the code still resolves to the product that gave it up"
    );
}

/// Merging duplicates keeps both products' barcodes scanning.
///
/// The point of merging two rows for the same item is that either code rings up
/// the survivor. `merge_products` moves stock, sales history and the alias to the
/// target and archives the source — but it never touched `product_barcodes`, so
/// the codes on the merged-away product stayed pointed at a row that is now
/// `is_active = 0`. `get_product_by_barcode` filters on active products, so those
/// codes stopped resolving to anything at all: the shop tidies its catalogue and
/// discovers at the till that half its barcodes have gone dead.
#[tokio::test]
async fn merging_two_products_carries_the_barcodes_across() {
    let pool = migrated_pool().await;
    seed(&pool).await;
    second_product(&pool).await;

    // The duplicate carries a code of its own.
    sqlx::query(
        "INSERT INTO product_barcodes (barcode_id, product_id, barcode, created_at, updated_at)
         VALUES ('bc_dup', 'prd_two', '6666666666666', datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE products SET barcode = '7777777777777' WHERE product_id = 'prd_two'")
        .execute(&pool)
        .await
        .unwrap();

    crate::db::repositories::product_dedup_repo::merge_products(
        &pool,
        "prd_two",
        "prd_inv",
        true,
        super::super::CASHIER,
    )
    .await
    .expect("merge the duplicate into the survivor");

    for code in ["6666666666666", "7777777777777"] {
        let found = product_repo::get_product_by_barcode(&pool, code)
            .await
            .expect("scanning must not error")
            .unwrap_or_else(|| {
                panic!("{code} stopped scanning after the merge — the shop lost a barcode")
            });
        assert_eq!(
            found.product.product_id, "prd_inv",
            "the code must now ring up the surviving product"
        );
    }
}

/// Two active products cannot share a SKU.
///
/// The SKU is what a purchase order, a stock count and a supplier price list all
/// name the item by. Two active products answering to one means a receipt lands
/// against an arbitrary one of them. Enforced case- and whitespace-insensitively,
/// because `ABC-1`, `abc-1 ` and `ABC-1` are the same code to everyone except a
/// byte comparison.
#[tokio::test]
async fn two_active_products_cannot_share_a_sku() {
    let pool = migrated_pool().await;
    seed(&pool).await;
    second_product(&pool).await;

    sqlx::query("UPDATE products SET sku = 'ABC-1' WHERE product_id = 'prd_inv'")
        .execute(&pool)
        .await
        .unwrap();

    for spelling in ["ABC-1", "abc-1", " ABC-1 "] {
        let clash = sqlx::query("UPDATE products SET sku = ? WHERE product_id = 'prd_two'")
            .bind(spelling)
            .execute(&pool)
            .await;
        assert!(
            clash.is_err(),
            "{spelling:?} was accepted as a second product's SKU while another \
             active product already uses ABC-1"
        );
    }
}

/// Archiving a product frees its SKU and barcode for the survivor.
///
/// The counterpart to the guard above: uniqueness is among *active* products, so
/// retiring a line must release its identifiers rather than reserving them
/// forever against a row nobody can see.
#[tokio::test]
async fn retiring_a_product_releases_its_sku_and_barcode() {
    let pool = migrated_pool().await;
    seed(&pool).await;
    second_product(&pool).await;

    sqlx::query(
        "UPDATE products SET sku = 'SHARED-1', barcode = '8888888888888'
          WHERE product_id = 'prd_inv'",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Still taken while the original is active.
    assert!(
        sqlx::query("UPDATE products SET sku = 'SHARED-1' WHERE product_id = 'prd_two'")
            .execute(&pool)
            .await
            .is_err()
    );

    sqlx::query(
        "UPDATE products SET is_active = 0, deleted_at = datetime('now')
          WHERE product_id = 'prd_inv'",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "UPDATE products SET sku = 'SHARED-1', barcode = '8888888888888'
          WHERE product_id = 'prd_two'",
    )
    .execute(&pool)
    .await
    .expect("a retired product must release its SKU and barcode");

    assert_eq!(
        product_repo::get_product_by_barcode(&pool, "8888888888888")
            .await
            .unwrap()
            .expect("the code scans")
            .product
            .product_id,
        "prd_two",
        "the code must resolve to the product that now holds it"
    );
}
