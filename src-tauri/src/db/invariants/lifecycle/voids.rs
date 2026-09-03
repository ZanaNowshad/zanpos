#![cfg(test)]
//! Cancelling a sale after it is rung up.
//!
//! A void reverses a completed sale. The goods go back on the shelf count,
//! and the return is a movement like any other so the ledger still explains
//! the quantity on hand.
//!
//! Part of the till lifecycle suite; the shared fixtures live in
//! [`super`](super).

use super::super::{migrated_pool, CASHIER, DEVICE};
use super::{cash, due, line, stock_of, till};
use crate::db::repositories::sale_repo;

// ── Cancelling a sale after it is rung up ──────────────────────────────

/// Voiding a sale puts the stock back and says so in the ledger.
///
/// The customer changes their mind after the receipt prints. What matters is
/// that the goods return to the shelf count and that the return is a movement
/// like any other, so the ledger still explains the number on hand — an
/// adjustment made by editing `stock_levels` alone would leave the cache and the
/// ledger disagreeing, which is the one thing stock accounting cannot survive.
#[tokio::test]
async fn voiding_a_sale_returns_the_stock_through_the_ledger() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    let before = stock_of(&pool, "prd_inv").await;
    cart.lines.push(line("prd_inv", "Cola 330ml", "3", 1000));

    let result = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(due(&cart)),
        "life-void",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("checkout");

    let sold = stock_of(&pool, "prd_inv").await;
    assert_eq!(sold, before - 3.0, "the sale took three off the shelf");

    let branch: String = sqlx::query_scalar("SELECT branch_id FROM sales WHERE sale_id = ?")
        .bind(&result.sale_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    crate::inventory::movements::return_void_sale(
        &mut tx,
        &result.sale_id,
        CASHIER,
        &branch,
        DEVICE,
    )
    .await
    .expect("void the sale");
    tx.commit().await.unwrap();

    assert_eq!(
        stock_of(&pool, "prd_inv").await,
        before,
        "voiding must put all three back"
    );

    let (kind, delta): (String, String) = sqlx::query_as(
        "SELECT movement_type, quantity_delta FROM stock_movements
          WHERE reference_id = ? AND CAST(quantity_delta AS REAL) > 0",
    )
    .bind(&result.sale_id)
    .fetch_one(&pool)
    .await
    .expect("the return must be a movement, not a silent edit to the cache");
    assert_eq!(delta.parse::<f64>().unwrap(), 3.0);
    assert!(
        !kind.is_empty(),
        "the movement needs a type so the ledger can be read back"
    );

    // The cache and the ledger still agree, which is the invariant a void is
    // most likely to break.
    let ledger: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(CAST(quantity_delta AS REAL)), 0) FROM stock_movements
          WHERE product_id = 'prd_inv'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        stock_of(&pool, "prd_inv").await - before,
        ledger,
        "the movements must add up to the change in the cached quantity"
    );
}
