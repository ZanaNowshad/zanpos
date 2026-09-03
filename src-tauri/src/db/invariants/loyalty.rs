#![cfg(test)]
//! Points a customer has, and why they have them.
//!
//! Loyalty is a ledger: `loyalty_events` records each award, redemption and
//! correction with the delta applied and the running total it produced, and
//! `customers.loyalty_points` is a cache of the latest total. The balance can be
//! rebuilt from the events, which is what makes it defensible when a customer
//! disputes it.
//!
//! That only holds if every event that changes the balance is *in* the ledger.
//! Earning was; reversing was not — a sale could be refunded or voided and the
//! points it awarded stayed on the customer, so buying and returning the same
//! basket repeatedly earned loyalty at no cost.

use super::{migrated_pool, seed, CASHIER, DEVICE, TAX_VAT};
use crate::db::repositories::{loyalty_repo, refund_repo, sale_repo, shift_repo};
use crate::domain::cart::{Cart, CartLine};
use crate::domain::refund::RefundItemInput;
use crate::domain::sale::PaymentInput;
use sqlx::SqlitePool;

const CUSTOMER: &str = "cus_loyalty";

async fn till_with_customer(pool: &SqlitePool) -> (String, String) {
    let branch = seed(pool).await;
    sqlx::query(
        "INSERT INTO customers (customer_id, branch_id, origin_device_id, name,
                                loyalty_points, created_at, updated_at)
         VALUES (?, ?, ?, 'A Customer', 0, datetime('now'), datetime('now'))",
    )
    .bind(CUSTOMER)
    .bind(&branch)
    .bind(DEVICE)
    .execute(pool)
    .await
    .expect("seed the customer");

    let shift = shift_repo::open_shift(pool, &branch, DEVICE, CASHIER, 10_000)
        .await
        .expect("open shift")
        .shift_id;
    (branch, shift)
}

/// Ring up `qty` units for the seeded customer.
async fn sell_to_customer(
    pool: &SqlitePool,
    branch: &str,
    shift: &str,
    qty: &str,
    idem: &str,
) -> (String, i64) {
    let mut cart = Cart::new(branch.into(), DEVICE.into(), shift.into(), CASHIER.into());
    cart.lines.push(CartLine::new(
        Some("prd_inv".into()),
        "Cola 330ml".into(),
        None,
        None,
        qty,
        1000,
        TAX_VAT.into(),
        1000,
        false,
    ));
    let due: i64 = cart.lines.iter().map(|l| l.line_total_minor).sum();
    let result = sale_repo::finalize_sale(
        pool,
        &cart,
        vec![PaymentInput {
            method: "cash".into(),
            amount_minor: due,
            tendered_minor: Some(due),
            external_reference: None,
        }],
        idem,
        Some(CUSTOMER),
        false,
        None,
        false,
    )
    .await
    .expect("checkout");
    (result.sale_id, due)
}

async fn balance(pool: &SqlitePool) -> i64 {
    loyalty_repo::ledger_balance(pool, CUSTOMER)
        .await
        .expect("the ledger must be readable")
        .unwrap_or(0)
}

async fn cached(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT loyalty_points FROM customers WHERE customer_id = ?")
        .bind(CUSTOMER)
        .fetch_one(pool)
        .await
        .expect("the customer exists")
}

