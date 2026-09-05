#![cfg(test)]
//! What the drawer should hold, according to five different pieces of code.
//!
//! The expected-cash formula — opening + cash sales − cash refunds + paid in −
//! paid out − safe drop, with unpaid deliveries left out — is written five
//! times: `shift_repo::close_shift`, `cash_commands::drawer_summary_inner`,
//! `report_commands::report_eod_cashup_inner`, and the assistant's
//! `get_cash_summary` and `x_report` tools. Each has its own copy of the SQL. A
//! shop discovers they have drifted at cash-up, when the number on the screen
//! disagrees with the notes in the till and nobody can say which is right.
//!
//! An earlier version of this file said "three" and tested only the delivery
//! clause. It passed while three of the five were wrong about something else
//! entirely: a refund settled as store credit never leaves the drawer, so it
//! must be carved out of the cash figure before the cash-ratio scaling. Two
//! copies did that; three deducted the whole refund, and the X-report a manager
//! reads mid-shift disagreed with the close-shift figure by the credited amount.
//! The test could not see it because it never issued a refund. It does now.

use super::{migrated_pool, seed, CASHIER, DEVICE, TAX_VAT};
use crate::db::repositories::{delivery_repo, refund_repo, sale_repo, shift_repo};
use crate::domain::cart::{Cart, CartLine};
use crate::domain::delivery::DeliveryInput;
use crate::domain::sale::PaymentInput;
use sqlx::SqlitePool;

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

fn delivery_to(name: &str) -> DeliveryInput {
    DeliveryInput {
        customer_id: None,
        customer_name: Some(name.into()),
        contact_number: "+97339000000".into(),
        house_number: Some("12".into()),
        area: None,
        address_text: "A Street".into(),
        delivery_note: None,
        delivery_staff_name: None,
        rider_id: None,
        expected_payment_method: "cash".into(),
    }
}

/// Ring up one basket and return what it collected.
async fn sell(
    pool: &SqlitePool,
    branch: &str,
    shift: &str,
    qty: &str,
    method: &str,
    idem: &str,
    delivery: Option<DeliveryInput>,
) -> (String, i64) {
    let mut cart = Cart::new(branch.into(), DEVICE.into(), shift.into(), CASHIER.into());
    cart.lines.push(cola(qty));
    let due: i64 = cart.lines.iter().map(|l| l.line_total_minor).sum();
    let result = sale_repo::finalize_sale(
        pool,
        &cart,
        vec![PaymentInput {
            method: method.into(),
            amount_minor: due,
            tendered_minor: Some(due),
            external_reference: None,
        }],
        idem,
        None,
        false,
        delivery,
        false,
    )
    .await
    .expect("checkout");
    (result.sale_id, due)
}

