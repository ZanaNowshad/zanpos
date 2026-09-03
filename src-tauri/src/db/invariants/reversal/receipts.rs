#![cfg(test)]
//! Receipt identity.
//!
//! A receipt number is how a shop finds a transaction again, so it has to
//! mean exactly one thing. These cover the number the till issues, the
//! sequence it comes from, and reprinting — which must be a lookup and
//! never a re-issue.
//!
//! Part of the reversal suite; the shared fixtures live in
//! [`super`](super).

use super::super::{migrated_pool, one_real_sale, seed, CASHIER, DEVICE, TAX_VAT};
use super::{only_line, refund_item};
use crate::db::repositories::{refund_repo, sale_repo, shift_repo};
use crate::domain::cart::{Cart, CartLine};
use crate::domain::sale::PaymentInput;
use sqlx::SqlitePool;

// ── Receipt identity ─────────────────────────────────────────────────────────

/// Sales and refunds share one counter, so they must read it the same way.
///
/// `devices.next_receipt_seq` is deliberately one sequence for both — the
/// comment in `create_refund` says so — and both format the number as
/// `{branch}-{device}-{seq:08}`. But the two readers disagreed about what the
/// column means: the sale takes `next_receipt_seq - 1`, the value the column
/// held before the bump, while the refund took the post-increment value. Take
/// one of each and the till issues the same number twice, once on a sale and
/// once on a refund. Neither `UNIQUE` catches it, because they are different
/// tables, and `get_sale_by_receipt` then has two rows to choose between.
#[tokio::test]
async fn a_sale_and_a_refund_never_carry_the_same_receipt_number() {
    let pool = migrated_pool().await;
    let (first_sale, _shift) = one_real_sale(&pool, "3", "rev-seq-1").await;

    // A refund between two sales, which is where the two readings meet.
    let (item, name, total) = only_line(&pool, &first_sale).await;
    refund_repo::create_refund(
        &pool,
        &first_sale,
        vec![refund_item(&item, &name, "1", total / 3)],
        "customer changed their mind",
        "customer_return",
        CASHIER,
        false,
        None,
    )
    .await
    .expect("refund");

    // Two more sales on the same till.
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM sales WHERE sale_id = ?")
        .bind(&first_sale)
        .fetch_one(&pool)
        .await
        .unwrap();
    let shift: String = sqlx::query_scalar("SELECT shift_id FROM sales WHERE sale_id = ?")
        .bind(&first_sale)
        .fetch_one(&pool)
        .await
        .unwrap();
    for n in 0..2 {
        let mut cart = Cart::new(branch.clone(), DEVICE.into(), shift.clone(), CASHIER.into());
        cart.lines.push(CartLine::new(
            Some("prd_inv".into()),
            "Cola 330ml".into(),
            None,
            None,
            "1",
            1000,
            TAX_VAT.into(),
            1000,
            false,
        ));
        let due: i64 = cart.lines.iter().map(|l| l.line_total_minor).sum();
        sale_repo::finalize_sale(
            &pool,
            &cart,
            vec![PaymentInput {
                method: "cash".into(),
                amount_minor: due,
                tendered_minor: Some(due),
                external_reference: None,
            }],
            &format!("rev-seq-{}", n + 2),
            None,
            false,
            None,
            false,
        )
        .await
        .expect("checkout");
    }

    let issued: Vec<String> = sqlx::query_scalar(
        "SELECT receipt_number FROM sales
         UNION ALL
         SELECT refund_receipt_number FROM refunds",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let mut unique = issued.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        issued.len(),
        "the till issued the same receipt number twice: {issued:?}"
    );
}

/// The shared sequence advances by one per document, with no gaps.
///
/// A gap is not a data-integrity failure but it is an audit question nobody can
/// answer — the shop cannot show that the missing number was never issued.
#[tokio::test]
async fn the_receipt_sequence_has_no_gaps_across_sales_and_refunds() {
    let pool = migrated_pool().await;
    let (sale, _shift) = one_real_sale(&pool, "3", "rev-gap-1").await;
    let (item, name, total) = only_line(&pool, &sale).await;
    refund_repo::create_refund(
        &pool,
        &sale,
        vec![refund_item(&item, &name, "1", total / 3)],
        "return",
        "customer_return",
        CASHIER,
        false,
        None,
    )
    .await
    .expect("refund");

    let mut seqs: Vec<i64> = sqlx::query_scalar::<_, String>(
        "SELECT receipt_number FROM sales
         UNION ALL
         SELECT refund_receipt_number FROM refunds",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .map(|r| {
        r.rsplit('-')
            .next()
            .and_then(|n| n.parse::<i64>().ok())
            .unwrap_or_else(|| panic!("unparseable receipt number {r:?}"))
    })
    .collect();
    seqs.sort();

    assert_eq!(
        seqs,
        (1..=seqs.len() as i64).collect::<Vec<_>>(),
        "the receipt book must run 1, 2, 3 with nothing skipped"
    );
}

