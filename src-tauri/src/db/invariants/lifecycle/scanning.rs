#![cfg(test)]
//! The scan door.
//!
//! What a barcode resolves to, and what it must not. Everything a cashier
//! rings up passes through here, so a withdrawn product or a retired code
//! has to stop at this point rather than at the Charge button.
//!
//! Part of the till lifecycle suite; the shared fixtures live in
//! [`super`](super).

use super::super::{migrated_pool, seed};
use crate::db::repositories::product_repo;
use sqlx::SqlitePool;

// ── The scan door ────────────────────────────────────────────────────────────

/// Give the seeded product a barcode on the product row and a second one in
/// `product_barcodes`, which is how a shop ends up with two codes for one item.
async fn barcode_the_product(pool: &SqlitePool) {
    sqlx::query("UPDATE products SET barcode = '5449000000996' WHERE product_id = 'prd_inv'")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO product_barcodes (barcode_id, product_id, barcode, created_at, updated_at)
         VALUES ('bc_alias', 'prd_inv', '0000000000017', datetime('now'), datetime('now'))",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// Both the product's own barcode and an alias find the same item.
#[tokio::test]
async fn a_scan_finds_the_product_by_either_barcode() {
    let pool = migrated_pool().await;
    seed(&pool).await;
    barcode_the_product(&pool).await;

    for code in ["5449000000996", "0000000000017"] {
        let found = product_repo::get_product_by_barcode(&pool, code)
            .await
            .expect("the scan must not error")
            .unwrap_or_else(|| panic!("scanning {code} found nothing"));
        assert_eq!(found.product.product_id, "prd_inv");
        assert_eq!(
            found.price_minor, 1000,
            "the scan carries the catalogue price"
        );
    }
}

/// A code nobody has registered scans as nothing, rather than as something else.
#[tokio::test]
async fn an_unknown_barcode_finds_nothing() {
    let pool = migrated_pool().await;
    seed(&pool).await;
    barcode_the_product(&pool).await;

    assert!(
        product_repo::get_product_by_barcode(&pool, "9999999999999")
            .await
            .expect("an unknown code is not an error")
            .is_none(),
        "an unregistered barcode must not resolve to a product"
    );
}

/// A discontinued product cannot be scanned back onto a basket.
///
/// Deactivating is how the back office takes something off sale, so the scan
/// door is where it has to take effect — including when the item still has a
/// price, stock and a working barcode, which it usually does.
#[tokio::test]
async fn a_deactivated_product_cannot_be_scanned() {
    let pool = migrated_pool().await;
    seed(&pool).await;
    barcode_the_product(&pool).await;

    sqlx::query("UPDATE products SET is_active = 0 WHERE product_id = 'prd_inv'")
        .execute(&pool)
        .await
        .unwrap();

    assert!(
        product_repo::get_product_by_barcode(&pool, "5449000000996")
            .await
            .expect("scanning a withdrawn product is not an error")
            .is_none(),
        "a withdrawn product must not scan"
    );
}

/// Retiring one of two barcodes retires only that one.
#[tokio::test]
async fn a_retired_barcode_stops_scanning_but_the_product_does_not() {
    let pool = migrated_pool().await;
    seed(&pool).await;
    barcode_the_product(&pool).await;

    sqlx::query("UPDATE product_barcodes SET deleted_at = datetime('now') WHERE barcode_id = ?")
        .bind("bc_alias")
        .execute(&pool)
        .await
        .unwrap();

    assert!(
        product_repo::get_product_by_barcode(&pool, "0000000000017")
            .await
            .unwrap()
            .is_none(),
        "a retired alias must stop resolving"
    );
    assert!(
        product_repo::get_product_by_barcode(&pool, "5449000000996")
            .await
            .unwrap()
            .is_some(),
        "retiring an alias must not take the product off sale"
    );
}
