use sqlx::{SqlitePool, Row};
use ulid::Ulid;
use crate::domain::refund::{SaleForRefund, SaleItemForRefund, RefundItemInput, RefundResult};
use crate::domain::sale::{SaleResult, SaleItemSummary, PaymentSummary};
use crate::errors::{AppError, AppResult};
use crate::sync::outbox;
use crate::inventory::movements;

pub async fn get_sale_by_receipt(pool: &SqlitePool, receipt_number: &str) -> AppResult<SaleForRefund> {
    let sale_row = sqlx::query(
        "SELECT s.sale_id, s.receipt_number, s.net_total_minor, s.currency, s.sold_at, s.status,
                u.display_name as cashier_name
         FROM sales s JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.receipt_number = ?"
    )
    .bind(receipt_number)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Receipt {} not found", receipt_number)))?;

    let sale_id: String = sale_row.get("sale_id");

    let item_rows = sqlx::query(
        "SELECT sale_item_id, product_name_snapshot, quantity, unit_price_minor, line_total_minor
         FROM sale_items WHERE sale_id = ? AND voided = 0"
    )
    .bind(&sale_id)
    .fetch_all(pool)
    .await?;

    let items = item_rows.iter().map(|r| SaleItemForRefund {
        sale_item_id: r.get("sale_item_id"),
        product_name_snapshot: r.get("product_name_snapshot"),
        quantity: r.get("quantity"),
        unit_price_minor: r.get("unit_price_minor"),
        line_total_minor: r.get("line_total_minor"),
    }).collect();

    Ok(SaleForRefund {
        sale_id,
        receipt_number: sale_row.get("receipt_number"),
        net_total_minor: sale_row.get("net_total_minor"),
        currency: sale_row.get("currency"),
        sold_at: sale_row.get("sold_at"),
        cashier_name: sale_row.get("cashier_name"),
        status: sale_row.get("status"),
        items,
    })
}

pub async fn get_sale_result_by_receipt(pool: &SqlitePool, receipt_number: &str) -> AppResult<SaleResult> {
    let sale_row = sqlx::query(
        "SELECT s.sale_id, s.receipt_number, s.net_total_minor, s.tax_total_minor,
                s.discount_total_minor, s.currency, s.sold_at, s.business_date, s.created_offline,
                u.display_name as cashier_name, b.name as branch_name
         FROM sales s
         JOIN users u ON u.user_id = s.cashier_user_id
         JOIN branches b ON b.branch_id = s.branch_id
         WHERE s.receipt_number = ?"
    )
    .bind(receipt_number)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Receipt {} not found", receipt_number)))?;

    let sale_id: String = sale_row.get("sale_id");

    let item_rows = sqlx::query(
        "SELECT product_name_snapshot, quantity, unit_price_minor, line_total_minor, tax_amount_minor
         FROM sale_items WHERE sale_id = ? AND voided = 0"
    )
    .bind(&sale_id)
    .fetch_all(pool)
    .await?;

    let items = item_rows.iter().map(|r| SaleItemSummary {
        product_name: r.get("product_name_snapshot"),
        quantity: r.get("quantity"),
        unit_price_minor: r.get("unit_price_minor"),
        line_total_minor: r.get("line_total_minor"),
        tax_amount_minor: r.get("tax_amount_minor"),
    }).collect();

    let payment_rows = sqlx::query(
        "SELECT payment_method, amount_minor, change_minor FROM payments WHERE sale_id = ?"
    )
    .bind(&sale_id)
    .fetch_all(pool)
    .await?;

    let payments = payment_rows.iter().map(|r| PaymentSummary {
        method: r.get("payment_method"),
        amount_minor: r.get("amount_minor"),
        change_minor: r.get("change_minor"),
    }).collect();

    let created_offline: i64 = sale_row.get("created_offline");

    Ok(SaleResult {
        sale_id,
        receipt_number: receipt_number.to_string(),
        net_total_minor: sale_row.get("net_total_minor"),
        tax_total_minor: sale_row.get("tax_total_minor"),
        discount_total_minor: sale_row.get("discount_total_minor"),
        currency: sale_row.get("currency"),
        payments,
        items,
        cashier_name: sale_row.get("cashier_name"),
        branch_name: sale_row.get("branch_name"),
        sold_at: sale_row.get("sold_at"),
        business_date: sale_row.get("business_date"),
        created_offline: created_offline != 0,
    })
}

async fn next_refund_receipt_number(pool: &SqlitePool, branch_code: &str, device_code: &str) -> AppResult<String> {
    let prefix = format!("{}-{}-REF-%", branch_code, device_code);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refunds WHERE refund_receipt_number LIKE ?")
        .bind(&prefix)
        .fetch_one(pool)
        .await?;
    Ok(format!("{}-{}-REF-{:08}", branch_code, device_code, count + 1))
}

