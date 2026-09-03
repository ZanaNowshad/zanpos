#![cfg(test)]
//! Integrity: stock against its ledger, orphans, and refund limits.
//!
//! Split from the module root only to keep each file under the 500-line
//! rule; the fixtures they share live in `super`.

use super::{migrated_pool, one_real_sale, CASHIER};
use crate::db::repositories::refund_repo;

// ── Invariant 3: stock is the ledger ─────────────────────────────────────────

/// `quantity_on_hand == opening + sum(movements)`.
///
/// `stock_levels` is a cache of the `stock_movements` ledger. Checkout writes
/// both — the level inside the sale transaction, the movement just after — so
/// they can only agree if that pairing holds.
#[tokio::test]
async fn the_stock_cache_equals_the_movement_ledger() {
    let pool = migrated_pool().await;
    one_real_sale(&pool, "4", "inv-stock").await;

    let cached: f64 = sqlx::query_scalar(
        "SELECT CAST(quantity_on_hand AS REAL) FROM stock_levels WHERE product_id='prd_inv'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    // The ledger balance the way `apply::ledger_balance` computes it: anchor on
    // the earliest surviving movement's post-state, then add every delta after.
    let (anchor, anchor_at): (f64, String) = sqlx::query_as(
        "SELECT CAST(quantity_after AS REAL), created_at FROM stock_movements
          WHERE product_id='prd_inv' ORDER BY datetime(created_at) ASC, rowid ASC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let after: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(CAST(quantity_delta AS REAL)),0.0) FROM stock_movements
          WHERE product_id='prd_inv' AND datetime(created_at) > datetime(?)",
    )
    .bind(&anchor_at)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(
        (cached - (anchor + after)).abs() < 0.0005,
        "stock cache {cached} does not match the ledger {} for prd_inv",
        anchor + after
    );
    assert!(
        (cached - 96.0).abs() < 0.0005,
        "100 opening less 4 sold should leave 96, found {cached}"
    );
}

// ── Invariant 5: no orphans, no duplicates ───────────────────────────────────

/// Children point at parents that exist.
///
/// `sale_items` and `payments` declare a foreign key to `sales`; `sales`,
/// `refunds` and `shifts` declare none at all, so the links from a sale up to
/// its shift, branch and cashier rest on application discipline. This asserts
/// the discipline held for rows the real checkout wrote.
#[tokio::test]
async fn nothing_the_checkout_wrote_is_an_orphan() {
    let pool = migrated_pool().await;
    one_real_sale(&pool, "1", "inv-orphan").await;

    for (label, sql) in [
        (
            "sale_items with no sale",
            "SELECT COUNT(*) FROM sale_items i
              WHERE NOT EXISTS (SELECT 1 FROM sales s WHERE s.sale_id = i.sale_id)",
        ),
        (
            "payments with no sale",
            "SELECT COUNT(*) FROM payments p
              WHERE NOT EXISTS (SELECT 1 FROM sales s WHERE s.sale_id = p.sale_id)",
        ),
        (
            // Not enforced by a foreign key — `sales` declares none.
            "sales with no shift",
            "SELECT COUNT(*) FROM sales s
              WHERE NOT EXISTS (SELECT 1 FROM shifts f WHERE f.shift_id = s.shift_id)",
        ),
        (
            "stock movements for a product that does not exist",
            "SELECT COUNT(*) FROM stock_movements m
              WHERE NOT EXISTS (SELECT 1 FROM products p WHERE p.product_id = m.product_id)",
        ),
        (
            "refunds against a sale that does not exist",
            "SELECT COUNT(*) FROM refunds r
              WHERE NOT EXISTS (SELECT 1 FROM sales s WHERE s.sale_id = r.original_sale_id)",
        ),
    ] {
        let count: i64 = sqlx::query_scalar(sql).fetch_one(&pool).await.unwrap();
        assert_eq!(count, 0, "{label}: {count} orphan row(s)");
    }
}

