#![cfg(test)]
//! When the ordinary path is interrupted.
//!
//! A cashier presses Charge twice, the shift was closed from the back office,
//! the shelf does not hold what the basket claims, or the transaction fails at
//! its last statement. In every case the question is the same: what is left in
//! the tables afterwards. A half-written sale is worse than no sale, because it
//! is not discovered until cash-up.
//!
//! Part of the till lifecycle suite; the shared fixtures live in
//! [`super`](super).

use super::super::{migrated_pool, DEVICE};
use super::{assert_pre_sale_state, cash, due, line, stock_of, till};
use crate::db::repositories::sale_repo;
use crate::domain::sale::PaymentInput;

// ── The double press ─────────────────────────────────────────────────────────

/// Charge pressed twice sends the same key twice, and gets the same sale back.
///
/// The second press must not create a second sale, must not consume a second
/// receipt number, must not deduct stock again — and must not report failure,
/// because a cashier told a completed sale failed will ring it up again.
#[tokio::test]
async fn pressing_charge_twice_returns_the_first_sale_and_changes_nothing() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    cart.lines.push(line("prd_inv", "Cola 330ml", "2", 1000));
    let total = due(&cart);

    let first = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(total),
        "life-double",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("first press");

    let second = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(total),
        "life-double",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("the second press must return the first sale, not an error");

    assert_eq!(second.sale_id, first.sale_id, "a second sale was created");
    assert_eq!(
        second.receipt_number, first.receipt_number,
        "a second receipt number was issued"
    );
    assert_eq!(second.net_total_minor, first.net_total_minor);

    let sales: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sales")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sales, 1, "the double press produced {sales} sales");

    let payments: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(payments, 1, "the customer was charged twice");

    assert!(
        (stock_of(&pool, "prd_inv").await - 98.0).abs() < 0.0005,
        "stock was deducted twice"
    );

    // The receipt counter moved once, not twice.
    let seq: i64 = sqlx::query_scalar("SELECT next_receipt_seq FROM devices WHERE device_id = ?")
        .bind(DEVICE)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(seq, 2, "the receipt sequence advanced twice for one sale");
}

/// A sale into a closed shift is refused outright.
#[tokio::test]
async fn a_sale_into_a_closed_shift_is_refused() {
    let pool = migrated_pool().await;
    let (shift_id, mut cart) = till(&pool).await;
    cart.lines.push(line("prd_inv", "Cola 330ml", "1", 1000));

    sqlx::query("UPDATE shifts SET status='closed' WHERE shift_id = ?")
        .bind(&shift_id)
        .execute(&pool)
        .await
        .unwrap();

    let outcome = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(1100),
        "life-closed",
        None,
        false,
        None,
        false,
    )
    .await;
    assert!(outcome.is_err(), "a closed shift accepted a sale");
    assert_pre_sale_state(&pool, "a sale into a closed shift").await;
}

/// Selling more than there is, with the negative-stock flag off, is refused —
/// and refused as a whole, not after the sale row has been written.
#[tokio::test]
async fn overselling_is_refused_without_writing_a_partial_sale() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    cart.lines.push(line("prd_inv", "Cola 330ml", "500", 1000));
    let total = due(&cart);

    let outcome = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(total),
        "life-oversell",
        None,
        false,
        None,
        false,
    )
    .await;
    assert!(
        outcome.is_err(),
        "500 units were sold from a shelf holding 100"
    );
    assert_pre_sale_state(&pool, "an oversold basket").await;
}

/// A payment that is not positive is refused after the sale row has been
/// inserted inside the transaction — so this proves the rollback, not the guard.
#[tokio::test]
async fn a_failure_partway_through_the_transaction_rolls_the_whole_sale_back() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    cart.lines.push(line("prd_inv", "Cola 330ml", "2", 1000));
    let total = due(&cart);

    // Two tenders summing to the correct total, but one of them is zero. The
    // sum check passes, so the sale and its lines are written; the payment loop
    // then refuses the zero and the transaction unwinds.
    let outcome = sale_repo::finalize_sale(
        &pool,
        &cart,
        vec![
            PaymentInput {
                method: "cash".into(),
                amount_minor: total,
                tendered_minor: Some(total),
                external_reference: None,
            },
            PaymentInput {
                method: "card".into(),
                amount_minor: 0,
                tendered_minor: None,
                external_reference: None,
            },
        ],
        "life-rollback",
        None,
        false,
        None,
        false,
    )
    .await;
    assert!(outcome.is_err(), "a zero-value tender was accepted");

    // Nothing survives — not the sale that was already inserted, not its lines,
    // not the first payment, and not the receipt number.
    assert_pre_sale_state(&pool, "a sale that failed mid-transaction").await;
}

/// And the till still works afterwards: the failed attempt did not poison the
/// receipt sequence or leave the shift unusable.
#[tokio::test]
async fn the_till_still_sells_after_a_failed_attempt() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    cart.lines.push(line("prd_inv", "Cola 330ml", "2", 1000));
    let total = due(&cart);

    let _ = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(total - 1),
        "life-recover-fail",
        None,
        false,
        None,
        false,
    )
    .await;

    let good = sale_repo::finalize_sale(
        &pool,
        &cart,
        cash(total),
        "life-recover-ok",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("the next sale must go through");

    assert!(
        good.receipt_number.ends_with("00000001"),
        "the failed attempt consumed a receipt number: got {}",
        good.receipt_number
    );
    let sales: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sales")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sales, 1);
}
