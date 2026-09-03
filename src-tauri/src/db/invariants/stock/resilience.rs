#![cfg(test)]
//! Retries, failures and policy.
//!
//! What the shelf count does when things go wrong: a repeated Charge, a sale
//! that fails partway, a deliberate oversell, and two products merged into
//! one.
//!
//! Part of the stock suite; the shared fixtures live in [`super`](super).

use super::super::{migrated_pool, seed, CASHIER, DEVICE, TAX_VAT};
use super::{books_balance, cached, PRODUCT};
use crate::db::repositories::{sale_repo, shift_repo};
use crate::domain::cart::{Cart, CartLine};
use crate::domain::sale::PaymentInput;
use crate::inventory::reconcile;

/// Pressing Charge twice deducts stock once.
///
/// The second press replays the first sale, so it must not take the goods off
/// the shelf a second time or write a second movement.
#[tokio::test]
async fn a_repeated_sale_does_not_deduct_stock_twice() {
    let pool = migrated_pool().await;
    let branch = seed(&pool).await;
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
        "5",
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

    for _ in 0..3 {
        sale_repo::finalize_sale(&pool, &cart, pay(), "stk-replay", None, false, None, false)
            .await
            .expect("the replay must return the sale already made");
    }

    assert_eq!(cached(&pool).await, 95.0, "five sold, once");
    let movements = reconcile::explain(&pool, PRODUCT, &branch).await.unwrap();
    assert_eq!(movements.len(), 1, "one sale, one movement");
    books_balance(&pool, "three presses of Charge").await;
}

