#![cfg(test)]
//! Why the shelf count is what it is.
//!
//! Stock is a ledger with a cache in front of it. `stock_movements` records
//! every event that moved a quantity — what moved, what the total became, who
//! did it, on which terminal, and which document it belongs to.
//! `stock_levels` is the running total, kept because every scan reads it.
//!
//! The equation these hold the application to:
//!
//! ```text
//! quantity = quantity_after(earliest movement) + sum(quantity_delta of every later one)
//! ```
//!
//! A cache that disagrees with its ledger is a number nobody can explain, and
//! the ways it happens are all the same shape: something wrote the quantity
//! without recording why. So these drive the real operations — a sale, a refund,
//! a void, a merge — and after each one ask whether the books still add up.

mod lifecycle;
mod provenance;
mod resilience;

use crate::inventory::reconcile;
use sqlx::SqlitePool;

pub(super) const PRODUCT: &str = "prd_inv";

pub(super) async fn branch_of(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT branch_id FROM stock_levels WHERE product_id = ?")
        .bind(PRODUCT)
        .fetch_one(pool)
        .await
        .expect("the seeded product has a stock row")
}

/// What every screen shows.
pub(super) async fn cached(pool: &SqlitePool) -> f64 {
    sqlx::query_scalar::<_, String>(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ?",
    )
    .bind(PRODUCT)
    .fetch_one(pool)
    .await
    .expect("stock row")
    .parse()
    .expect("a numeric quantity")
}

/// Assert the cache and the ledger agree, for every product in the database.
pub(super) async fn books_balance(pool: &SqlitePool, after_what: &str) {
    let drifted = reconcile::discrepancies(pool, None, 0.0001)
        .await
        .expect("reconciliation must be able to run");
    assert!(
        drifted.is_empty(),
        "after {after_what} the shelf count no longer matches the movements that made it: {:?}",
        drifted
            .iter()
            .map(|d| format!(
                "{} cached {} vs ledger {} ({} movements)",
                d.product_name, d.cached_quantity, d.ledger_quantity, d.movement_count
            ))
            .collect::<Vec<_>>()
    );
}
