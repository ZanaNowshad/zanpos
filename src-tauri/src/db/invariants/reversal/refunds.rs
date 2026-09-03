#![cfg(test)]
//! Refunds.
//!
//! What went back to the customer, recorded as its own document rather than
//! as an edit to the sale. Includes what happens when the same refund is
//! submitted twice, and what a refused one must leave behind.
//!
//! Part of the reversal suite; the shared fixtures live in
//! [`super`](super).

use super::super::{migrated_pool, one_real_sale, CASHIER, DEVICE};
use super::{only_line, refund_item};
use crate::db::repositories::refund_repo;
use sqlx::SqlitePool;

// ── Duplicate protection ─────────────────────────────────────────────────────

/// Sending the same refund twice must not pay the customer twice.
///
/// `refunds.idempotency_key` is `UNIQUE`, which reads like retry protection, but
/// the key was built from a ULID minted inside `create_refund` — so it was
/// different on every call and the constraint could never fire. The per-item
/// ceiling catches a repeated *full* refund, because the second one would push
/// the line past its total; a repeated *partial* refund fits under the ceiling
/// and goes through. One tap of Refund on a flaky connection, retried, and the
/// customer is paid twice for one returned item.
#[tokio::test]
async fn the_same_refund_submitted_twice_pays_out_once() {
    let pool = migrated_pool().await;
    let (sale, _shift) = one_real_sale(&pool, "3", "rev-dup").await;
    let (item, name, total) = only_line(&pool, &sale).await;
    let one_unit = total / 3;

    // The refund screen mints one key per attempt and keeps it across retries.
    let attempt = Some("refund-attempt-1".to_string());

    let first = refund_repo::create_refund(
        &pool,
        &sale,
        vec![refund_item(&item, &name, "1", one_unit)],
        "returned one",
        "customer_return",
        CASHIER,
        false,
        attempt.clone(),
    )
    .await
    .expect("the first refund");

    // The same request again — the operator's finger, or a retry after a timeout.
    let replay = refund_repo::create_refund(
        &pool,
        &sale,
        vec![refund_item(&item, &name, "1", one_unit)],
        "returned one",
        "customer_return",
        CASHIER,
        false,
        attempt,
    )
    .await;

    let (count, paid): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(refund_total_minor), 0)
           FROM refunds WHERE original_sale_id = ?",
    )
    .bind(&sale)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        count, 1,
        "a repeated refund created a second reversal: {replay:?}"
    );
    assert_eq!(paid, one_unit, "the customer was paid twice for one return");
    if let Ok(second) = replay {
        assert_eq!(
            second.refund_receipt_number, first.refund_receipt_number,
            "a replay must hand back the original refund, not mint a new receipt"
        );
    }
}

// ── The reversal is a record, not an edit ────────────────────────────────────

/// A refund leaves the original sale exactly as it was.
///
/// The whole audit trail rests on this: the sale says what was collected, the
/// refund says what went back, and the balance is the difference. Adjusting
/// `sales.net_total_minor` down instead would balance just as well and destroy
/// the evidence that the money was ever taken.
#[tokio::test]
async fn a_refund_never_rewrites_the_sale_it_reverses() {
    let pool = migrated_pool().await;
    let (sale, _shift) = one_real_sale(&pool, "3", "rev-immutable").await;

    let before: (i64, i64, i64, String) = sqlx::query_as(
        "SELECT net_total_minor, tax_total_minor, gross_total_minor, receipt_number
           FROM sales WHERE sale_id = ?",
    )
    .bind(&sale)
    .fetch_one(&pool)
    .await
    .unwrap();
    let paid_before: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(amount_minor), 0) FROM payments WHERE sale_id = ?")
            .bind(&sale)
            .fetch_one(&pool)
            .await
            .unwrap();

    let (item, name, total) = only_line(&pool, &sale).await;
    let refund = refund_repo::create_refund(
        &pool,
        &sale,
        vec![refund_item(&item, &name, "1", total / 3)],
        "one back",
        "customer_return",
        CASHIER,
        false,
        None,
    )
    .await
    .expect("refund");

    let after: (i64, i64, i64, String) = sqlx::query_as(
        "SELECT net_total_minor, tax_total_minor, gross_total_minor, receipt_number
           FROM sales WHERE sale_id = ?",
    )
    .bind(&sale)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before, after, "the refund edited the sale it reversed");

    let paid_after: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(amount_minor), 0) FROM payments WHERE sale_id = ?")
            .bind(&sale)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        paid_before, paid_after,
        "the refund altered the payments on the original sale"
    );

    // original → reversal → balance, readable from the rows alone.
    let (reversed_sale, reversed_amount): (String, i64) = sqlx::query_as(
        "SELECT original_sale_id, refund_total_minor FROM refunds WHERE refund_id = ?",
    )
    .bind(&refund.refund_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        reversed_sale, sale,
        "the reversal must name what it reverses"
    );
    assert_eq!(
        paid_after - reversed_amount,
        after.0 - total / 3,
        "collected minus reversed is what the shop kept"
    );
}