/// Reading a receipt back is a lookup, never a re-issue.
///
/// Reprinting is the one operation a cashier reaches for when the printer has
/// jammed, so it has to be a pure read: same receipt number, same totals, no new
/// row, no second payment.
#[tokio::test]
async fn looking_a_receipt_up_twice_issues_nothing() {
    let pool = migrated_pool().await;
    let (sale, _shift) = one_real_sale(&pool, "2", "rev-reprint").await;
    let receipt: String = sqlx::query_scalar("SELECT receipt_number FROM sales WHERE sale_id = ?")
        .bind(&sale)
        .fetch_one(&pool)
        .await
        .unwrap();

    let counters = |pool: SqlitePool| async move {
        let sales: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sales")
            .fetch_one(&pool)
            .await
            .unwrap();
        let payments: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payments")
            .fetch_one(&pool)
            .await
            .unwrap();
        let seq: i64 =
            sqlx::query_scalar("SELECT next_receipt_seq FROM devices WHERE device_id = ?")
                .bind(DEVICE)
                .fetch_one(&pool)
                .await
                .unwrap();
        (sales, payments, seq)
    };

    let before = counters(pool.clone()).await;
    for _ in 0..3 {
        let again = refund_repo::get_sale_result_by_receipt(&pool, &receipt)
            .await
            .expect("a reprint must find the sale it already made");
        assert_eq!(
            again.receipt_number, receipt,
            "a reprint re-issued a number"
        );
        assert_eq!(again.sale_id, sale);
    }
    assert_eq!(
        counters(pool.clone()).await,
        before,
        "reprinting wrote to the books"
    );
}

/// A sale the printer never printed is still a sale, and still only one.
///
/// Printing happens after the transaction commits, so a printer that is off,
/// jammed or unplugged cannot roll the sale back — the customer has paid. What
/// must not happen is the operator retrying the *sale* to get paper: the
/// idempotency key makes that replay return the original rather than charge
/// again.
#[tokio::test]
async fn a_printer_failure_cannot_turn_one_sale_into_two() {
    let pool = migrated_pool().await;
    let branch = seed(&pool).await;
    let shift = shift_repo::open_shift(&pool, &branch, DEVICE, CASHIER, 10_000)
        .await
        .expect("open shift");
    let mut cart = Cart::new(branch, DEVICE.into(), shift.shift_id, CASHIER.into());
    cart.lines.push(CartLine::new(
        Some("prd_inv".into()),
        "Cola 330ml".into(),
        None,
        None,
        "2",
        1000,
        TAX_VAT.into(),
        1000,
        false,
    ));
    let due: i64 = cart.lines.iter().map(|l| l.line_total_minor).sum();
    let pay = || {
        vec![PaymentInput {
            method: "cash".into(),
            amount_minor: due,
            tendered_minor: Some(due),
            external_reference: None,
        }]
    };

    let printed =
        sale_repo::finalize_sale(&pool, &cart, pay(), "rev-printer", None, false, None, false)
            .await
            .expect("the sale completes whether or not paper comes out");

    // The paper jams; the operator presses Charge again to get a receipt.
    let retried =
        sale_repo::finalize_sale(&pool, &cart, pay(), "rev-printer", None, false, None, false)
            .await
            .expect("the retry must return the sale already made");

    assert_eq!(retried.sale_id, printed.sale_id);
    assert_eq!(
        retried.receipt_number, printed.receipt_number,
        "the retry issued a second receipt number for one sale"
    );

    let (sales, payments, collected): (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM sales),
                (SELECT COUNT(*) FROM payments),
                (SELECT COALESCE(SUM(amount_minor), 0) FROM payments)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(sales, 1, "the printer failure created a second sale");
    assert_eq!(payments, 1, "the customer was charged twice");
    assert_eq!(collected, due);
}

/// Every reprint is recorded, and none of them issues anything.
///
/// A reprint is a read — it must be, or a jammed printer could not be recovered
/// from. But it is not nothing: duplicate receipts are how a returned item gets
/// refunded twice, and the till could not answer "who printed this, and how many
/// times". The entry names the actor and the receipt without touching the sale.
#[tokio::test]
async fn every_reprint_is_recorded_without_issuing_anything() {
    let pool = migrated_pool().await;
    let (sale, _shift) = one_real_sale(&pool, "2", "rev-reprint-audit").await;
    let receipt: String = sqlx::query_scalar("SELECT receipt_number FROM sales WHERE sale_id = ?")
        .bind(&sale)
        .fetch_one(&pool)
        .await
        .unwrap();

    let before: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM sales),
                (SELECT COUNT(*) FROM payments),
                (SELECT next_receipt_seq FROM devices WHERE device_id = ?)",
    )
    .bind(DEVICE)
    .fetch_one(&pool)
    .await
    .unwrap();

    for _ in 0..3 {
        crate::db::repositories::audit_hash::insert_audit_entry_override(
            &pool,
            "sale.receipt_reprinted",
            "sale",
            &sale,
            CASHIER,
            "user",
            DEVICE,
            &sqlx::query_scalar::<_, String>("SELECT branch_id FROM sales WHERE sale_id = ?")
                .bind(&sale)
                .fetch_one(&pool)
                .await
                .unwrap(),
            None,
            Some(&format!("{{\"receipt_number\":\"{receipt}\"}}")),
            None,
            false,
        )
        .await
        .expect("record the reprint");
    }

    let (reprints, who): (i64, Option<String>) = sqlx::query_as(
        "SELECT COUNT(*), MAX(actor_user_id) FROM audit_logs
          WHERE entity_id = ? AND event_type = 'sale.receipt_reprinted'",
    )
    .bind(&sale)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(reprints, 3, "each reprint must leave its own entry");
    assert_eq!(
        who.as_deref(),
        Some(CASHIER),
        "the entry must name who printed"
    );

    let after: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM sales),
                (SELECT COUNT(*) FROM payments),
                (SELECT next_receipt_seq FROM devices WHERE device_id = ?)",
    )
    .bind(DEVICE)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        before, after,
        "a reprint issued a sale, a payment or a receipt number"
    );
}
