#![cfg(test)]
//! One product, one price, however you ask.
//!
//! The selling price lives in `product_prices`, never on `products`, and the
//! question "what does this cost right now" has one correct answer: the newest
//! `selling` row whose effective window is open. Every screen that shows a price
//! and every path that charges one has to resolve it the same way, because the
//! places they disagree are exactly the places a cashier sees one number and the
//! till charges another — or refuses the sale.
//!
//! These call the real production lookups rather than re-stating their SQL. A
//! test that inlines the predicate proves the predicate, not the code.

mod barcodes;
mod history;
mod prices;

use super::CASHIER;
use crate::db::repositories::{product_repo, sale_repo};
use sqlx::SqlitePool;

/// Add a `selling` price row, exactly as the price commands write them.
async fn price_row(pool: &SqlitePool, price_id: &str, minor: i64, from: &str, to: Option<&str>) {
    sqlx::query(
        "INSERT INTO product_prices
           (price_id, product_id, price_type, price_minor, currency,
            effective_from, effective_to, created_by_user_id, created_at, updated_at)
         VALUES (?, 'prd_inv', 'selling', ?, 'BHD', ?, ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(price_id)
    .bind(minor)
    .bind(from)
    .bind(to)
    .bind(CASHIER)
    .execute(pool)
    .await
    .expect("write a price row");
}

/// What checkout would charge for the seeded product.
async fn what_the_till_charges(pool: &SqlitePool) -> i64 {
    *sale_repo::current_selling_prices(pool, &["prd_inv"])
        .await
        .expect("resolve the selling price")
        .get("prd_inv")
        .expect("the product has a price in force")
}

/// What the scan door and every product screen show.
async fn what_the_screen_shows(pool: &SqlitePool) -> i64 {
    product_repo::get_product_by_id(pool, "prd_inv")
        .await
        .expect("look the product up")
        .expect("the product exists")
        .price_minor
}