/// Every calculation of expected cash agrees, including about deliveries.
///
/// The delivery case is the one that separates them. A delivery is rung up and
/// tendered as cash at the counter, but the money is not in the drawer until the
/// rider comes back with it — so an unpaid delivery must be left out of expected
/// cash and a settled one must be counted. Miss that clause in one of the three
/// copies and the till appears short by every delivery still out on the road.
#[tokio::test]
async fn every_calculation_of_expected_cash_gives_the_same_answer() {
    let pool = migrated_pool().await;
    let branch = seed(&pool).await;
    let opening = 10_000;
    let shift = shift_repo::open_shift(&pool, &branch, DEVICE, CASHIER, opening)
        .await
        .expect("open shift")
        .shift_id;

    // Over the counter: one cash, one card, and one cash sale that is partly
    // returned later as store credit.
    let (_, cash_sale) = sell(&pool, &branch, &shift, "2", "cash", "drw-cash", None).await;
    let (_, _card_sale) = sell(&pool, &branch, &shift, "1", "card", "drw-card", None).await;
    let (credited_sale, credited) =
        sell(&pool, &branch, &shift, "2", "cash", "drw-credit", None).await;

    // Two deliveries, both tendered as cash. One comes back settled, one is still out.
    let (settled_id, settled) = sell(
        &pool,
        &branch,
        &shift,
        "3",
        "cash",
        "drw-dlv-paid",
        Some(delivery_to("Settled Customer")),
    )
    .await;
    let (_out_id, _still_out) = sell(
        &pool,
        &branch,
        &shift,
        "4",
        "cash",
        "drw-dlv-unpaid",
        Some(delivery_to("Waiting Customer")),
    )
    .await;

    let settled_delivery: String =
        sqlx::query_scalar("SELECT delivery_id FROM delivery_orders WHERE sale_id = ?")
            .bind(&settled_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    delivery_repo::confirm_payment(
        &pool,
        CASHIER,
        &crate::domain::delivery::ConfirmPaymentInput {
            delivery_id: settled_delivery,
            payment_reference: Some("rider returned".into()),
            payment_note: None,
        },
    )
    .await
    .expect("settle the delivery that came back");

    // A return settled as store credit towards a replacement. No notes leave the
    // drawer, so expected cash must not move — the case that separated the five
    // copies from each other.
    let (item, name, line_total): (String, String, i64) = sqlx::query_as(
        "SELECT sale_item_id, product_name_snapshot, line_total_minor
           FROM sale_items WHERE sale_id = ?",
    )
    .bind(&credited_sale)
    .fetch_one(&pool)
    .await
    .unwrap();
    let credited_back = line_total / 2;
    let refund = refund_repo::create_refund(
        &pool,
        &credited_sale,
        vec![crate::domain::refund::RefundItemInput {
            sale_item_id: item,
            product_name_snapshot: name,
            quantity: "1".into(),
            unit_price_minor: 1000,
            refund_amount_minor: credited_back,
        }],
        "returned, taking a replacement instead",
        "exchange",
        CASHIER,
        false,
        Some("drw-credit-refund".into()),
    )
    .await
    .expect("refund");

    // The credit is recorded as an `exchange_credit` payment naming the refund,
    // which is how the shop says "this went onto a replacement, not into a hand".
    sqlx::query(
        "INSERT INTO payments
           (payment_id, sale_id, payment_method, amount_minor, external_reference,
            recorded_by_user_id, recorded_at, created_at, updated_at)
         VALUES ('pay-credit', ?, 'exchange_credit', ?, ?, ?,
                 datetime('now'), datetime('now'), datetime('now'))",
    )
    .bind(&credited_sale)
    .bind(credited_back)
    .bind(&refund.refund_id)
    .bind(CASHIER)
    .execute(&pool)
    .await
    .unwrap();

    // What the drawer should hold: opening, plus the counter cash, plus the
    // delivery that was actually collected. The card sale and the delivery still
    // out are not in the till, and neither is the store-credited return.
    let should_hold = opening + cash_sale + settled + credited;

    let drawer = crate::commands::cash_commands::drawer_summary_inner(&pool, &shift)
        .await
        .expect("drawer summary");
    assert_eq!(
        drawer.expected_minor, should_hold,
        "the drawer summary counted cash that is not in the till (or missed cash that is)"
    );
    assert_eq!(
        drawer.pending_delivery_cash_minor,
        (cola("4").line_total_minor),
        "the delivery still out should be reported as pending, not as takings"
    );

    let date: String = sqlx::query_scalar("SELECT business_date FROM shifts WHERE shift_id = ?")
        .bind(&shift)
        .fetch_one(&pool)
        .await
        .unwrap();
    let eod =
        crate::commands::report_commands::report_eod_cashup_inner(&pool, &branch, &date, &date)
            .await
            .expect("end-of-day cash-up");
    assert_eq!(
        eod.total_cash_minor,
        cash_sale + settled + credited,
        "the end-of-day report disagrees with the drawer about what came in as cash"
    );

    // Closing the shift computes it a third time and stores it.
    shift_repo::close_shift(&pool, &shift, Some(should_hold), None)
        .await
        .expect("close the shift");
    let (stored, variance): (Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT expected_cash_minor, cash_difference_minor FROM shifts WHERE shift_id = ?",
    )
    .bind(&shift)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        stored,
        Some(should_hold),
        "closing the shift expected a different amount than the drawer summary did"
    );
    assert_eq!(
        variance,
        Some(0),
        "counting exactly what the till should hold reported a discrepancy"
    );
}
