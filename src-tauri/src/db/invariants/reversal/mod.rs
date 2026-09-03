#![cfg(test)]
//! Money going back, and the paper trail it has to leave.
//!
//! A reversal is the only operation in the till that moves money towards the
//! customer, so it is the one an audit looks at first. What matters is not that
//! `create_refund` returns `Ok` but that afterwards the original sale is still
//! there unchanged, the reversal is a row of its own that points at it, and the
//! two together explain the balance. A refund that edited the sale instead would
//! balance perfectly and tell nobody anything.

mod balances;
mod receipts;
mod refunds;
mod voids;

use crate::domain::refund::RefundItemInput;
use sqlx::SqlitePool;

async fn only_line(pool: &SqlitePool, sale_id: &str) -> (String, String, i64) {
    sqlx::query_as(
        "SELECT sale_item_id, product_name_snapshot, line_total_minor FROM sale_items WHERE sale_id = ?",
    )
    .bind(sale_id)
    .fetch_one(pool)
    .await
    .expect("the sale has a line")
}

fn refund_item(id: &str, name: &str, qty: &str, amount: i64) -> RefundItemInput {
    RefundItemInput {
        sale_item_id: id.into(),
        product_name_snapshot: name.into(),
        quantity: qty.into(),
        unit_price_minor: 1000,
        refund_amount_minor: amount,
    }
}