/// Receipt numbers and idempotency keys are unique, and the database enforces
/// it rather than the application remembering to.
#[tokio::test]
async fn the_database_refuses_a_duplicate_receipt_or_idempotency_key() {
    let pool = migrated_pool().await;
    let (sale_id, _) = one_real_sale(&pool, "1", "inv-unique").await;

    let (receipt, idem): (String, String) =
        sqlx::query_as("SELECT receipt_number, idempotency_key FROM sales WHERE sale_id = ?")
            .bind(&sale_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let dup_receipt = sqlx::query(
        "INSERT INTO sales (sale_id, receipt_number, branch_id, device_id, origin_device_id,
             shift_id, cashier_user_id, status, gross_total_minor, discount_total_minor,
             tax_total_minor, net_total_minor, business_date, idempotency_key,
             sold_at, created_at, updated_at)
         SELECT 'DUP-1', ?, branch_id, device_id, origin_device_id, shift_id, cashier_user_id,
                status, gross_total_minor, discount_total_minor, tax_total_minor,
                net_total_minor, business_date, 'other-key', sold_at, created_at, updated_at
           FROM sales WHERE sale_id = ?",
    )
    .bind(&receipt)
    .bind(&sale_id)
    .execute(&pool)
    .await;
    assert!(
        dup_receipt.is_err(),
        "a second sale took receipt number {receipt} — the till's numbering is not unique"
    );

    let dup_idem = sqlx::query(
        "INSERT INTO sales (sale_id, receipt_number, branch_id, device_id, origin_device_id,
             shift_id, cashier_user_id, status, gross_total_minor, discount_total_minor,
             tax_total_minor, net_total_minor, business_date, idempotency_key,
             sold_at, created_at, updated_at)
         SELECT 'DUP-2', 'OTHER-RECEIPT', branch_id, device_id, origin_device_id, shift_id,
                cashier_user_id, status, gross_total_minor, discount_total_minor,
                tax_total_minor, net_total_minor, business_date, ?, sold_at, created_at, updated_at
           FROM sales WHERE sale_id = ?",
    )
    .bind(&idem)
    .bind(&sale_id)
    .execute(&pool)
    .await;
    assert!(
        dup_idem.is_err(),
        "a retried checkout could be recorded twice — idempotency_key is not unique"
    );
}

// ── Invariant 6: a refund cannot exceed what was paid ────────────────────────

/// Refunds against a sale never total more than the sale did.
#[tokio::test]
async fn a_refund_never_exceeds_the_sale_it_refunds() {
    let pool = migrated_pool().await;
    one_real_sale(&pool, "2", "inv-refund").await;

    let over: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT s.sale_id, s.net_total_minor, COALESCE(SUM(r.refund_total_minor),0)
           FROM sales s
           JOIN refunds r ON r.original_sale_id = s.sale_id
          GROUP BY s.sale_id, s.net_total_minor
         HAVING COALESCE(SUM(r.refund_total_minor),0) > s.net_total_minor",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(over.is_empty(), "refunded more than was sold: {over:?}");

    // The aggregate above passes trivially with no refunds, so the guard itself
    // is exercised: `refunds` has no CHECK constraint tying it to the sale, and
    // no foreign key either — `create_refund` is the only thing standing between
    // a shop and refunding more than it took.
    let (sale_id, sale_item_id, unit, net): (String, String, i64, i64) = sqlx::query_as(
        "SELECT s.sale_id, i.sale_item_id, i.unit_price_minor, s.net_total_minor
           FROM sales s JOIN sale_items i ON i.sale_id = s.sale_id LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let over_refund = refund_repo::create_refund(
        &pool,
        &sale_id,
        vec![crate::domain::refund::RefundItemInput {
            sale_item_id: sale_item_id.clone(),
            product_name_snapshot: "Cola 330ml".into(),
            quantity: "99".into(),
            unit_price_minor: unit,
            refund_amount_minor: net * 10,
        }],
        "test over-refund",
        "damaged",
        CASHIER,
        false,
        None,
    )
    .await;
    assert!(
        over_refund.is_err(),
        "refunding {} against a sale of {net} was accepted",
        net * 10
    );

    // And the refusal left nothing behind: a rejected refund that still wrote
    // rows would be worse than one that succeeded, because the totals would
    // disagree with the refund the shop believes it declined.
    let stragglers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refunds")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        stragglers, 0,
        "a refused refund left {stragglers} row(s) behind"
    );
}
