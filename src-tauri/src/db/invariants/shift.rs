#![cfg(test)]
//! The controls that make a cashier accountable for a drawer.
//!
//! A shift is the unit a cash count is measured against: it opens with a float,
//! collects takings and cash movements, and closes against a physical count. The
//! variance between what the till should hold and what it does hold is the only
//! signal a shop has that money has gone missing — so what matters is that the
//! variance is computed, recorded, and cannot be revised afterwards.

use super::{migrated_pool, seed, CASHIER, DEVICE};
use crate::db::repositories::shift_repo;
use sqlx::SqlitePool;

async fn open(pool: &SqlitePool, float: i64) -> (String, String) {
    let branch = seed(pool).await;
    let shift = shift_repo::open_shift(pool, &branch, DEVICE, CASHIER, float)
        .await
        .expect("open shift")
        .shift_id;
    (branch, shift)
}

/// A device can only have one shift open at a time, and the database says so.
///
/// An application-level check has a race window between the read and the insert;
/// a partial unique index does not. `idx_shifts_one_open_per_device` is what
/// actually enforces this, and the repository turns its violation into a
/// readable message rather than a raw constraint error.
#[tokio::test]
async fn a_device_cannot_have_two_shifts_open_at_once() {
    let pool = migrated_pool().await;
    let (branch, _first) = open(&pool, 10_000).await;

    let second = shift_repo::open_shift(&pool, &branch, DEVICE, CASHIER, 5_000).await;
    assert!(
        second.is_err(),
        "two shifts open on one till means two cashiers accountable for one drawer"
    );

    let open_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM shifts WHERE device_id = ? AND status = 'open'")
            .bind(DEVICE)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(open_count, 1);
}

/// Closing a shift records the variance, and closing it again cannot revise it.
///
/// A second close used to be possible through the assistant, which had its own
/// UPDATE with no `status = 'open'` guard: it overwrote `counted_cash_minor`
/// long after the fact, and computed no expected figure or variance at all. A
/// cash count that can be edited after the drawer has been counted is not a
/// control.
#[tokio::test]
async fn a_closed_shift_cannot_have_its_count_revised() {
    let pool = migrated_pool().await;
    let (_branch, shift) = open(&pool, 10_000).await;

    // The drawer is counted short by 0.500.
    shift_repo::close_shift(&pool, &shift, Some(9_500), Some("counted short".into()))
        .await
        .expect("close");

    let before: (Option<i64>, Option<i64>, Option<i64>, String) = sqlx::query_as(
        "SELECT counted_cash_minor, expected_cash_minor, cash_difference_minor, status
           FROM shifts WHERE shift_id = ?",
    )
    .bind(&shift)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before.0, Some(9_500), "the count is recorded");
    assert_eq!(
        before.1,
        Some(10_000),
        "the float is what the till should hold"
    );
    assert_eq!(
        before.2,
        Some(-500),
        "and the shortfall is recorded as a variance"
    );
    assert_eq!(before.3, "closed");

    // Closing again — the shape of "revising the count" — must change nothing.
    let again =
        shift_repo::close_shift(&pool, &shift, Some(10_000), Some("actually fine".into())).await;
    let after: (Option<i64>, Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT counted_cash_minor, expected_cash_minor, cash_difference_minor
           FROM shifts WHERE shift_id = ?",
    )
    .bind(&shift)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (after.0, after.1, after.2),
        (before.0, before.1, before.2),
        "a second close rewrote the cash count, erasing a 0.500 shortfall: {again:?}"
    );
}

/// Every cash movement is attributed and none of them can be edited away.
///
/// `cash_events` is append-only: nothing in the application updates or deletes a
/// row. A paid-out that could be edited or removed would let cash leave the
/// drawer and the reconciliation still balance.
#[tokio::test]
async fn cash_movements_are_attributed_and_append_only() {
    let pool = migrated_pool().await;
    let (branch, shift) = open(&pool, 10_000).await;

    sqlx::query(
        "INSERT INTO cash_events
           (cash_event_id, shift_id, branch_id, device_id, origin_device_id, event_type,
            amount_minor, note, created_by_user_id, created_at, updated_at)
         VALUES ('ce_1', ?, ?, ?, ?, 'paid_out', 2500, 'supplier delivery',
                 ?, datetime('now'), datetime('now'))",
    )
    .bind(&shift)
    .bind(&branch)
    .bind(DEVICE)
    .bind(DEVICE)
    .bind(CASHIER)
    .execute(&pool)
    .await
    .expect("cash event");

    let (who, terminal, when, tied): (String, String, String, String) = sqlx::query_as(
        "SELECT created_by_user_id, device_id, created_at, shift_id
           FROM cash_events WHERE cash_event_id = 'ce_1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(who, CASHIER, "a cash movement with no author");
    assert_eq!(terminal, DEVICE, "a cash movement with no terminal");
    assert!(!when.is_empty(), "a cash movement with no time");
    assert_eq!(tied, shift, "a cash movement not tied to a shift");

    // And it lands in the drawer arithmetic rather than being cosmetic.
    let drawer = crate::commands::cash_commands::drawer_summary_inner(&pool, &shift)
        .await
        .expect("drawer summary");
    assert_eq!(drawer.paid_out_minor, 2500);
    assert_eq!(
        drawer.expected_minor, 7_500,
        "money paid out of the drawer must reduce what the till should hold"
    );
}

/// A sale cannot be rung into a shift that has been closed.
#[tokio::test]
async fn a_closed_shift_takes_no_more_sales() {
    let pool = migrated_pool().await;
    let (branch, shift) = open(&pool, 10_000).await;
    shift_repo::close_shift(&pool, &shift, Some(10_000), None)
        .await
        .expect("close");

    let mut cart =
        crate::domain::cart::Cart::new(branch, DEVICE.into(), shift.clone(), CASHIER.into());
    cart.lines.push(crate::domain::cart::CartLine::new(
        Some("prd_inv".into()),
        "Cola 330ml".into(),
        None,
        None,
        "1",
        1000,
        super::TAX_VAT.into(),
        1000,
        false,
    ));
    let due: i64 = cart.lines.iter().map(|l| l.line_total_minor).sum();

    crate::db::repositories::sale_repo::finalize_sale(
        &pool,
        &cart,
        vec![crate::domain::sale::PaymentInput {
            method: "cash".into(),
            amount_minor: due,
            tendered_minor: Some(due),
            external_reference: None,
        }],
        "shift-closed-sale",
        None,
        false,
        None,
        false,
    )
    .await
    .expect_err("a closed shift must not take a sale — the drawer is already counted");
}
