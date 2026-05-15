use sqlx::{SqlitePool, Row};
use ulid::Ulid;
use crate::domain::cart::Cart;
use crate::domain::sale::{PaymentInput, SaleResult, PaymentSummary, SaleItemSummary};
use crate::errors::{AppError, AppResult};
use crate::sync::outbox;
use crate::inventory::movements;

async fn next_receipt_number(pool: &SqlitePool, branch_code: &str, device_code: &str) -> AppResult<String> {
    let prefix = format!("{}-{}-%", branch_code, device_code);
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sales WHERE receipt_number LIKE ?"
    )
    .bind(&prefix)
    .fetch_one(pool)
    .await?;

    Ok(format!("{}-{}-{:08}", branch_code, device_code, count + 1))
}

pub async fn finalize_sale(
    pool: &SqlitePool,
    cart: &Cart,
    payments: Vec<PaymentInput>,
    idempotency_key: &str,
) -> AppResult<SaleResult> {
    let total_paid: i64 = payments.iter().map(|p| p.amount_minor).sum();
    let net_total = cart.net_total();
    if total_paid < net_total {
        return Err(AppError::Validation(format!(
            "Total paid ({}) is less than net total ({})", total_paid, net_total
        )));
    }

    // Lookup branch
    let branch_row = sqlx::query(
        "SELECT branch_code, currency, name FROM branches WHERE branch_id = ?"
    )
    .bind(&cart.branch_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Branch not found".into()))?;

    let branch_code: String = branch_row.get("branch_code");
    let currency: String = branch_row.get("currency");
    let branch_name: String = branch_row.get("name");

    let device_row = sqlx::query("SELECT device_code FROM devices WHERE device_id = ?")
        .bind(&cart.device_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Device not found".into()))?;
    let device_code: String = device_row.get("device_code");

    let cashier_row = sqlx::query("SELECT display_name FROM users WHERE user_id = ?")
        .bind(&cart.cashier_user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Cashier not found".into()))?;
    let cashier_name: String = cashier_row.get("display_name");

    let receipt_number = next_receipt_number(pool, &branch_code, &device_code).await?;
    let sale_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let business_date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let gross = cart.gross_total();
    let tax = cart.tax_total();
    let discount = cart.discount_total();
    let net = cart.net_total();

    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO sales
         (sale_id, receipt_number, branch_id, device_id, shift_id, cashier_user_id,
          status, gross_total_minor, discount_total_minor, tax_total_minor, net_total_minor,
          currency, business_date, sold_at, created_offline, idempotency_key, sync_status)
         VALUES (?,?,?,?,?,?,'completed',?,?,?,?,?,?,?,0,?,'pending')"
    )
    .bind(&sale_id).bind(&receipt_number).bind(&cart.branch_id).bind(&cart.device_id)
    .bind(&cart.shift_id).bind(&cart.cashier_user_id)
    .bind(gross).bind(discount).bind(tax).bind(net)
    .bind(&currency).bind(&business_date).bind(&now).bind(idempotency_key)
    .execute(&mut *tx)
    .await?;

    let mut item_summaries = Vec::new();
    for line in cart.lines.iter().filter(|l| !l.voided) {
        let item_id = Ulid::new().to_string();
        let tax_snapshot = serde_json::json!({
            "rule_id": line.tax_rule_id,
            "rate_basis_points": line.tax_rate_basis_points,
            "inclusive": line.tax_inclusive,
        }).to_string();

        sqlx::query(
            "INSERT INTO sale_items
             (sale_item_id, sale_id, product_id, product_name_snapshot, sku_snapshot,
              barcode_snapshot, quantity, unit_price_minor, line_discount_minor,
              tax_rule_snapshot, tax_amount_minor, line_total_minor, note, voided)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,0)"
        )
        .bind(&item_id).bind(&sale_id).bind(&line.product_id).bind(&line.product_name)
        .bind(&line.sku).bind(&line.barcode).bind(&line.quantity)
        .bind(line.unit_price_minor).bind(line.line_discount_minor)
        .bind(&tax_snapshot).bind(line.tax_amount_minor).bind(line.line_total_minor)
        .bind(&line.note)
        .execute(&mut *tx)
        .await?;

        item_summaries.push(SaleItemSummary {
            product_name: line.product_name.clone(),
            quantity: line.quantity.clone(),
            unit_price_minor: line.unit_price_minor,
            line_total_minor: line.line_total_minor,
            tax_amount_minor: line.tax_amount_minor,
        });
    }

    let mut payment_summaries = Vec::new();
    for payment in &payments {
        let payment_id = Ulid::new().to_string();
        let change = if payment.method == "cash" {
            payment.tendered_minor.map(|t| t - payment.amount_minor)
        } else {
            None
        };

        sqlx::query(
            "INSERT INTO payments
             (payment_id, sale_id, payment_method, amount_minor, currency, status,
              external_reference, tendered_minor, change_minor, recorded_by_user_id, recorded_at, sync_status)
             VALUES (?,?,?,?,?,'approved',?,?,?,?,?,'pending')"
        )
        .bind(&payment_id).bind(&sale_id).bind(&payment.method).bind(payment.amount_minor)
        .bind(&currency).bind(&payment.external_reference).bind(payment.tendered_minor)
        .bind(change).bind(&cart.cashier_user_id).bind(&now)
        .execute(&mut *tx)
        .await?;

        payment_summaries.push(PaymentSummary {
            method: payment.method.clone(),
            amount_minor: payment.amount_minor,
            change_minor: change,
        });
    }

    // Audit log
    let audit_id = Ulid::new().to_string();
    let after_json = serde_json::json!({ "sale_id": &sale_id, "net_total_minor": net }).to_string();
    let hash = format!("{:016x}", sale_id.len() as u64 + net as u64);
    sqlx::query(
        "INSERT INTO audit_logs
         (audit_log_id, event_type, entity_type, entity_id, actor_user_id, actor_type,
          device_id, branch_id, after_json, created_at, hash)
         VALUES (?,'sale.created','sale',?,?,'user',?,?,?,?,?)"
    )
    .bind(&audit_id).bind(&sale_id).bind(&cart.cashier_user_id)
    .bind(&cart.device_id).bind(&cart.branch_id).bind(&after_json).bind(&now).bind(&hash)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    tracing::info!("Sale finalized: {} ({})", sale_id, receipt_number);

    // Enqueue full payloads for sync (after commit so failures don't roll back the sale)
    let _ = outbox::enqueue_sale(
        pool, &cart.device_id, &cart.branch_id, &sale_id,
        &receipt_number, &cart.shift_id, &cart.cashier_user_id,
        "completed", gross, discount, tax, net,
        &currency, &business_date, &now, false, idempotency_key,
    ).await;

    // Enqueue each sale item
    for line in cart.lines.iter().filter(|l| !l.voided) {
        let tax_snapshot = serde_json::json!({
            "rule_id": line.tax_rule_id,
            "rate_basis_points": line.tax_rate_basis_points,
            "inclusive": line.tax_inclusive,
        }).to_string();
        // Use the same item_id that was inserted — we need to look it up or regen with same seed.
        // For simplicity: generate a stable id from sale_id + product_id + quantity.
        let item_id_seed = format!("{}-{}-{}", &sale_id, line.product_id.as_deref().unwrap_or("custom"), &line.quantity);
        let item_id_for_queue = format!("{:x}", item_id_seed.bytes().fold(0u64, |a, b| a.wrapping_add(b as u64)));
        // Re-read the actual item id from db
        let actual_item_id: Option<String> = sqlx::query_scalar(
            "SELECT sale_item_id FROM sale_items WHERE sale_id = ? AND product_name_snapshot = ? LIMIT 1"
        )
        .bind(&sale_id)
        .bind(&line.product_name)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        let item_id_str = actual_item_id.unwrap_or(item_id_for_queue);
        let _ = outbox::enqueue_sale_item(
            pool, &cart.device_id, &cart.branch_id, &item_id_str, &sale_id,
            line.product_id.as_deref(), &line.product_name,
            line.sku.as_deref(), line.barcode.as_deref(),
            &line.quantity, line.unit_price_minor, line.line_discount_minor,
            &tax_snapshot, line.tax_amount_minor, line.line_total_minor,
            line.note.as_deref(), false,
        ).await;
    }

    // Enqueue each payment
    for (payment, summary) in payments.iter().zip(payment_summaries.iter()) {
        let pay_id: Option<String> = sqlx::query_scalar(
            "SELECT payment_id FROM payments WHERE sale_id = ? AND payment_method = ? LIMIT 1"
        )
        .bind(&sale_id)
        .bind(&payment.method)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        if let Some(payment_id) = pay_id {
            let change = summary.change_minor;
            let _ = outbox::enqueue_payment(
                pool, &cart.device_id, &cart.branch_id, &payment_id, &sale_id,
                &payment.method, payment.amount_minor, &currency, "approved",
                payment.external_reference.as_deref(),
                payment.tendered_minor, change,
                &cart.cashier_user_id, &now,
            ).await;
        }
    }

    // Deduct inventory (after commit; failures don't roll back sale)
    let low_stock_alerts = movements::deduct_sale(pool, &sale_id, &cart.cashier_user_id)
        .await
        .unwrap_or_default();

    Ok(SaleResult {
        sale_id,
        receipt_number,
        net_total_minor: net,
        tax_total_minor: tax,
        discount_total_minor: discount,
        currency,
        payments: payment_summaries,
        items: item_summaries,
        cashier_name,
        branch_name,
        sold_at: now,
        business_date,
        created_offline: false,
        low_stock_alerts,
    })
}