/// A partial refund leaves the rest of the line refundable, and no more.
#[tokio::test]
async fn a_partial_refund_leaves_exactly_the_remainder_refundable() {
    let pool = migrated_pool().await;
    let (sale, _shift) = one_real_sale(&pool, "3", "rev-partial").await;
    let (item, name, total) = only_line(&pool, &sale).await;

    refund_repo::create_refund(
        &pool,
        &sale,
        vec![refund_item(&item, &name, "1", total / 3)],
        "one of three",
        "customer_return",
        CASHIER,
        false,
        None,
    )
    .await
    .expect("the partial refund");

    let refunded: i64 =
        sqlx::query_scalar("SELECT refunded_amount_minor FROM sale_items WHERE sale_item_id = ?")
            .bind(&item)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(refunded, total / 3, "the line records what came back");

    // The remainder is refundable...
    refund_repo::create_refund(
        &pool,
        &sale,
        vec![refund_item(&item, &name, "2", total - total / 3)],
        "the other two",
        "customer_return",
        CASHIER,
        false,
        None,
    )
    .await
    .expect("the rest of the line must still be refundable");

    // ...and nothing beyond it.
    let over = refund_repo::create_refund(
        &pool,
        &sale,
        vec![refund_item(&item, &name, "1", 1)],
        "one too many",
        "customer_return",
        CASHIER,
        false,
        None,
    )
    .await;
    assert!(
        over.is_err(),
        "the till refunded more than the customer ever paid"
    );

    let total_back: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(refund_total_minor), 0) FROM refunds WHERE original_sale_id = ?",
    )
    .bind(&sale)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        total_back, total,
        "the sum of the reversals must equal the line, never exceed it"
    );
}

/// A voided sale cannot also be refunded — that would reverse it twice.
#[tokio::test]
async fn a_voided_sale_cannot_be_refunded_as_well() {
    let pool = migrated_pool().await;
    let (sale, _shift) = one_real_sale(&pool, "2", "rev-void").await;
    let (item, name, total) = only_line(&pool, &sale).await;

    sqlx::query("UPDATE sales SET status = 'voided' WHERE sale_id = ?")
        .bind(&sale)
        .execute(&pool)
        .await
        .unwrap();

    let outcome = refund_repo::create_refund(
        &pool,
        &sale,
        vec![refund_item(&item, &name, "1", total / 2)],
        "already voided",
        "customer_return",
        CASHIER,
        false,
        None,
    )
    .await;
    assert!(
        outcome.is_err(),
        "a voided sale was refunded on top of the void — the money went back twice"
    );

    let refunds: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM refunds WHERE original_sale_id = ?")
            .bind(&sale)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(refunds, 0, "a refused refund must leave no reversal behind");
}

/// A refused refund does not consume a receipt number.
///
/// The counter is bumped inside the same transaction that writes the refund, so
/// a rollback has to take the bump with it. If it did not, every failed attempt
/// would punch a hole in the receipt book.
#[tokio::test]
async fn a_refused_refund_does_not_burn_a_receipt_number() {
    let pool = migrated_pool().await;
    let (sale, _shift) = one_real_sale(&pool, "2", "rev-noburn").await;
    let (item, name, total) = only_line(&pool, &sale).await;

    let before: i64 =
        sqlx::query_scalar("SELECT next_receipt_seq FROM devices WHERE device_id = ?")
            .bind(DEVICE)
            .fetch_one(&pool)
            .await
            .unwrap();

    // More than the line is worth.
    let _ = refund_repo::create_refund(
        &pool,
        &sale,
        vec![refund_item(&item, &name, "9", total * 9)],
        "too much",
        "customer_return",
        CASHIER,
        false,
        None,
    )
    .await;

    let after: i64 = sqlx::query_scalar("SELECT next_receipt_seq FROM devices WHERE device_id = ?")
        .bind(DEVICE)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        before, after,
        "a refused refund advanced the receipt counter, leaving a number nobody can account for"
    );
}

/// A refund survives a restart: it is committed, not held in memory.
#[tokio::test]
async fn a_reversal_is_still_there_after_the_application_restarts() {
    let pool = migrated_pool().await;
    let (sale, _shift) = one_real_sale(&pool, "2", "rev-restart").await;
    let (item, name, total) = only_line(&pool, &sale).await;

    let refund = refund_repo::create_refund(
        &pool,
        &sale,
        vec![refund_item(&item, &name, "1", total / 2)],
        "returned",
        "customer_return",
        CASHIER,
        false,
        None,
    )
    .await
    .expect("refund");

    // Everything the process was holding goes away.
    let url = (*pool.connect_options()).clone();
    pool.close().await;
    let reopened = SqlitePool::connect_with(url)
        .await
        .expect("reopen the database as a restarted app would");

    let (found, amount): (String, i64) = sqlx::query_as(
        "SELECT refund_receipt_number, refund_total_minor FROM refunds WHERE refund_id = ?",
    )
    .bind(&refund.refund_id)
    .fetch_one(&reopened)
    .await
    .expect("the reversal must outlive the process that made it");
    assert_eq!(found, refund.refund_receipt_number);
    assert_eq!(amount, total / 2);
}
