#![cfg(test)]
//! Stock through a sale and back.
//!
//! The ordinary arc: goods leave the shelf when they are sold and return
//! when they are brought back, with the books balancing at every step.
//!
//! Part of the stock suite; the shared fixtures live in [`super`](super).

use super::super::{migrated_pool, one_real_sale, seed, CASHIER, DEVICE, TAX_VAT};
use super::{books_balance, branch_of, cached, PRODUCT};
use crate::db::repositories::{refund_repo, sale_repo, shift_repo};
use crate::domain::cart::{Cart, CartLine};
use crate::domain::refund::RefundItemInput;
use crate::domain::sale::PaymentInput;
use crate::inventory::reconcile;

/// Sell, refund, and check the books at every step.
///
/// The whole arc the brief asks for: stock goes down by what was sold, comes back
/// by what was returned, and at no point does the cached quantity stop being
/// explainable by the ledger.
#[tokio::test]
async fn a_sale_then_a_refund_leaves_the_books_balanced_at_every_step() {
    let pool = migrated_pool().await;
    let (sale, _shift) = one_real_sale(&pool, "4", "stk-arc").await;
    let branch = branch_of(&pool).await;

    assert_eq!(cached(&pool).await, 96.0, "four off a shelf of a hundred");
    books_balance(&pool, "a sale").await;

    let (item, name, total): (String, String, i64) = sqlx::query_as(
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
            quantity: "1".into(),
            unit_price_minor: 1000,
            refund_amount_minor: total / 4,
        }],
        "one back",
        "customer_return",
        CASHIER,
        false,
        Some("stk-arc-refund".into()),
    )
    .await
    .expect("refund");

    assert_eq!(cached(&pool).await, 97.0, "one came back onto the shelf");
    books_balance(&pool, "a refund").await;

    // And the account reads as a story: opening, sale, refund.
    let account = reconcile::explain(&pool, PRODUCT, &branch)
        .await
        .expect("the account must be readable");
    let kinds: Vec<&str> = account.iter().map(|m| m.movement_type.as_str()).collect();
    assert_eq!(
        kinds,
        vec!["sale", "refund"],
        "the movements do not describe what happened"
    );
    assert_eq!(account[0].quantity_delta.parse::<f64>().unwrap(), -4.0);
    assert_eq!(account[1].quantity_delta.parse::<f64>().unwrap(), 1.0);
}

/// Signs point the right way: a sale is negative, a return is positive.
#[tokio::test]
async fn a_sale_moves_stock_down_and_a_return_moves_it_up() {
    let pool = migrated_pool().await;
    let (sale, _shift) = one_real_sale(&pool, "3", "stk-signs").await;
    let branch = branch_of(&pool).await;

    sale_repo::void_sale(&pool, &sale, CASHIER, Some("changed their mind"))
        .await
        .expect("void");

    let account = reconcile::explain(&pool, PRODUCT, &branch).await.unwrap();
    let deltas: Vec<f64> = account
        .iter()
        .map(|m| m.quantity_delta.parse().unwrap())
        .collect();
    assert_eq!(deltas, vec![-3.0, 3.0], "a sale down, a void back up");
    assert_eq!(
        deltas.iter().sum::<f64>(),
        0.0,
        "a sale and its reversal must cancel exactly"
    );
    assert_eq!(cached(&pool).await, 100.0, "the shelf is as it started");
    books_balance(&pool, "a void").await;
}

/// Fractional quantities survive the round trip.
///
/// Loose produce is sold by weight, so the ledger has to carry a fraction
/// through a sale and back through a refund without the shelf count drifting.
#[tokio::test]
async fn a_weighed_sale_and_its_refund_leave_no_residue() {
    let pool = migrated_pool().await;
    let branch = seed(&pool).await;
    sqlx::query("UPDATE products SET allow_decimal_quantity = 1 WHERE product_id = ?")
        .bind(PRODUCT)
        .execute(&pool)
        .await
        .unwrap();
    let shift = shift_repo::open_shift(&pool, &branch, DEVICE, CASHIER, 10_000)
        .await
        .expect("open shift")
        .shift_id;

    let mut cart = Cart::new(branch.clone(), DEVICE.into(), shift, CASHIER.into());
    cart.lines.push(CartLine::new(
        Some(PRODUCT.into()),
        "Cola 330ml".into(),
        None,
        None,
        "1.25",
        1000,
        TAX_VAT.into(),
        1000,
        false,
    ));
    let due: i64 = cart.lines.iter().map(|l| l.line_total_minor).sum();
    let sale = sale_repo::finalize_sale(
        &pool,
        &cart,
        vec![PaymentInput {
            method: "cash".into(),
            amount_minor: due,
            tendered_minor: Some(due),
            external_reference: None,
        }],
        "stk-weighed",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("checkout");

    assert_eq!(
        cached(&pool).await,
        98.75,
        "the shelf came down by the weight"
    );
    books_balance(&pool, "a weighed sale").await;

    let (item, name, total): (String, String, i64) = sqlx::query_as(
        "SELECT sale_item_id, product_name_snapshot, line_total_minor
           FROM sale_items WHERE sale_id = ?",
    )
    .bind(&sale.sale_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    refund_repo::create_refund(
        &pool,
        &sale.sale_id,
        vec![RefundItemInput {
            sale_item_id: item,
            product_name_snapshot: name,
            quantity: "1.25".into(),
            unit_price_minor: 1000,
            refund_amount_minor: total,
        }],
        "all of it back",
        "customer_return",
        CASHIER,
        false,
        Some("stk-weighed-refund".into()),
    )
    .await
    .expect("refund");

    assert_eq!(
        cached(&pool).await,
        100.0,
        "returning the whole weight must restore the shelf exactly"
    );
    books_balance(&pool, "a weighed refund").await;
}
