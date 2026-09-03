#![cfg(test)]
//! A cashier's shift, end to end, checked in the rows it leaves behind.
//!
//! Each test here is a thing that happens at a till — a basket with two lines, a
//! discount, a split payment, a double-pressed Charge button, a sale that fails
//! at the last step — and each asserts against `sales`, `sale_items`,
//! `payments`, `stock_levels`, `stock_movements` and `audit_logs` rather than
//! against what `finalize_sale` returned. A function can return `Ok` and leave
//! the books wrong; that is the whole reason these read the tables.
//!
//! The failure cases matter more than the successes. A till that cannot ring up
//! a sale is discovered in a minute; a till that half-rings one is discovered at
//! cash-up, or never.
//!
//! This file holds only what every part of the shift needs. The tests are split
//! by what a cashier is actually doing at the time.

mod checkout;
mod discounts;
mod interruptions;
mod pricing;
mod scanning;
mod voids;

use super::{seed, BRANCH_FALLBACK, CASHIER, DEVICE, TAX_VAT};
use crate::db::repositories::shift_repo;
use crate::domain::cart::{Cart, CartLine};
use crate::domain::sale::PaymentInput;
use sqlx::SqlitePool;

async fn seed_second_product(pool: &SqlitePool, branch: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO products
           (product_id, category_id, name, track_inventory, is_active, tax_rule_id,
            created_at, updated_at)
         VALUES ('prd_two','cat_inv','Bread 400g', 1, 1, ?, datetime('now'), datetime('now'))",
    )
    .bind(TAX_VAT)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO product_prices
           (price_id, product_id, price_type, price_minor, currency,
            effective_from, created_by_user_id, created_at, updated_at)
         VALUES ('prc_two','prd_two','selling', 450, 'BHD',
                 datetime('now','-1 day'), ?, datetime('now'), datetime('now'))",
    )
    .bind(CASHIER)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO stock_levels
           (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at)
         VALUES ('SL-prd_two','prd_two', ?, '50', datetime('now'), datetime('now'))",
    )
    .bind(branch)
    .execute(pool)
    .await
    .unwrap();
}

fn line(product: &str, name: &str, qty: &str, price: i64) -> CartLine {
    CartLine::new(
        Some(product.into()),
        name.into(),
        None,
        None,
        qty,
        price,
        TAX_VAT.into(),
        1000,
        false,
    )
}

/// An open shift and a cart bound to it.
async fn till(pool: &SqlitePool) -> (String, Cart) {
    let branch = seed(pool).await;
    seed_second_product(pool, &branch).await;
    let shift = shift_repo::open_shift(pool, &branch, DEVICE, CASHIER, 10_000)
        .await
        .expect("open shift");
    let cart = Cart::new(
        branch,
        DEVICE.into(),
        shift.shift_id.clone(),
        CASHIER.into(),
    );
    (shift.shift_id, cart)
}

fn cash(amount: i64) -> Vec<PaymentInput> {
    vec![PaymentInput {
        method: "cash".into(),
        amount_minor: amount,
        tendered_minor: Some(amount),
        external_reference: None,
    }]
}

fn due(cart: &Cart) -> i64 {
    (cart
        .lines
        .iter()
        .filter(|l| !l.voided)
        .map(|l| l.line_total_minor)
        .sum::<i64>()
        - cart.bill_discount_minor)
        .max(0)
}

// ── Failures leave no trace ──────────────────────────────────────────────────

/// After a refused sale the database must look exactly as it did before it.
async fn assert_pre_sale_state(pool: &SqlitePool, what: &str) {
    for (table, label) in [
        ("sales", "sale"),
        ("sale_items", "sale line"),
        ("payments", "payment"),
        ("stock_movements", "stock movement"),
    ] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "{what} left {count} {label} row(s) behind");
    }
    assert!(
        (stock_of(pool, "prd_inv").await - 100.0).abs() < 0.0005,
        "{what} moved stock"
    );

    let seq: i64 = sqlx::query_scalar("SELECT next_receipt_seq FROM devices WHERE device_id = ?")
        .bind(DEVICE)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(seq, 1, "{what} consumed a receipt number");
}

/// Stock on hand as a number, however the writer happened to spell it.
async fn stock_of(pool: &SqlitePool, product_id: &str) -> f64 {
    let raw: String =
        sqlx::query_scalar("SELECT quantity_on_hand FROM stock_levels WHERE product_id = ?")
            .bind(product_id)
            .fetch_one(pool)
            .await
            .unwrap();
    raw.parse()
        .unwrap_or_else(|_| panic!("unparseable quantity {raw:?}"))
}

/// Keeps the fixture module honest about what it exports.
#[allow(dead_code)]
const _: &str = BRANCH_FALLBACK;