/// The whole arc: earn, redeem, refund, and a balance that still adds up.
#[tokio::test]
async fn a_sale_earns_a_redemption_spends_and_a_refund_gives_back() {
    let pool = migrated_pool().await;
    let (branch, shift) = till_with_customer(&pool).await;

    // 11 units at 1.000 + 10% VAT = 12.100 net → 12 points at one per BHD.
    let (sale, net) = sell_to_customer(&pool, &branch, &shift, "11", "loy-sale").await;
    let earned = net / 1000;
    assert_eq!(earned, 12);
    assert_eq!(balance(&pool).await, 12, "the sale awarded its points");

    // The customer spends five.
    loyalty_repo::record(
        &pool,
        loyalty_repo::AwardContext {
            customer_id: CUSTOMER,
            branch_id: Some(&branch),
            device_id: Some(DEVICE),
            event: loyalty_repo::LoyaltyEvent::Redeem,
            points_delta: -5,
            reference_type: None,
            reference_id: None,
            reason: Some("spent at the counter"),
            actor_user_id: Some(CASHIER),
        },
    )
    .await
    .expect("redeem");
    assert_eq!(balance(&pool).await, 7);

    // A quarter of the basket comes back, so a quarter of the award does too.
    let (item, name, line_total): (String, String, i64) = sqlx::query_as(
        "SELECT sale_item_id, product_name_snapshot, line_total_minor
           FROM sale_items WHERE sale_id = ?",
    )
    .bind(&sale)
    .fetch_one(&pool)
    .await
    .unwrap();
    refund_repo::create_refund(
        &pool,
        &sale,
        vec![RefundItemInput {
            sale_item_id: item,
            product_name_snapshot: name,
            quantity: "3".into(),
            unit_price_minor: 1000,
            refund_amount_minor: line_total / 4,
        }],
        "three back",
        "customer_return",
        CASHIER,
        false,
        Some("loy-refund".into()),
    )
    .await
    .expect("refund");

    // 12 awarded, a quarter refunded → 3 clawed back. 7 − 3 = 4.
    assert_eq!(
        balance(&pool).await,
        4,
        "the refund did not take back its share of the points"
    );
    assert_eq!(
        cached(&pool).await,
        balance(&pool).await,
        "cache and ledger disagree"
    );

    // Every row still accounts for itself, so the balance can be rebuilt.
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT points_delta, points_after FROM loyalty_events
          WHERE customer_id = ? ORDER BY datetime(created_at) ASC, rowid ASC",
    )
    .bind(CUSTOMER)
    .fetch_all(&pool)
    .await
    .unwrap();
    let mut running = 0i64;
    for (delta, after) in &rows {
        running += delta;
        assert_eq!(
            running, *after,
            "an event recorded a delta it did not apply"
        );
    }
}

/// Voiding a sale takes back everything it awarded.
#[tokio::test]
async fn voiding_a_sale_takes_back_the_points_it_gave() {
    let pool = migrated_pool().await;
    let (branch, shift) = till_with_customer(&pool).await;
    let (sale, net) = sell_to_customer(&pool, &branch, &shift, "11", "loy-void").await;
    assert_eq!(balance(&pool).await, net / 1000);

    sale_repo::void_sale(&pool, &sale, CASHIER, Some("rung up in error"))
        .await
        .expect("void");

    assert_eq!(
        balance(&pool).await,
        0,
        "a voided sale left its loyalty points on the customer"
    );
    assert_eq!(cached(&pool).await, 0);
}

/// Repeated partial refunds never claw back more than the sale awarded.
///
/// The reversal is proportional, so a sequence of partials has to sum to the
/// award and stop — otherwise refunding a basket piece by piece would take back
/// more points than it ever gave, and a customer could be driven negative by
/// returning what they bought.
#[tokio::test]
async fn partial_refunds_never_take_back_more_than_was_awarded() {
    let pool = migrated_pool().await;
    let (branch, shift) = till_with_customer(&pool).await;
    let (sale, net) = sell_to_customer(&pool, &branch, &shift, "11", "loy-partial").await;
    let awarded = net / 1000;

    let (item, name, line_total): (String, String, i64) = sqlx::query_as(
        "SELECT sale_item_id, product_name_snapshot, line_total_minor
           FROM sale_items WHERE sale_id = ?",
    )
    .bind(&sale)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Return the basket in four goes, covering the whole line.
    for (n, part) in [
        line_total / 4,
        line_total / 4,
        line_total / 4,
        line_total - 3 * (line_total / 4),
    ]
    .into_iter()
    .enumerate()
    {
        refund_repo::create_refund(
            &pool,
            &sale,
            vec![RefundItemInput {
                sale_item_id: item.clone(),
                product_name_snapshot: name.clone(),
                quantity: "2".into(),
                unit_price_minor: 1000,
                refund_amount_minor: part,
            }],
            "another piece back",
            "customer_return",
            CASHIER,
            false,
            Some(format!("loy-partial-{n}")),
        )
        .await
        .expect("each partial refund must go through");
    }

    let back: i64 = sqlx::query_scalar(
        "SELECT COALESCE(-SUM(points_delta), 0) FROM loyalty_events
          WHERE reference_type = 'sale_reversal' AND reference_id = ?",
    )
    .bind(&sale)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(
        back <= awarded,
        "refunding piece by piece clawed back {back} points from an award of {awarded}"
    );
    assert!(
        balance(&pool).await >= 0,
        "a customer was driven negative by returning what they bought"
    );
}
