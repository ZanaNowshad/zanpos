#![cfg(test)]
//! Voids.
//!
//! Reversing a sale outright: the status, the stock and the audit entry move
//! together or not at all.
//!
//! Part of the reversal suite; the shared fixtures live in
//! [`super`](super).

use super::super::{migrated_pool, one_real_sale, CASHIER};
use crate::db::repositories::sale_repo;

// ── Voids ────────────────────────────────────────────────────────────────────

/// A void puts the stock back, writes an audit row, and cannot be repeated.
///
/// All three in one transaction. The assistant used to void through five
/// separate statements with a permission check partway down the stock loop, so a
/// refused void left the sale reversed, the shelf untouched and nothing in the
/// audit log. Both callers go through `sale_repo::void_sale` now.
#[tokio::test]
async fn a_void_reverses_the_sale_the_stock_and_the_record_together() {
    let pool = migrated_pool().await;
    let before: f64 = sqlx::query_scalar::<_, String>(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = 'prd_inv'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap()
    .map_or(0.0, |q| q.parse().unwrap_or(0.0));
    let (sale, _shift) = one_real_sale(&pool, "2", "rev-void-atomic").await;

    sale_repo::void_sale(&pool, &sale, CASHIER, Some("customer changed their mind"))
        .await
        .expect("void");

    let status: String = sqlx::query_scalar("SELECT status FROM sales WHERE sale_id = ?")
        .bind(&sale)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "voided");

    let after: f64 = sqlx::query_scalar::<_, String>(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = 'prd_inv'",
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .parse()
    .unwrap();
    assert_eq!(
        after,
        before + 100.0,
        "the two units went back on the shelf"
    );

    let (events, reason): (i64, Option<String>) = sqlx::query_as(
        "SELECT COUNT(*), MAX(reason) FROM audit_logs
          WHERE entity_id = ? AND event_type = 'sale.voided'",
    )
    .bind(&sale)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(events, 1, "a void must leave exactly one audit entry");
    assert_eq!(
        reason.as_deref(),
        Some("customer changed their mind"),
        "the reason belongs in the record, not only on the screen"
    );

    // A second void finds nothing to void.
    assert!(
        sale_repo::void_sale(&pool, &sale, CASHIER, None)
            .await
            .is_err(),
        "a sale was voided twice — the stock would come back twice"
    );
}
