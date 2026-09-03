#![cfg(test)]
//! What the customer handed over, and what the books say they handed over.
//!
//! A tender is the one part of a sale with a counterpart outside the database —
//! notes in a drawer, a card settlement file, a transfer that either arrived or
//! did not. So these check the arithmetic that has to hold between
//! `sales.net_total_minor`, `payments.amount_minor`, `tendered_minor` and
//! `change_minor`, for every method the till actually offers, and they check it
//! in the rows rather than in the return value.

use super::{migrated_pool, seed, CASHIER, DEVICE, TAX_VAT};
use crate::db::repositories::{sale_repo, shift_repo};
use crate::domain::cart::{Cart, CartLine};
use crate::domain::sale::PaymentInput;
use sqlx::SqlitePool;

/// A till with an open shift and one product on the shelf.
async fn till(pool: &SqlitePool) -> (String, Cart) {
    let branch = seed(pool).await;
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

fn cola(qty: &str) -> CartLine {
    CartLine::new(
        Some("prd_inv".into()),
        "Cola 330ml".into(),
        None,
        None,
        qty,
        1000,
        TAX_VAT.into(),
        1000,
        false,
    )
}

fn pay(method: &str, amount: i64, tendered: Option<i64>) -> PaymentInput {
    PaymentInput {
        method: method.into(),
        amount_minor: amount,
        tendered_minor: tendered,
        external_reference: None,
    }
}

/// What the till owes for this basket.
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

/// Every tender the application can put on a sale must be storable.
///
/// `payments.payment_method` is constrained by a CHECK, and the application has
/// its own idea of what a tender can be — `PaymentInput.method` in `types.ts`.
/// The two drifted: `exchange_credit` was added to the frontend, produced by
/// `posExchange.ts` whenever a returned item's credit goes towards a
/// replacement, and read back by name in `shift_repo` and `report_commands` —
/// while the CHECK still listed four methods. So an exchange was a constraint
/// violation at the moment the cashier pressed Charge, after `create_refund` had
/// already committed the credit. Migration 0063 widened it; this is what keeps
/// the two lists together.
#[tokio::test]
async fn every_method_the_application_can_produce_is_storable() {
    let pool = migrated_pool().await;

    // The set `types.ts` declares for `PaymentInput.method`, which is what the
    // frontend is free to send over IPC.
    let (_shift, base) = till(&pool).await;
    for method in ["cash", "card", "wallet", "other", "exchange_credit"] {
        let mut cart = Cart::new(
            base.branch_id.clone(),
            base.device_id.clone(),
            base.shift_id.clone(),
            base.cashier_user_id.clone(),
        );
        cart.lines.push(cola("1"));
        let total = due(&cart);

        let outcome = sale_repo::finalize_sale(
            &pool,
            &cart,
            vec![pay(method, total, Some(total))],
            &format!("tender-{method}"),
            None,
            false,
            None,
            false,
        )
        .await;

        assert!(
            outcome.is_ok(),
            "'{method}' is a tender the app can build but the database refuses: {:?}",
            outcome.err()
        );
    }
}

/// Cash tendered over the total records the change, not a larger payment.
///
/// `amount_minor` is what settles the sale and `tendered_minor` is what crossed
/// the counter. Folding the overpayment into `amount_minor` would make the
/// payments exceed the sale and every reconciliation report disagree with the
/// drawer by the change given.
#[tokio::test]
async fn change_lives_in_tendered_not_in_the_amount() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    cart.lines.push(cola("1"));
    let total = due(&cart);

    let result = sale_repo::finalize_sale(
        &pool,
        &cart,
        vec![pay("cash", total, Some(total + 900))],
        "tender-change",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("checkout");

    let (amount, tendered, change): (i64, Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT amount_minor, tendered_minor, change_minor FROM payments WHERE sale_id = ?",
    )
    .bind(&result.sale_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(amount, total, "the payment settles the sale, no more");
    assert_eq!(tendered, Some(total + 900));
    assert_eq!(change, Some(900), "the change is recorded, not discarded");
    assert_eq!(
        tendered.unwrap() - change.unwrap(),
        amount,
        "tendered minus change must be what the sale collected"
    );
}

/// A card tender records no change, because a card cannot overpay.
#[tokio::test]
async fn a_card_tender_carries_no_change() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    cart.lines.push(cola("1"));
    let total = due(&cart);

    let result = sale_repo::finalize_sale(
        &pool,
        &cart,
        vec![pay("card", total, None)],
        "tender-card",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("checkout");

    let (amount, change): (i64, Option<i64>) =
        sqlx::query_as("SELECT amount_minor, change_minor FROM payments WHERE sale_id = ?")
            .bind(&result.sale_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(amount, total);
    assert!(
        change.unwrap_or(0) == 0,
        "a card tender that reports change means the drawer is expected to open"
    );
}

/// Across a split, the parts settle the sale exactly and only cash gives change.
#[tokio::test]
async fn a_split_settles_exactly_and_only_the_cash_part_gives_change() {
    let pool = migrated_pool().await;
    let (_shift, mut cart) = till(&pool).await;
    cart.lines.push(cola("3"));
    let total = due(&cart);
    let on_card = 1_000;

    let result = sale_repo::finalize_sale(
        &pool,
        &cart,
        vec![
            pay("card", on_card, None),
            pay("cash", total - on_card, Some(total - on_card + 500)),
        ],
        "tender-split",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("checkout");

    let rows: Vec<(String, i64, Option<i64>)> = sqlx::query_as(
        "SELECT payment_method, amount_minor, change_minor FROM payments
          WHERE sale_id = ? ORDER BY payment_method",
    )
    .bind(&result.sale_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), 2, "both tenders must be recorded separately");
    let collected: i64 = rows.iter().map(|(_, a, _)| a).sum();
    assert_eq!(collected, total, "the parts must settle the sale exactly");

    let card = rows
        .iter()
        .find(|(m, _, _)| m == "card")
        .expect("card part");
    assert_eq!(card.2.unwrap_or(0), 0, "no change against a card");
    let cash = rows
        .iter()
        .find(|(m, _, _)| m == "cash")
        .expect("cash part");
    assert_eq!(cash.2, Some(500), "the change came out of the cash tender");
}
