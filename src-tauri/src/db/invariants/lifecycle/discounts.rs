#![cfg(test)]
//! Money taken off, and who agreed to it.
//!
//! A discount is the one thing at a till that reduces the takings on
//! purpose, so what matters is not the arithmetic but the approval behind
//! it. These cover both: tax charged on the discounted amount, and a
//! discount refused when no manager approved it.
//!
//! Part of the till lifecycle suite; the shared fixtures live in
//! [`super`](super).

use super::super::{approve_discount, migrated_pool};
use super::{assert_pre_sale_state, cash, due, line, stock_of, till};
use crate::db::repositories::{held_cart_repo, sale_repo};

// ── Discounts ────────────────────────────────────────────────────────────────

/// A line discount reduces the taxable amount, not just the total.
///
/// Tax is charged on what the customer actually pays for the line. Computing it
/// on the undiscounted price would overcharge VAT on every discounted item — a
/// small error per line and a reportable one at the end of a quarter.
#[tokio::test]
async fn a_line_discount_is_taxed_after_the_discount() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    let mut discounted = line("prd_inv", "Cola 330ml", "2", 1000);
    discounted.line_discount_minor = 200; // 0.200 off a 2.000 line
                                          // The cart's own arithmetic, which the server recomputes from scratch.
    discounted.tax_amount_minor = 180;
    discounted.line_total_minor = 1980;
    let line_id = discounted.cart_line_id.clone();
    cart.lines.push(discounted);
    approve_discount(&pool, &cart.cart_id, &line_id, 200).await;

    let result = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(1980),
        "life-line-disc",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("checkout");

    let (tax, net, discount, gross): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT tax_total_minor, net_total_minor, discount_total_minor, gross_total_minor
           FROM sales WHERE sale_id = ?",
    )
    .bind(&result.sale_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(gross, 2000, "gross is the pre-discount value of the line");
    assert_eq!(discount, 200);
    assert_eq!(tax, 180, "10% of 1.800, not of 2.000");
    assert_eq!(net, 1980);
}

/// A whole-bill discount comes off after line tax, and the payment must match.
#[tokio::test]
async fn a_bill_discount_reduces_what_is_owed() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    cart.lines.push(line("prd_inv", "Cola 330ml", "2", 1000));
    cart.bill_discount_minor = 300;
    approve_discount(&pool, &cart.cart_id, "", 300).await;

    let total = due(&cart); // 2.200 with tax, less 0.300
    assert_eq!(total, 1900);

    let result = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(total),
        "life-bill-disc",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("checkout");

    let (net, discount): (i64, i64) =
        sqlx::query_as("SELECT net_total_minor, discount_total_minor FROM sales WHERE sale_id = ?")
            .bind(&result.sale_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(net, 1900);
    assert_eq!(discount, 300);
}

// ── Discount authorisation ───────────────────────────────────────────────────

/// A discount nobody approved does not reach the books.
///
/// `pos_apply_bill_discount` checks the manager's permission, enforces the
/// ceiling, demands a reason and writes an audit row — then hands the cart back
/// to the frontend, which hands it to finalize. If finalize trusts the number it
/// is given, every one of those checks is skippable by calling finalize
/// directly, and the till gives money away with no manager and no record of who
/// took it. This is the same reason `pos_price_overrides` exists for prices.
#[tokio::test]
async fn an_unapproved_bill_discount_is_refused() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    cart.lines.push(line("prd_inv", "Cola 330ml", "2", 1000));
    cart.bill_discount_minor = 300; // no approval recorded

    let err = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(1900),
        "life-bill-disc-unapproved",
        None,
        false,
        None,
        false,
    )
    .await
    .expect_err("an unapproved bill discount must not sell");

    assert!(
        err.to_string().contains("has not been approved"),
        "the cashier needs to be told to get it approved, got: {err}"
    );
    assert_pre_sale_state(&pool, "an unapproved bill discount").await;
}

/// The same for a single line, and the stock stays where it was.
#[tokio::test]
async fn an_unapproved_line_discount_is_refused() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    let before = stock_of(&pool, "prd_inv").await;
    let mut discounted = line("prd_inv", "Cola 330ml", "2", 1000);
    discounted.line_discount_minor = 200;
    discounted.tax_amount_minor = 180;
    discounted.line_total_minor = 1980;
    cart.lines.push(discounted);

    let err = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(1980),
        "life-line-disc-unapproved",
        None,
        false,
        None,
        false,
    )
    .await
    .expect_err("an unapproved line discount must not sell");

    assert!(
        err.to_string().contains("Cola 330ml"),
        "the message should name the line, got: {err}"
    );
    assert_pre_sale_state(&pool, "an unapproved line discount").await;
    assert_eq!(
        stock_of(&pool, "prd_inv").await,
        before,
        "a refused sale must not move stock"
    );
}