/// A sale that fails leaves the shelf untouched and the ledger empty.
///
/// The deduction and the movement are written by the same transaction as the
/// sale, so a failure anywhere in it takes all three back. A half-applied sale
/// would leave stock reduced for goods that were never sold.
#[tokio::test]
async fn a_failed_sale_moves_no_stock_and_writes_no_movement() {
    let pool = migrated_pool().await;
    let branch = seed(&pool).await;
    let shift = shift_repo::open_shift(&pool, &branch, DEVICE, CASHIER, 10_000)
        .await
        .expect("open shift")
        .shift_id;
    let before = cached(&pool).await;

    let mut cart = Cart::new(branch.clone(), DEVICE.into(), shift, CASHIER.into());
    cart.lines.push(CartLine::new(
        Some(PRODUCT.into()),
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

    // Two tenders summing to the right total, one of them zero: the sum check
    // passes, the payment loop refuses the zero, and the transaction unwinds.
    sale_repo::finalize_sale(
        &pool,
        &cart,
        vec![
            PaymentInput {
                method: "cash".into(),
                amount_minor: due,
                tendered_minor: Some(due),
                external_reference: None,
            },
            PaymentInput {
                method: "card".into(),
                amount_minor: 0,
                tendered_minor: None,
                external_reference: None,
            },
        ],
        "stk-fail",
        None,
        false,
        None,
        false,
    )
    .await
    .expect_err("a zero tender must fail the sale");

    assert_eq!(cached(&pool).await, before, "a failed sale moved stock");
    let movements = reconcile::explain(&pool, PRODUCT, &branch).await.unwrap();
    assert!(
        movements.is_empty(),
        "a failed sale left a movement behind: {movements:?}"
    );

    // The till still works afterwards.
    sale_repo::finalize_sale(
        &pool,
        &cart,
        vec![PaymentInput {
            method: "cash".into(),
            amount_minor: due,
            tendered_minor: Some(due),
            external_reference: None,
        }],
        "stk-fail-recover",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("the next sale must go through");
    books_balance(&pool, "a failure and a recovery").await;
}

/// The negative-stock policy decides, and either answer is explainable.
///
/// A shop that allows overselling is back-ordering; one that does not wants the
/// sale refused. Both are legitimate. What must not happen is stock going
/// negative silently when the flag says it should not, or a refusal leaving a
/// half-applied deduction.
#[tokio::test]
async fn the_negative_stock_flag_decides_and_the_books_balance_either_way() {
    let pool = migrated_pool().await;
    let branch = seed(&pool).await;
    let shift = shift_repo::open_shift(&pool, &branch, DEVICE, CASHIER, 10_000)
        .await
        .expect("open shift")
        .shift_id;

    let oversell = |idem: &'static str| {
        let branch = branch.clone();
        let shift = shift.clone();
        async move {
            let mut cart = Cart::new(branch, DEVICE.into(), shift, CASHIER.into());
            cart.lines.push(CartLine::new(
                Some(PRODUCT.into()),
                "Cola 330ml".into(),
                None,
                None,
                "500", // the shelf holds 100
                1000,
                TAX_VAT.into(),
                1000,
                false,
            ));
            let due: i64 = cart.lines.iter().map(|l| l.line_total_minor).sum();
            (cart, due, idem)
        }
    };

    // Flag off: refused, and nothing moves.
    let (cart, due, idem) = oversell("stk-neg-off").await;
    sale_repo::finalize_sale(
        &pool,
        &cart,
        vec![PaymentInput {
            method: "cash".into(),
            amount_minor: due,
            tendered_minor: Some(due),
            external_reference: None,
        }],
        idem,
        None,
        false,
        None,
        false,
    )
    .await
    .expect_err("overselling must be refused while the flag is off");
    assert_eq!(cached(&pool).await, 100.0, "a refused oversell moved stock");
    assert!(
        reconcile::explain(&pool, PRODUCT, &branch)
            .await
            .unwrap()
            .is_empty(),
        "a refused oversell left a movement"
    );

    // Flag on: allowed, goes negative, and the ledger explains it.
    let (cart, due, idem) = oversell("stk-neg-on").await;
    sale_repo::finalize_sale(
        &pool,
        &cart,
        vec![PaymentInput {
            method: "cash".into(),
            amount_minor: due,
            tendered_minor: Some(due),
            external_reference: None,
        }],
        idem,
        None,
        false,
        None,
        true,
    )
    .await
    .expect("with the flag on, the shop is choosing to back-order");
    assert_eq!(cached(&pool).await, -400.0, "500 sold from a shelf of 100");
    books_balance(&pool, "a deliberate oversell").await;
}

/// Merging two products moves their stock through the ledger, not around it.
///
/// The merge folds the loser's shelf quantity into the survivor. That is stock
/// changing hands, and it used to be written straight into `stock_levels` with
/// nothing recorded — so the survivor's count permanently exceeded what its
/// movements could account for.
#[tokio::test]
async fn merging_two_products_records_the_stock_it_moves() {
    let pool = migrated_pool().await;
    let branch = seed(&pool).await;
    sqlx::query(
        "INSERT INTO products
           (product_id, category_id, name, track_inventory, is_active, tax_rule_id,
            created_at, updated_at)
         VALUES ('prd_dupe','cat_inv','Cola 330ml (duplicate)', 1, 1, ?,
                 datetime('now'), datetime('now'))",
    )
    .bind(TAX_VAT)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO stock_levels
           (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at)
         VALUES ('SL-prd_dupe', 'prd_dupe', ?, '7', datetime('now'), datetime('now'))",
    )
    .bind(&branch)
    .execute(&pool)
    .await
    .unwrap();

    crate::db::repositories::product_dedup_repo::merge_products(
        &pool, "prd_dupe", PRODUCT, true, CASHIER,
    )
    .await
    .expect("merge");

    assert_eq!(
        cached(&pool).await,
        107.0,
        "the survivor should hold both shelves' worth"
    );

    let account = reconcile::explain(&pool, PRODUCT, &branch).await.unwrap();
    let merge_in = account
        .iter()
        .find(|m| m.movement_type == "merge")
        .expect("the survivor's jump must be explained by a movement");
    assert_eq!(merge_in.quantity_delta.parse::<f64>().unwrap(), 7.0);
    assert_eq!(
        merge_in.reference_id.as_deref(),
        Some("prd_dupe"),
        "the movement must name which product the stock came from"
    );
    assert_eq!(
        merge_in.created_by_user_id.as_deref(),
        Some(CASHIER),
        "a merge must record who did it"
    );

    // The source is accounted for too: its ledger ends at zero.
    let source = reconcile::explain(&pool, "prd_dupe", &branch)
        .await
        .unwrap();
    assert_eq!(
        source
            .last()
            .map(|m| m.quantity_after.parse::<f64>().unwrap()),
        Some(0.0),
        "the merged-away product should end with nothing on its shelf"
    );

    books_balance(&pool, "a product merge").await;
}
