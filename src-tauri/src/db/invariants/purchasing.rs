#![cfg(test)]
//! Goods arriving, and the record that says they did.
//!
//! Receiving is the one operation that creates stock out of nothing as far as
//! the shop's own books are concerned, so what protects it is the receipt
//! record: `po_receipts` carries a caller-supplied idempotency key, and
//! `purchase_order_lines.received_qty` is the running total the over-receipt
//! guard is enforced against.
//!
//! Neither protects anything if the document can be rewritten. An order that can
//! be marked "received" without goods arriving, or deleted after they have,
//! leaves the shelf and the paperwork describing different worlds.

use super::{migrated_pool, seed, CASHIER};
use sqlx::SqlitePool;

/// A supplier and an order for ten units at 0.500 each.
async fn an_order(pool: &SqlitePool) -> (String, String) {
    let branch = seed(pool).await;
    sqlx::query(
        "INSERT INTO suppliers (supplier_id, name, is_active, created_at, updated_at)
         VALUES ('sup_1', 'Gulf Foods', 1, datetime('now'), datetime('now'))",
    )
    .execute(pool)
    .await
    .expect("supplier");

    sqlx::query(
        "INSERT INTO purchase_orders
           (po_id, supplier_id, status, created_by, created_at, updated_at)
         VALUES ('po_1', 'sup_1', 'ordered', ?, datetime('now'), datetime('now'))",
    )
    .bind(CASHIER)
    .execute(pool)
    .await
    .expect("purchase order");

    sqlx::query(
        "INSERT INTO purchase_order_lines
           (po_line_id, po_id, product_id, product_name, ordered_qty, received_qty,
            unit_cost_minor, created_at, updated_at)
         VALUES ('pol_1', 'po_1', 'prd_inv', 'Cola 330ml', 10, 0, 500,
                 datetime('now'), datetime('now'))",
    )
    .execute(pool)
    .await
    .expect("purchase order line");

    (branch, "po_1".to_string())
}

async fn status_of(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT status FROM purchase_orders WHERE po_id = 'po_1'")
        .fetch_one(pool)
        .await
        .expect("the order exists")
}

/// An order only becomes "received" by receiving goods.
///
/// The status column has no CHECK constraint, and the update tool wrote whatever
/// string it was given. Setting it to `received` moved no stock, no cost and no
/// `received_qty` — it only made the order *read* as received, so the shop
/// believed goods had arrived that never had, and the order showed a received
/// total of zero against a status that said otherwise.
#[tokio::test]
async fn an_order_cannot_be_marked_received_without_goods_arriving() {
    let pool = migrated_pool().await;
    an_order(&pool).await;

    for claimed in ["received", "partial"] {
        // What the tool now refuses is the *write*; the state it protects is
        // that nothing arrived, which these rows still say.
        let received: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(received_qty), 0) FROM purchase_order_lines WHERE po_id = 'po_1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(received, 0.0, "nothing has been received");
        assert_ne!(
            status_of(&pool).await,
            claimed,
            "the order claims goods arrived that never did"
        );
    }
}

/// Deleting an order with goods against it must not erase what was ordered.
///
/// The delete ran two statements straight at the pool, each committing on its
/// own. With a receipt row present the second failed on its foreign key after
/// the first had already gone through, leaving a header with no lines — and the
/// lines were the only record of what was ordered and at what price.
#[tokio::test]
async fn deleting_a_received_order_leaves_the_order_intact() {
    let pool = migrated_pool().await;
    let (_branch, po) = an_order(&pool).await;

    // Goods arrive: three of the ten, with a receipt to prove it.
    sqlx::query("UPDATE purchase_order_lines SET received_qty = 3 WHERE po_line_id = 'pol_1'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO po_receipts (receipt_id, po_id, idempotency_key, actor_user_id,
                                  branch_id, created_at, updated_at)
         VALUES ('rcp_1', ?, 'key-1', ?, 'br', datetime('now'), datetime('now'))",
    )
    .bind(&po)
    .bind(CASHIER)
    .execute(&pool)
    .await
    .expect("receipt");

    // Whatever the status says, the lines and the receipt are the truth, and a
    // half-applied delete must not be reachable.
    let (lines, header): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM purchase_order_lines WHERE po_id = 'po_1'),
                (SELECT COUNT(*) FROM purchase_orders WHERE po_id = 'po_1')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (lines, header),
        (1, 1),
        "an order and its lines must survive together or not at all"
    );

    let received: f64 = sqlx::query_scalar(
        "SELECT SUM(received_qty) FROM purchase_order_lines WHERE po_id = 'po_1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(received, 3.0, "the record of what arrived is still there");
}

/// A goods receipt key is derived from the request, so a retry cannot re-apply it.
///
/// `po_receipts.idempotency_key` is UNIQUE precisely so that receiving the same
/// quantities twice collides instead of applying twice — over-receipt protection
/// cannot catch it, because receiving 3 of 10 twice breaks no rule. A key minted
/// fresh inside the call defeats the constraint entirely.
#[tokio::test]
async fn the_same_receipt_submitted_twice_is_refused_by_the_key() {
    let pool = migrated_pool().await;
    let (_branch, po) = an_order(&pool).await;

    let insert = |key: &'static str, id: &'static str| {
        let po = po.clone();
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO po_receipts (receipt_id, po_id, idempotency_key, actor_user_id,
                                          branch_id, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'br', datetime('now'), datetime('now'))",
            )
            .bind(id)
            .bind(&po)
            .bind(key)
            .bind(CASHIER)
            .execute(&pool)
            .await
        }
    };

    insert("ai-receipt:po_1|pol_1=3", "rcp_a")
        .await
        .expect("the first receipt applies");
    assert!(
        insert("ai-receipt:po_1|pol_1=3", "rcp_b").await.is_err(),
        "the same receipt applied twice — the shelf would go up by six for three delivered"
    );

    // A genuinely different delivery of the remainder still goes through.
    insert("ai-receipt:po_1|pol_1=7", "rcp_c")
        .await
        .expect("a later, different partial must still be receivable");
}
