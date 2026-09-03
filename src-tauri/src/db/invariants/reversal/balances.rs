#![cfg(test)]
//! Money still owed.
//!
//! A delivery is handed over before it is paid for, so the outstanding
//! balance is a number the shop chases. It has to be the number the sale
//! actually collected.
//!
//! Part of the reversal suite; the shared fixtures live in
//! [`super`](super).

use super::super::{migrated_pool, seed, CASHIER, DEVICE, TAX_VAT};
use crate::db::repositories::{sale_repo, shift_repo};
use crate::domain::cart::{Cart, CartLine};
use crate::domain::sale::PaymentInput;

// ── Money owed after the customer has gone ───────────────────────────────────

/// What a delivery is owed for is what the sale collected, to the fils.
///
/// `delivery_orders.amount_minor` is the outstanding balance every report reads.
/// It is written inside the sale's own transaction from the server-recomputed
/// net, which is what keeps the two from drifting; a delivery whose amount came
/// from the cart instead could show an outstanding figure the sale never
/// supported.
#[tokio::test]
async fn what_a_delivery_owes_is_what_the_sale_collected() {
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
        "3",
        1000,
        TAX_VAT.into(),
        1000,
        false,
    ));
    let owed: i64 = cart.lines.iter().map(|l| l.line_total_minor).sum();

    let result = sale_repo::finalize_sale(
        &pool,
        &cart,
        vec![PaymentInput {
            method: "cash".into(),
            amount_minor: owed,
            tendered_minor: Some(owed),
            external_reference: None,
        }],
        "rev-delivery",
        None,
        false,
        Some(crate::domain::delivery::DeliveryInput {
            customer_id: None,
            customer_name: Some("A Customer".into()),
            contact_number: "+97339000000".into(),
            house_number: Some("12".into()),
            area: None,
            address_text: "A Street".into(),
            delivery_note: None,
            delivery_staff_name: None,
            rider_id: None,
            expected_payment_method: "cash".into(),
        }),
        false,
    )
    .await
    .expect("a delivery sale");

    let (amount, status, net): (i64, String, i64) = sqlx::query_as(
        "SELECT d.amount_minor, d.payment_status, s.net_total_minor
           FROM delivery_orders d JOIN sales s ON s.sale_id = d.sale_id
          WHERE d.sale_id = ?",
    )
    .bind(&result.sale_id)
    .fetch_one(&pool)
    .await
    .expect("the delivery was created with the sale");

    assert_eq!(
        amount, net,
        "the outstanding balance disagrees with the sale it belongs to"
    );
    assert_eq!(amount, owed);
    assert_eq!(status, "unpaid", "a delivery starts owed, not settled");
}

/// Settling a delivery records who actually decided it.
///
/// The confirmation can come from the automatic screenshot check or from a
/// manager overriding it in the notification panel, and the audit trail has to
/// tell them apart — "who accepted this payment" is the question asked when one
/// turns out to be wrong. The actor was hardcoded to the automatic verifier, so
/// a manager's decision was filed under its name.
#[tokio::test]
async fn settling_a_delivery_names_whoever_actually_settled_it() {
    let pool = migrated_pool().await;
    let branch = seed(&pool).await;
    let shift = shift_repo::open_shift(&pool, &branch, DEVICE, CASHIER, 10_000)
        .await
        .expect("open shift")
        .shift_id;
    let mut cart = Cart::new(branch, DEVICE.into(), shift, CASHIER.into());
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
    let owed: i64 = cart.lines.iter().map(|l| l.line_total_minor).sum();
    let sale = sale_repo::finalize_sale(
        &pool,
        &cart,
        vec![PaymentInput {
            method: "cash".into(),
            amount_minor: owed,
            tendered_minor: Some(owed),
            external_reference: None,
        }],
        "bal-attrib",
        None,
        false,
        Some(crate::domain::delivery::DeliveryInput {
            customer_id: None,
            customer_name: Some("A Customer".into()),
            contact_number: "+97339000000".into(),
            house_number: Some("12".into()),
            area: None,
            address_text: "A Street".into(),
            delivery_note: None,
            delivery_staff_name: None,
            rider_id: None,
            expected_payment_method: "cash".into(),
        }),
        false,
    )
    .await
    .expect("a delivery sale");

    let delivery: String =
        sqlx::query_scalar("SELECT delivery_id FROM delivery_orders WHERE sale_id = ?")
            .bind(&sale.sale_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    crate::db::repositories::delivery_repo::confirm_payment(
        &pool,
        &crate::domain::delivery::ConfirmPaymentInput {
            delivery_id: delivery.clone(),
            confirmed_by_user_id: CASHIER.into(),
            payment_reference: Some("counted at the counter".into()),
            payment_note: None,
        },
    )
    .await
    .expect("settle");

    let (status, by): (String, Option<String>) = sqlx::query_as(
        "SELECT payment_status, paid_confirmed_by_user_id FROM delivery_orders WHERE delivery_id = ?",
    )
    .bind(&delivery)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "paid");
    assert_eq!(
        by.as_deref(),
        Some(CASHIER),
        "the settlement was filed under someone who did not make it"
    );

    // Settling again changes nothing — the rider does not get paid twice.
    let before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE entity_id = ? AND event_type LIKE 'delivery%'",
    )
    .bind(&delivery)
    .fetch_one(&pool)
    .await
    .unwrap();
    let _ = crate::db::repositories::delivery_repo::confirm_payment(
        &pool,
        &crate::domain::delivery::ConfirmPaymentInput {
            delivery_id: delivery.clone(),
            confirmed_by_user_id: "someone-else".into(),
            payment_reference: None,
            payment_note: None,
        },
    )
    .await;
    let (still, after): (Option<String>, i64) = sqlx::query_as(
        "SELECT (SELECT paid_confirmed_by_user_id FROM delivery_orders WHERE delivery_id = ?),
                (SELECT COUNT(*) FROM audit_logs WHERE entity_id = ? AND event_type LIKE 'delivery%')",
    )
    .bind(&delivery)
    .bind(&delivery)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        still.as_deref(),
        Some(CASHIER),
        "a second settlement overwrote who settled it the first time"
    );
    assert_eq!(after, before, "a settled delivery was settled again");
}
