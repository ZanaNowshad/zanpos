#![cfg(test)]
//! Who moved it, and why.
//!
//! A quantity is only explainable if its movements carry their provenance:
//! what caused them, who did it, on which terminal, and when.
//!
//! Part of the stock suite; the shared fixtures live in [`super`](super).

use super::super::{migrated_pool, one_real_sale, CASHIER, DEVICE, TAX_VAT};
use super::{branch_of, PRODUCT};
use crate::db::repositories::sale_repo;
use crate::domain::cart::{Cart, CartLine};
use crate::domain::sale::PaymentInput;
use crate::inventory::reconcile;

/// Every movement says what caused it, who did it and where.
///
/// A quantity is only explainable if its movements carry their provenance. A row
/// with no source document, no user and no terminal records that something
/// happened without recording what, which is the same as not recording it.
#[tokio::test]
async fn every_movement_names_its_cause_its_author_and_its_terminal() {
    let pool = migrated_pool().await;
    let (sale, _shift) = one_real_sale(&pool, "2", "stk-prov").await;
    let branch = branch_of(&pool).await;

    let account = reconcile::explain(&pool, PRODUCT, &branch).await.unwrap();
    assert!(!account.is_empty(), "a sale must leave a movement");

    for m in &account {
        assert!(!m.movement_type.is_empty(), "a movement with no type");
        assert_eq!(
            m.reference_type.as_deref(),
            Some("sale"),
            "a movement that does not say what caused it"
        );
        assert_eq!(
            m.reference_id.as_deref(),
            Some(sale.as_str()),
            "a movement that does not name the document it belongs to"
        );
        assert_eq!(
            m.created_by_user_id.as_deref(),
            Some(CASHIER),
            "a movement with no author"
        );
        assert_eq!(
            m.device_id.as_deref(),
            Some(DEVICE),
            "a movement with no terminal"
        );
        assert!(
            !m.created_at.is_empty(),
            "a movement with no timestamp cannot be ordered, and the balance \
             depends on ordering"
        );
    }
}

/// No completed sale is missing the movements that explain its stock effect.
///
/// A database-wide statement rather than a single-sale one: whatever route a
/// sale took, every tracked line it sold must have a movement naming it. The
/// deduction and the movement are written by one transaction, so this cannot be
/// violated by a partial write — it stands as the guard that keeps it that way.
#[tokio::test]
async fn no_completed_sale_is_missing_its_stock_movements() {
    let pool = migrated_pool().await;
    let (first, shift) = one_real_sale(&pool, "2", "stk-cover-1").await;
    let branch = branch_of(&pool).await;

    // A second sale on the same shift, then voided: a void must not leave the
    // original sale looking as though its movements went missing.
    let mut cart = Cart::new(branch, DEVICE.into(), shift, CASHIER.into());
    cart.lines.push(CartLine::new(
        Some(PRODUCT.into()),
        "Cola 330ml".into(),
        None,
        None,
        "3",
        1000,
        TAX_VAT.into(),
        1000,
        false,
    ));
    let due: i64 = cart.lines.iter().map(|l| l.line_total_minor).sum();
    let second = sale_repo::finalize_sale(
        &pool,
        &cart,
        vec![PaymentInput {
            method: "cash".into(),
            amount_minor: due,
            tendered_minor: Some(due),
            external_reference: None,
        }],
        "stk-cover-2",
        None,
        false,
        None,
        false,
    )
    .await
    .expect("second sale");
    sale_repo::void_sale(&pool, &second.sale_id, CASHIER, None)
        .await
        .expect("void");
    let _ = first;

    let orphans: Vec<(String, String)> = sqlx::query_as(
        "SELECT s.receipt_number, si.product_name_snapshot
           FROM sale_items si
           JOIN sales s ON s.sale_id = si.sale_id
           JOIN products p ON p.product_id = si.product_id
          WHERE s.status = 'completed' AND si.voided = 0
            AND p.track_inventory = 1 AND si.product_id IS NOT NULL
            AND NOT EXISTS (
              SELECT 1 FROM stock_movements m
               WHERE m.reference_type = 'sale' AND m.reference_id = si.sale_id
                 AND m.product_id = si.product_id
            )",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert!(
        orphans.is_empty(),
        "these sold items took stock off the shelf with nothing recording it: {orphans:?}"
    );
}