/// An approval for 0.100 does not authorise 5.000.
///
/// Recording only *that* a discount was approved would let the amount be edited
/// afterwards, which is the same hole one step further along. The approval is
/// for an amount, and finalize matches on the amount.
#[tokio::test]
async fn an_inflated_discount_is_refused() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    cart.lines.push(line("prd_inv", "Cola 330ml", "2", 1000));
    approve_discount(&pool, &cart.cart_id, "", 100).await;
    cart.bill_discount_minor = 500; // more than was approved

    let err = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(1700),
        "life-bill-disc-inflated",
        None,
        false,
        None,
        false,
    )
    .await
    .expect_err("a discount larger than the approved amount must not sell");

    assert!(
        err.to_string().contains("has not been approved"),
        "got: {err}"
    );
    assert_pre_sale_state(&pool, "an inflated bill discount").await;
}

/// The approval is spent with the sale, so it cannot authorise a second one.
///
/// Approvals are keyed by cart, and a cart id is generated per sale, so this is
/// belt and braces — but a leftover row is a standing permission to discount,
/// and those should not accumulate in a table nobody looks at.
#[tokio::test]
async fn an_approval_does_not_survive_the_sale() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    cart.lines.push(line("prd_inv", "Cola 330ml", "2", 1000));
    cart.bill_discount_minor = 300;
    approve_discount(&pool, &cart.cart_id, "", 300).await;

    sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(1900),
        "life-bill-disc-spent",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("checkout");

    let left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pos_discount_authorizations WHERE cart_id = ?")
            .bind(&cart.cart_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(left, 0, "the approval outlived the sale it was given for");
}

// ── Parking a basket and picking it up again ─────────────────────────────────

/// A basket parked with an approved discount still sells when it is resumed.
///
/// Resuming mints a new cart id, and the approval tables are keyed by cart id,
/// so the approval has to travel with it. Otherwise the discount survives on
/// screen — the cashier can see it on the resumed basket — and dies at the
/// Charge button, which is the worst possible moment to discover it.
#[tokio::test]
async fn a_parked_basket_keeps_its_approved_discount() {
    let pool = migrated_pool().await;
    let (shift, mut cart) = till(&pool).await;
    cart.lines.push(line("prd_inv", "Cola 330ml", "2", 1000));
    cart.bill_discount_minor = 300;
    approve_discount(&pool, &cart.cart_id, "", 300).await;

    let held = held_cart_repo::save_held_cart(&pool, &cart, Some("customer went back".into()))
        .await
        .expect("park the basket");
    let resumed = held_cart_repo::resume_held_cart(&pool, &held.held_cart_id, &shift)
        .await
        .expect("pick it back up");

    assert_ne!(
        resumed.cart_id, cart.cart_id,
        "a resumed basket is a new cart — this is what the approval has to survive"
    );
    assert_eq!(resumed.bill_discount_minor, 300);

    let result = sale_repo::finalize_sale(
        &pool,
        &resumed,
        cash(1900),
        "life-resumed-disc",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("a resumed basket with an approved discount must still sell");

    let (net, discount): (i64, i64) =
        sqlx::query_as("SELECT net_total_minor, discount_total_minor FROM sales WHERE sale_id = ?")
            .bind(&result.sale_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(net, 1900);
    assert_eq!(discount, 300);
}

/// Parking a basket does not by itself authorise the discount on it.
///
/// The hold/resume path must not become a way around the manager check: a cart
/// carrying a discount nobody approved is still refused after a round trip
/// through `held_carts`.
#[tokio::test]
async fn parking_a_basket_does_not_approve_its_discount() {
    let pool = migrated_pool().await;
    let (shift, mut cart) = till(&pool).await;
    cart.lines.push(line("prd_inv", "Cola 330ml", "2", 1000));
    cart.bill_discount_minor = 300; // never approved

    let held = held_cart_repo::save_held_cart(&pool, &cart, None)
        .await
        .expect("park the basket");
    let resumed = held_cart_repo::resume_held_cart(&pool, &held.held_cart_id, &shift)
        .await
        .expect("pick it back up");

    sale_repo::finalize_sale(
        &pool,
        &resumed,
        cash(1900),
        "life-resumed-unapproved",
        None,
        false,
        None,
        false,
    )
    .await
    .expect_err("a hold and resume must not launder an unapproved discount");

    assert_pre_sale_state(&pool, "an unapproved discount parked and resumed").await;
}