pub async fn create_refund(
    pool: &SqlitePool,
    original_sale_id: &str,
    items: Vec<RefundItemInput>,
    reason: &str,
    created_by_user_id: &str,
) -> AppResult<RefundResult> {
    if items.is_empty() {
        return Err(AppError::Validation("No items selected for refund".into()));
    }

    let sale_row = sqlx::query(
        "SELECT s.currency, b.branch_code, d.device_code
         FROM sales s
         JOIN branches b ON b.branch_id = s.branch_id
         JOIN devices d ON d.device_id = s.device_id
         WHERE s.sale_id = ?"
    )
    .bind(original_sale_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Original sale not found".into()))?;

    let currency: String = sale_row.get("currency");
    let branch_code: String = sale_row.get("branch_code");
    let device_code: String = sale_row.get("device_code");

    let refund_total: i64 = items.iter().map(|i| i.refund_amount_minor).sum();
    let refund_id = Ulid::new().to_string();
    let refund_receipt_number = next_refund_receipt_number(pool, &branch_code, &device_code).await?;
    let now = chrono::Utc::now().to_rfc3339();
    let idempotency_key = format!("refund-{}", refund_id);

    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO refunds (refund_id, original_sale_id, refund_receipt_number, reason,
         refund_total_minor, currency, created_by_user_id, created_at, sync_status, idempotency_key)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?)"
    )
    .bind(&refund_id).bind(original_sale_id).bind(&refund_receipt_number)
    .bind(reason).bind(refund_total).bind(&currency)
    .bind(created_by_user_id).bind(&now).bind(&idempotency_key)
    .execute(&mut *tx)
    .await?;

    for item in &items {
        let refund_item_id = Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO refund_items (refund_item_id, refund_id, sale_item_id, product_name_snapshot, quantity, unit_price_minor, refund_amount_minor)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&refund_item_id).bind(&refund_id).bind(&item.sale_item_id)
        .bind(&item.product_name_snapshot).bind(&item.quantity)
        .bind(item.unit_price_minor).bind(item.refund_amount_minor)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query("UPDATE sales SET status = 'partially_refunded' WHERE sale_id = ? AND status = 'completed'")
        .bind(original_sale_id)
        .execute(&mut *tx)
        .await?;

    let audit_id = Ulid::new().to_string();
    let after_json = serde_json::json!({ "refund_id": &refund_id, "total_minor": refund_total }).to_string();
    let hash = format!("{:016x}", refund_id.len() as u64 + refund_total as u64);
    sqlx::query(
        "INSERT INTO audit_logs (audit_log_id, event_type, entity_type, entity_id,
         actor_user_id, actor_type, after_json, created_at, hash)
         VALUES (?, 'refund.created', 'refund', ?, ?, 'user', ?, ?, ?)"
    )
    .bind(&audit_id).bind(&refund_id).bind(created_by_user_id)
    .bind(&after_json).bind(&now).bind(&hash)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    tracing::info!("Refund created: {} ({})", refund_id, refund_receipt_number);

    // Return stock for refunded items (after commit; failures don't roll back refund)
    let _ = movements::return_refund(pool, &refund_id, created_by_user_id).await;

    // Enqueue refund for sync
    // Need device_id and branch_id from the original sale's device/branch
    let sale_meta = sqlx::query(
        "SELECT device_id, branch_id FROM sales WHERE sale_id = ?"
    )
    .bind(original_sale_id)
    .fetch_optional(pool)
    .await;

    if let Ok(Some(meta)) = sale_meta {
        let device_id: String = meta.get("device_id");
        let branch_id_str: String = meta.get("branch_id");
        let _ = outbox::enqueue_refund(
            pool, &device_id, &branch_id_str, &refund_id, original_sale_id,
            &refund_receipt_number, reason, refund_total, &currency,
            created_by_user_id, &now, &idempotency_key,
        ).await;

        for item in &items {
            let ri_id: Option<String> = sqlx::query_scalar(
                "SELECT refund_item_id FROM refund_items WHERE refund_id = ? AND sale_item_id = ? LIMIT 1"
            )
            .bind(&refund_id)
            .bind(&item.sale_item_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

            if let Some(refund_item_id) = ri_id {
                let _ = outbox::enqueue_refund_item(
                    pool, &device_id, &branch_id_str, &refund_item_id, &refund_id,
                    &item.sale_item_id, &item.product_name_snapshot,
                    &item.quantity, item.unit_price_minor, item.refund_amount_minor,
                ).await;
            }
        }
    }

    Ok(RefundResult {
        refund_id,
        refund_receipt_number,
        refund_total_minor: refund_total,
        currency,
        created_at: now,
    })
}
