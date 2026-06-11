use crate::db::repositories::{audit_hash, delivery_repo};
use crate::domain::cart::Cart;
use crate::domain::delivery::DeliveryInput;
use crate::domain::sale::{PaymentInput, PaymentSummary, SaleItemSummary, SaleResult};
use crate::errors::{AppError, AppResult};
use crate::inventory::movements;
use sqlx::{Row, SqlitePool};
use ulid::Ulid;

async fn next_receipt_number<'e, E>(
    executor: E,
    branch_code: &str,
    device_code: &str,
    device_id: &str,
) -> AppResult<String>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    // Atomic per-device counter — UPDATE...RETURNING prevents races.
    // The counter persists independently of sale rows; pruning old sales cannot cause collisions.
    let seq: i64 = sqlx::query_scalar(
        "UPDATE devices SET next_receipt_seq = next_receipt_seq + 1
         WHERE device_id = ?
         RETURNING next_receipt_seq",
    )
    .bind(device_id)
    .fetch_one(executor)
    .await?;

    Ok(format!("{}-{}-{:08}", branch_code, device_code, seq))
}

pub async fn finalize_sale(
    pool: &SqlitePool,
    cart: &Cart,
    payments: Vec<PaymentInput>,
    idempotency_key: &str,
    customer_id: Option<&str>,
    created_offline: bool,
    delivery: Option<DeliveryInput>,
    allow_negative_stock: bool,
) -> AppResult<SaleResult> {
    // ── Guard: shift must be open ─────────────────────────────────────────────
    let shift_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM shifts WHERE shift_id = ?",
    )
    .bind(&cart.shift_id)
    .fetch_optional(pool)
    .await?;

    match shift_status.as_deref() {
        Some("open") => {}
        Some(_) => {
            return Err(AppError::Validation(
                "Cannot finalize a sale: the shift is closed.".into(),
            ))
        }
        None => {
            return Err(AppError::Validation(
                "Cannot finalize a sale: shift not found.".into(),
            ))
        }
    }

    // ── Guard: cart must have at least one active (non-voided) line ────────────
    let active_line_count = cart.lines.iter().filter(|l| !l.voided).count();
    if active_line_count == 0 {
        return Err(AppError::Validation(
            "Cannot finalize an empty cart.".into(),
        ));
    }

    // ── Guard: validate quantities and totals are within safe ranges ──────────
    // Catches f64 overflow / malicious IPC input before any DB writes.
    cart.validate()?;

    // ── Guard: each cart line must have a positive price ─────────────────────
    // For product-mapped items, verify the price matches the DB to close the
    // price-manipulation attack vector (zero-price, manipulated IPC call).
    // Pre-batch: fetch all current prices in ONE query instead of N per-line queries.
    let active_lines: Vec<_> = cart.lines.iter().filter(|l| !l.voided).collect();
    let price_product_ids: Vec<&str> = active_lines
        .iter()
        .filter_map(|l| l.product_id.as_deref())
        .collect();

    let db_prices: std::collections::HashMap<String, i64> = if price_product_ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        let placeholders = price_product_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        // Subquery: for each product_id, pick the most recent effective price.
        let sql = format!(
            "SELECT pp.product_id, pp.price_minor
             FROM product_prices pp
             WHERE pp.product_id IN ({placeholders})
               AND pp.effective_from <= datetime('now')
               AND (pp.effective_to IS NULL OR pp.effective_to >= datetime('now'))
             GROUP BY pp.product_id
             HAVING pp.effective_from = MAX(pp.effective_from)"
        );
        let mut q = sqlx::query(&sql);
        for pid in &price_product_ids {
            q = q.bind(*pid);
        }
        q.fetch_all(pool)
            .await?
            .into_iter()
            .map(|row: sqlx::sqlite::SqliteRow| {
                let pid: String = row.get("product_id");
                let price: i64 = row.get("price_minor");
                (pid, price)
            })
            .collect()
    };

    for line in &active_lines {
        if line.unit_price_minor <= 0 {
            return Err(AppError::Validation(format!(
                "Item '{}' has an invalid price ({}). Please re-add it to the cart.",
                line.product_name, line.unit_price_minor
            )));
        }
        if let Some(ref product_id) = line.product_id {
            if let Some(&db_p) = db_prices.get(product_id.as_str()) {
                if line.unit_price_minor != db_p {
                    return Err(AppError::Validation(format!(
                        "Price for '{}' has changed (expected {} fils, got {} fils). \
                         Please re-add the item to the cart.",
                        line.product_name, db_p, line.unit_price_minor
                    )));
                }
            }
        }
    }

    // ── Server-side tax recalculation ────────────────────────────────────────
    // Recalculate tax and line totals for every active line using server-side
    // integer arithmetic. This closes the tax-manipulation vector: a compromised
    // frontend could craft arbitrary tax_amount_minor values in the IPC payload.
    // We compute gross, tax, and net from first principles and use those values
    // for all persistence steps, ignoring the frontend-supplied cart totals.
    let mut server_gross: i64 = 0;
    let mut server_line_taxes: Vec<i64> = Vec::with_capacity(active_lines.len());
    let mut server_line_totals: Vec<i64> = Vec::with_capacity(active_lines.len());

    for line in &active_lines {
        let subtotal =
            crate::domain::money::mul_minor_by_qty(line.unit_price_minor, &line.quantity);
        server_gross += subtotal;
        let discounted = (subtotal - line.line_discount_minor).max(0);
        let tax_amount = if line.tax_inclusive {
            let divisor = 10_000 + line.tax_rate_basis_points;
            (discounted * line.tax_rate_basis_points + divisor / 2) / divisor
        } else {
            crate::domain::money::calc_tax_exclusive(discounted, line.tax_rate_basis_points)
        };
        let line_total = discounted + if line.tax_inclusive { 0 } else { tax_amount };
        server_line_taxes.push(tax_amount);
        server_line_totals.push(line_total);
    }

    let server_tax: i64 = server_line_taxes.iter().sum();
    let server_post_line: i64 = server_line_totals.iter().sum();
    let server_net: i64 = (server_post_line - cart.bill_discount_minor).max(0);
    let server_discount: i64 = cart.discount_total();

    // ── Guard: payment amounts must sum exactly to server_net ───────────────
    // Validated against server-computed net, not frontend-supplied totals.
    // Cash overpayment is captured in tendered_minor/change_minor — NOT in amount_minor.
    let total_paid: i64 = payments.iter().map(|p| p.amount_minor).sum();
    if total_paid != server_net {
        return Err(AppError::Validation(format!(
            "Payment amounts ({}) must sum exactly to net total ({}). \
             Use tendered_minor for cash overpayment.",
            total_paid, server_net
        )));
    }

    // Lookup branch + device + cashier in a single round-trip (3 sequential queries → 1 JOIN).
    let ctx_row = sqlx::query(
        "SELECT b.branch_code, b.currency, b.name AS branch_name,
                d.device_code,
                u.display_name AS cashier_name
         FROM branches b
         JOIN devices d ON d.device_id = ?
         JOIN users   u ON u.user_id   = ?
         WHERE b.branch_id = ?",
    )
    .bind(&cart.device_id)
    .bind(&cart.cashier_user_id)
    .bind(&cart.branch_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Branch, device or cashier not found".into()))?;

    let branch_code: String = ctx_row.get("branch_code");
    let currency: String = ctx_row.get("currency");
    let branch_name: String = ctx_row.get("branch_name");
    let device_code: String = ctx_row.get("device_code");
    let cashier_name: String = ctx_row.get("cashier_name");

    let sale_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let business_date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let gross = server_gross;
    let tax = server_tax;
    let discount = server_discount;
    let net = server_net;

    // Open the transaction BEFORE generating the receipt number so that the
    // UPDATE counter and the sale INSERT are atomic. SQLite serialises writers,
    // so no two concurrent transactions can use the same counter value.
    let mut tx = pool.begin().await?;
    let receipt_number = next_receipt_number(&mut *tx, &branch_code, &device_code, &cart.device_id).await?;

    sqlx::query(
        "INSERT INTO sales
         (sale_id, receipt_number, branch_id, device_id, origin_device_id, shift_id, cashier_user_id,
          status, gross_total_minor, discount_total_minor, tax_total_minor, net_total_minor,
          currency, business_date, sold_at, created_offline, idempotency_key, sync_status,
          customer_id, is_delivery, created_at, updated_at)
         VALUES (?,?,?,?,?,?,?,'completed',?,?,?,?,?,?,?,?,?,'pending',?,?,?,?)",
    )
    .bind(&sale_id)
    .bind(&receipt_number)
    .bind(&cart.branch_id)
    .bind(&cart.device_id)
    .bind(&cart.device_id)
    .bind(&cart.shift_id)
    .bind(&cart.cashier_user_id)
    .bind(gross)
    .bind(discount)
    .bind(tax)
    .bind(net)
    .bind(&currency)
    .bind(&business_date)
    .bind(&now)
    .bind(created_offline as i64)
    .bind(idempotency_key)
    .bind(customer_id)
    .bind(delivery.is_some() as i64)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    let mut item_summaries = Vec::new();
    for (i, line) in active_lines.iter().enumerate() {
        let item_id = Ulid::new().to_string();
        let line_tax = server_line_taxes[i];
        let line_total = server_line_totals[i];
        let tax_snapshot = serde_json::json!({
            "rule_id": line.tax_rule_id,
            "rate_basis_points": line.tax_rate_basis_points,
            "inclusive": line.tax_inclusive,
        })
        .to_string();

        sqlx::query(
            "INSERT INTO sale_items
             (sale_item_id, sale_id, origin_device_id, product_id, product_name_snapshot, sku_snapshot,
              barcode_snapshot, quantity, unit_price_minor, line_discount_minor,
              tax_rule_snapshot, tax_amount_minor, line_total_minor, note, voided,
              created_at, updated_at)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,0,?,?)",
        )
        .bind(&item_id)
        .bind(&sale_id)
        .bind(cart.device_id.as_str())
        .bind(&line.product_id)
        .bind(&line.product_name)
        .bind(&line.sku)
        .bind(&line.barcode)
        .bind(&line.quantity)
        .bind(line.unit_price_minor)
        .bind(line.line_discount_minor)
        .bind(&tax_snapshot)
        .bind(line_tax)
        .bind(line_total)
        .bind(&line.note)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        item_summaries.push(SaleItemSummary {
            product_name: line.product_name.clone(),
            quantity: line.quantity.clone(),
            unit_price_minor: line.unit_price_minor,
            line_total_minor: line_total,
            tax_amount_minor: line_tax,
        });
    }

    let mut payment_summaries = Vec::new();
    for payment in &payments {
        if payment.amount_minor <= 0 {
            return Err(AppError::Validation(format!(
                "Payment amount must be positive (got {} for method '{}')",
                payment.amount_minor, payment.method
            )));
        }
        let payment_id = Ulid::new().to_string();
        let change = if payment.method == "cash" {
            let tendered = payment.tendered_minor.unwrap_or(payment.amount_minor);
            if tendered < payment.amount_minor {
                return Err(AppError::Validation(format!(
                    "Cash tendered ({}) is less than payment amount ({})",
                    tendered, payment.amount_minor
                )));
            }
            Some(tendered - payment.amount_minor)
        } else {
            None
        };

        sqlx::query(
            "INSERT INTO payments
             (payment_id, sale_id, origin_device_id, payment_method, amount_minor, currency, status,
              external_reference, tendered_minor, change_minor, recorded_by_user_id, recorded_at,
              created_at, updated_at)
             VALUES (?,?,?,?,?,?,'approved',?,?,?,?,?,?,?)"
        )
        .bind(&payment_id).bind(&sale_id).bind(&cart.device_id).bind(&payment.method).bind(payment.amount_minor)
        .bind(&currency).bind(&payment.external_reference).bind(payment.tendered_minor)
        .bind(change).bind(&cart.cashier_user_id).bind(&now).bind(&now).bind(&now)
        .execute(&mut *tx)
        .await?;

        payment_summaries.push(PaymentSummary {
            method: payment.method.clone(),
            amount_minor: payment.amount_minor,
            change_minor: change,
        });
    }

    // ── Atomic stock deduction (within transaction) ───────────────────────────
    // By deducting inside the transaction, we prevent concurrent sales from
    // overselling: if two transactions compete, one will serialize behind the
    // other. The UPDATE's WHERE clause enforces qty >= sold; rows_affected==0
    // means insufficient stock and the whole transaction is rolled back.
    //
    // Pre-batch: collect all product_ids that need tracking in ONE query instead
    // of one SELECT per line (eliminates the N+1 track_inventory check).
    let line_product_ids: Vec<&str> = cart
        .lines
        .iter()
        .filter(|l| !l.voided)
        .filter_map(|l| l.product_id.as_deref())
        .collect();

    let tracked_ids: std::collections::HashSet<String> = if line_product_ids.is_empty() {
        std::collections::HashSet::new()
    } else {
        let placeholders = line_product_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT product_id FROM products WHERE track_inventory = 1 AND product_id IN ({placeholders})"
        );
        let mut q = sqlx::query_scalar::<_, String>(&sql);
        for pid in &line_product_ids {
            q = q.bind(*pid);
        }
        q.fetch_all(&mut *tx).await?.into_iter().collect()
    };

    // Capture post-deduction quantities inside the transaction so movement
    // records (written post-commit) reflect the exact state after this sale.
    let mut captured_qtys: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();

    for line in cart.lines.iter().filter(|l| !l.voided) {
        let product_id = match &line.product_id {
            Some(id) => id,
            None => continue, // custom items have no product_id — skip stock
        };

        let sold_qty: f64 = line.quantity.parse().unwrap_or(0.0);
        if sold_qty <= 0.0 {
            continue;
        }

        // Use pre-fetched set — no DB round-trip per line.
        if !tracked_ids.contains(product_id.as_str()) {
            continue;
        }

        // Deduct stock atomically within the transaction.
        // When allow_negative_stock is OFF: the WHERE clause requires qty >= sold,
        // so an under-stocked item causes 0 rows_affected and we return an error.
        // When allow_negative_stock is ON: we drop the qty guard so stock can go
        // negative (back-order / temporary oversell scenario).
        let rows = if allow_negative_stock {
            sqlx::query(
                "UPDATE stock_levels
                 SET quantity_on_hand = CAST(CAST(quantity_on_hand AS REAL) - ? AS TEXT),
                     last_movement_at = ?,
                     updated_at       = ?,
                     sync_status      = 'pending'
                 WHERE product_id = ? AND branch_id = ?",
            )
            .bind(sold_qty)
            .bind(&now)
            .bind(&now)
            .bind(product_id)
            .bind(&cart.branch_id)
            .execute(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "UPDATE stock_levels
                 SET quantity_on_hand = CAST(CAST(quantity_on_hand AS REAL) - ? AS TEXT),
                     last_movement_at = ?,
                     updated_at       = ?,
                     sync_status      = 'pending'
                 WHERE product_id = ? AND branch_id = ?
                   AND CAST(quantity_on_hand AS REAL) >= ?",
            )
            .bind(sold_qty)
            .bind(&now)
            .bind(&now)
            .bind(product_id)
            .bind(&cart.branch_id)
            .bind(sold_qty)
            .execute(&mut *tx)
            .await?
        };

        if rows.rows_affected() == 0 {
            // rows_affected == 0 means either:
            //   (a) no stock_level row for this product+branch yet (uninitialized) — allow
            //   (b) allow_negative_stock is OFF and qty was insufficient — block
            let exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM stock_levels WHERE product_id = ? AND branch_id = ?",
            )
            .bind(product_id)
            .bind(&cart.branch_id)
            .fetch_optional(&mut *tx)
            .await?;

            if exists.is_some() && !allow_negative_stock {
                return Err(AppError::Validation(format!(
                    "Insufficient stock for '{}'. Please check inventory levels.",
                    line.product_name
                )));
            }
            // No stock record yet (uninitialized product) → allow through.
        }

        // Capture post-deduction quantity while still inside the sale tx
        // so movement records written post-commit use the exact value.
        if tracked_ids.contains(product_id.as_str()) {
            let qty_after: Option<String> = sqlx::query_scalar(
                "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
            )
            .bind(product_id)
            .bind(&cart.branch_id)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
            if let Some(qstr) = qty_after {
                captured_qtys.insert(product_id.clone(), qstr.parse().unwrap_or(0.0));
            }
        }
    }

    // Audit log with SHA-256 hash chain
    let audit_id = Ulid::new().to_string();
    let after_json = serde_json::json!({ "sale_id": &sale_id, "net_total_minor": net }).to_string();
    let prev_hash: String = sqlx::query_scalar(
        "SELECT hash FROM audit_logs
         WHERE device_id = ? AND length(hash) = 64
         ORDER BY created_at DESC, audit_log_id DESC
         LIMIT 1",
    )
    .bind(&cart.device_id)
    .fetch_optional(&mut *tx)
    .await
    .unwrap_or(None)
    .flatten()
    .unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &audit_id,
        event_type: "sale.created",
        entity_type: "sale",
        entity_id: &sale_id,
        actor_user_id: &cart.cashier_user_id,
        actor_type: "user",
        created_at: &now,
        before_json: None,
        after_json: Some(&after_json),
        reason: None,
        previous_hash: &prev_hash,
    });
    sqlx::query(
        "INSERT INTO audit_logs
         (audit_log_id, event_type, entity_type, entity_id, actor_user_id, actor_type,
          device_id, origin_device_id, branch_id, after_json, created_at, hash, previous_hash)
         VALUES (?,'sale.created','sale',?,?,'user',?,?,?,?,?,?,?)",
    )
    .bind(&audit_id)
    .bind(&sale_id)
    .bind(&cart.cashier_user_id)
    .bind(&cart.device_id)
    .bind(&cart.device_id)
    .bind(&cart.branch_id)
    .bind(&after_json)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash.clone())
    })
    .execute(&mut *tx)
    .await?;

    // ── Optional: create delivery order in same transaction ──────────────────
    let delivery_row = if let Some(ref d_input) = delivery {
        let row = delivery_repo::create_delivery_in_tx(
            &mut *tx,
            &sale_id,
            &receipt_number,
            net,
            &currency,
            d_input,
            &cart.cashier_user_id,
            &cart.branch_id,
            &cart.device_id,
            &now,
        )
        .await?;
        Some(row)
    } else {
        None
    };

    tx.commit().await?;
    tracing::info!("Sale finalized: {} ({})", sale_id, receipt_number);

    // Add loyalty points: floor(net_total / 1000) — best-effort, non-fatal
    if let Some(cid) = customer_id {
        let points = net / 1000;
        if points > 0 {
            if let Err(e) = sqlx::query(
                "UPDATE customers SET loyalty_points = loyalty_points + ?, updated_at = ?, sync_status = 'pending' WHERE customer_id = ?",
            )
            .bind(points)
            .bind(&now)
            .bind(cid)
            .execute(pool)
            .await
            {
                tracing::warn!("T06: loyalty update failed for customer {}: {:?}", cid, e);
            }
        }
    }

    // Deduct inventory (after commit; failures don't roll back sale).
    // branch_id and device_id come from the cart — always the real active values.
    let low_stock_alerts = movements::deduct_sale(
        pool,
        &sale_id,
        &cart.cashier_user_id,
        &cart.branch_id,
        &cart.device_id,
        Some(&captured_qtys),
    )
    .await
    .inspect_err(|e| {
        tracing::error!(
            "T06: deduct_sale movement records failed for sale {}: {e}. \
             Stock levels were already updated in the sale transaction.",
            sale_id
        );
    })
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
        created_offline,
        low_stock_alerts,
        delivery: delivery_row,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Integration tests — run with `cargo test -p zanpos-tauri`
// Uses an in-memory SQLite DB seeded by the real migration chain.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::cart::{Cart, CartLine};
    use sqlx::sqlite::SqlitePoolOptions;

    // Seed IDs that match 0001_initial.sql
    const BRANCH: &str = "01JBRANCH0000000000000001";
    const DEVICE: &str = "01JDEVICE0000000000000001";
    const CASHIER: &str = "01JUSER000000000000CASH01";
    const TAX_VAT: &str = "01JTAX000000000000VAT001"; // 10% exclusive (1 000 bp)
    const TAX_ZER: &str = "01JTAX000000000000ZERO01"; // 0%

    // Build an in-memory pool and run all migrations.
    async fn make_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");
        // 0018_remove_demo_data.sql deleted the sample products; re-seed the two
        // products this test module needs so FK constraints are satisfied.
        // Activate the seed device and branch (seeded inactive by 0011_seed.sql)
        sqlx::query("UPDATE devices SET is_active = 1 WHERE device_id = '01JDEVICE0000000000000001'")
            .execute(&pool).await.ok();
        sqlx::query("UPDATE branches SET is_active = 1 WHERE branch_id = '01JBRANCH0000000000000001'")
            .execute(&pool).await.ok();

        // Seed tax rules needed by the test products
        sqlx::query(
            "INSERT OR IGNORE INTO tax_rules (tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from, created_at, updated_at, version)
             VALUES
             ('01JTAX000000000000VAT001', 'VAT 10%', 1000, 0, 1, datetime('now'), datetime('now'), datetime('now'), 1),
             ('01JTAX000000000000ZERO01', 'Zero-rated', 0, 0, 1, datetime('now'), datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test tax rules");

        // Seed cashier user (needed by test sales)
        sqlx::query(
            "INSERT OR IGNORE INTO users (user_id, branch_id, display_name, username, pin_hash, role_id, is_active, created_at, updated_at, version)
             VALUES ('01JUSER000000000000CASH01', '01JBRANCH0000000000000001', 'Test Cashier', 'cashier_test', 'PLAIN:1234', '01JROLES000000000000000003', 1, datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test cashier");

        sqlx::query(
            "INSERT OR IGNORE INTO categories (category_id, name, sort_order, is_active, created_at, updated_at, version)
             VALUES ('01JCAT000000000000DRINK01', 'Drinks', 1, 1, datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test category");

        sqlx::query(
            "INSERT OR IGNORE INTO products
             (product_id, category_id, name, sku, barcode, description, track_inventory, allow_decimal_quantity, is_active, tax_rule_id, cost_minor, currency, reorder_point, created_at, updated_at, version)
             VALUES
             ('01JPROD00000000000COLA001', '01JCAT000000000000DRINK01', 'Coca-Cola 330ml', 'COLA-330', '5449000000996', NULL, 1, 0, 1, '01JTAX000000000000VAT001', 100, 'BHD', 0, datetime('now'), datetime('now'), 1),
             ('01JPROD00000000000WATR001', '01JCAT000000000000DRINK01', 'Water 500ml',     'WATR-500', '6281001511222', NULL, 1, 0, 1, '01JTAX000000000000ZERO01', 50,  'BHD', 0, datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test products");

        sqlx::query(
            "INSERT OR IGNORE INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, updated_at, created_at, sync_status, sync_attempts)
             VALUES
             ('SL-TEST-COLA', '01JPROD00000000000COLA001', '01JBRANCH0000000000000001', '1000', datetime('now'), datetime('now'), 'synced', 0),
             ('SL-TEST-WATR', '01JPROD00000000000WATR001', '01JBRANCH0000000000000001', '1000', datetime('now'), datetime('now'), 'synced', 0)"
        ).execute(&pool).await.expect("seed test stock");

        pool
    }

    // Insert a minimal open shift so sales FK is satisfied.
    async fn insert_shift(pool: &SqlitePool) -> String {
        let shift_id = ulid::Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO shifts (shift_id, branch_id, device_id, origin_device_id, cashier_user_id, opened_at, status, created_at, updated_at, version, sync_status, sync_attempts)
             VALUES (?, ?, ?, ?, ?, datetime('now'), 'open', datetime('now'), datetime('now'), 1, 'pending', 0)"
        )
        .bind(&shift_id).bind(BRANCH).bind(DEVICE).bind(DEVICE).bind(CASHIER)
        .execute(pool).await.expect("insert shift");
        shift_id
    }

    // Build a CartLine with explicit tax fields already calculated.
    fn cola_line(qty: &str) -> CartLine {
        // Cola: 400 minor, 10% exclusive VAT (1 000 bp)
        // Use integer arithmetic via mul_minor_by_qty — no float round-trips on money.
        let subtotal = crate::domain::money::mul_minor_by_qty(400, qty);
        let tax = subtotal * 1_000 / 10_000;
        CartLine {
            cart_line_id: ulid::Ulid::new().to_string(),
            product_id: Some("01JPROD00000000000COLA001".into()),
            product_name: "Coca-Cola 330ml".into(),
            sku: Some("COLA330".into()),
            barcode: None,
            quantity: qty.to_string(),
            unit_price_minor: 400,
            line_discount_minor: 0,
            line_discount_reason: None,
            tax_rule_id: TAX_VAT.to_string(),
            tax_rate_basis_points: 1_000,
            tax_inclusive: false,
            tax_amount_minor: tax,
            line_total_minor: subtotal + tax,
            note: None,
            voided: false,
        }
    }

    fn water_line(qty: &str) -> CartLine {
        // Water: 250 minor, zero-rated (0 bp)
        // Use integer arithmetic via mul_minor_by_qty — no float round-trips on money.
        let subtotal = crate::domain::money::mul_minor_by_qty(250, qty);
        CartLine {
            cart_line_id: ulid::Ulid::new().to_string(),
            product_id: Some("01JPROD00000000000WATR001".into()),
            product_name: "Water 500ml".into(),
            sku: Some("WATR500".into()),
            barcode: None,
            quantity: qty.to_string(),
            unit_price_minor: 250,
            line_discount_minor: 0,
            line_discount_reason: None,
            tax_rule_id: TAX_ZER.to_string(),
            tax_rate_basis_points: 0,
            tax_inclusive: false,
            tax_amount_minor: 0,
            line_total_minor: subtotal,
            note: None,
            voided: false,
        }
    }

    // ── 1. Happy path: 2× Cola, 10% VAT, cash with change ────────────────────
    // Cola 400 × 2 = 800 subtotal; tax 80; line_total 880; pay 1 000 → change 120
    #[tokio::test]
    async fn test_finalize_sale_happy_path() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        cart.lines.push(cola_line("2"));

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 880,
            tendered_minor: Some(1_000),
            external_reference: None,
        }];

        let result = finalize_sale(&pool, &cart, payments, "idem-001", None, false, None, false)
            .await
            .expect("finalize_sale");

        assert_eq!(result.net_total_minor, 880);
        assert_eq!(result.tax_total_minor, 80);
        assert_eq!(result.discount_total_minor, 0);
        assert!(!result.sale_id.is_empty());
        assert!(!result.receipt_number.is_empty());
        assert_eq!(result.payments.len(), 1);
        assert_eq!(result.payments[0].change_minor, Some(120));
        assert_eq!(result.items.len(), 1);
    }

    // ── 2. Under-payment is rejected before any DB write ─────────────────────
    #[tokio::test]
    async fn test_finalize_sale_underpay_rejected() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        cart.lines.push(cola_line("1")); // net = 440

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 400, // < 440 → underpay
            tendered_minor: Some(400),
            external_reference: None,
        }];

        let err = finalize_sale(&pool, &cart, payments, "idem-underpay", None, false, None, false)
            .await
            .unwrap_err();

        assert!(
            matches!(err, AppError::Validation(_)),
            "expected Validation error, got {err:?}"
        );

        // Confirm no sale was written
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sales")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "no sale should be persisted on underpay");
    }

    // ── 3. Duplicate idempotency key is rejected ──────────────────────────────
    #[tokio::test]
    async fn test_finalize_sale_idempotency_key_unique() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let payments = || {
            vec![PaymentInput {
                method: "cash".into(),
                amount_minor: 440,
                tendered_minor: Some(440),
                external_reference: None,
            }]
        };

        let mut cart1 = Cart::new(
            BRANCH.into(),
            DEVICE.into(),
            shift_id.clone(),
            CASHIER.into(),
        );
        cart1.lines.push(cola_line("1"));
        finalize_sale(&pool, &cart1, payments(), "idem-dup", None, false, None, false)
            .await
            .expect("first sale");

        let mut cart2 = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        cart2.lines.push(cola_line("1"));
        let err = finalize_sale(&pool, &cart2, payments(), "idem-dup", None, false, None, false)
            .await
            .unwrap_err();

        // Expect a DB error (UNIQUE constraint on idempotency_key)
        assert!(
            matches!(err, AppError::Database(_)),
            "expected Database unique-constraint error, got {err:?}"
        );
    }

    // ── 4. Zero-tax (zero-rated) item: no tax charged ─────────────────────────
    #[tokio::test]
    async fn test_finalize_sale_zero_tax_item() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        cart.lines.push(water_line("4")); // 4 × 250 = 1 000, zero-rated

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 1_000,
            tendered_minor: Some(1_000),
            external_reference: None,
        }];

        let result = finalize_sale(&pool, &cart, payments, "idem-water", None, false, None, false)
            .await
            .expect("zero-tax sale");

        assert_eq!(result.net_total_minor, 1_000);
        assert_eq!(result.tax_total_minor, 0, "zero-rated items carry no tax");
    }

    // ── 5. Line discount reduces net total ────────────────────────────────────
    #[tokio::test]
    async fn test_finalize_sale_with_line_discount() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        let mut line = cola_line("1"); // 400 minor, no tax for simplicity — override to zero-rated
        line.tax_rule_id = TAX_ZER.to_string();
        line.tax_rate_basis_points = 0;
        line.tax_amount_minor = 0;
        line.line_total_minor = 400;
        line.line_discount_minor = 100; // discount 100 minor
        line.line_total_minor = 300; // after discount
        cart.lines.push(line);

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 300,
            tendered_minor: Some(300),
            external_reference: None,
        }];

        let result = finalize_sale(&pool, &cart, payments, "idem-discount", None, false, None, false)
            .await
            .expect("discounted sale");

        assert_eq!(result.net_total_minor, 300);
        assert_eq!(result.discount_total_minor, 100);
    }

    // ── 6. Split payment (cash + card) ────────────────────────────────────────
    #[tokio::test]
    async fn test_finalize_sale_split_payment() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        // 2× Cola (880) + 2× Water (500) = 1 380 total; tax = 80
        cart.lines.push(cola_line("2"));
        cart.lines.push(water_line("2"));

        let payments = vec![
            PaymentInput {
                method: "cash".into(),
                amount_minor: 880,
                tendered_minor: Some(880),
                external_reference: None,
            },
            PaymentInput {
                method: "card".into(),
                amount_minor: 500,
                tendered_minor: None,
                external_reference: Some("TXN-999".into()),
            },
        ];

        let result = finalize_sale(&pool, &cart, payments, "idem-split", None, false, None, false)
            .await
            .expect("split-payment sale");

        assert_eq!(result.net_total_minor, 1_380);
        assert_eq!(result.payments.len(), 2);

        let cash_pay = result.payments.iter().find(|p| p.method == "cash").unwrap();
        let card_pay = result.payments.iter().find(|p| p.method == "card").unwrap();
        assert_eq!(cash_pay.change_minor, Some(0));
        assert!(card_pay.change_minor.is_none());
    }

    // ── T8. Concurrent sales for qty=1 item: only one succeeds ──────────────
    // Seeds stock_level = 1, then issues two sequential sales (simulating
    // near-concurrent access in a single-threaded test). The atomic UPDATE in
    // finalize_sale ensures the second sale cannot deplete stock below zero.
    #[tokio::test]
    async fn test_stock_prevents_oversell() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;
        let product_id = "01JPROD00000000000COLA001";
        let branch_id = BRANCH;

        // Seed exactly 1 unit in stock
        sqlx::query(
            "INSERT INTO stock_levels
             (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at)
             VALUES ('SL-COLA', ?, ?, '1', datetime('now'), datetime('now'))
             ON CONFLICT(product_id, branch_id)
             DO UPDATE SET quantity_on_hand = '1', updated_at = datetime('now')",
        )
        .bind(product_id)
        .bind(branch_id)
        .execute(&pool)
        .await
        .expect("seed stock");

        let payment_for_cola = || {
            vec![PaymentInput {
                method: "cash".into(),
                amount_minor: 440,
                tendered_minor: Some(440),
                external_reference: None,
            }]
        };

        // First sale — should succeed (stock 1 → 0)
        let mut cart1 = Cart::new(
            BRANCH.into(),
            DEVICE.into(),
            shift_id.clone(),
            CASHIER.into(),
        );
        cart1.lines.push(cola_line("1"));
        let r1 = finalize_sale(&pool, &cart1, payment_for_cola(), "idem-t8-first", None, false, None, false).await;
        assert!(r1.is_ok(), "first sale must succeed with stock=1: {r1:?}");

        // Second sale — must fail; stock is now 0
        let mut cart2 = Cart::new(
            BRANCH.into(),
            DEVICE.into(),
            shift_id.clone(),
            CASHIER.into(),
        );
        cart2.lines.push(cola_line("1"));
        let r2 = finalize_sale(&pool, &cart2, payment_for_cola(), "idem-t8-second", None, false, None, false).await;
        assert!(
            matches!(r2, Err(AppError::Validation(_))),
            "second sale must be rejected when stock=0: got {r2:?}"
        );
    }

    // ── 7. allow_negative_stock=true lets a sale proceed below zero ────────────
    #[tokio::test]
    async fn test_allow_negative_stock_sells_through_zero() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;
        let product_id = "01JPROD00000000000COLA001";
        let branch_id = BRANCH;

        // Seed stock at exactly 0 — would normally block a sale
        sqlx::query(
            "INSERT INTO stock_levels
             (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at)
             VALUES ('SL-COLA-NEG', ?, ?, '0', datetime('now'), datetime('now'))
             ON CONFLICT(product_id, branch_id)
             DO UPDATE SET quantity_on_hand = '0', updated_at = datetime('now')",
        )
        .bind(product_id)
        .bind(branch_id)
        .execute(&pool)
        .await
        .expect("seed zero stock");

        let payment = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 440,
            tendered_minor: Some(440),
            external_reference: None,
        }];

        let mut cart = Cart::new(
            BRANCH.into(),
            DEVICE.into(),
            shift_id.clone(),
            CASHIER.into(),
        );
        cart.lines.push(cola_line("1"));

        // With allow_negative_stock=true the sale must succeed even at stock=0
        let result = finalize_sale(&pool, &cart, payment, "idem-neg-stock", None, false, None, true).await;
        assert!(
            result.is_ok(),
            "sale must succeed with allow_negative_stock=true even when stock=0: {result:?}"
        );

        // Stock should now be -1
        let qty: String = sqlx::query_scalar(
            "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
        )
        .bind(product_id)
        .bind(branch_id)
        .fetch_one(&pool)
        .await
        .expect("stock row must exist");

        let qty_f: f64 = qty.parse().expect("quantity_on_hand must be numeric");
        assert!(
            qty_f < 0.0,
            "stock must be negative after oversell with flag ON, got: {qty}"
        );
    }

    // ── 8. Receipt number is sequential per device/branch ────────────────────
    #[tokio::test]
    async fn test_receipt_number_sequential() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let single_water_payment = || {
            vec![PaymentInput {
                method: "cash".into(),
                amount_minor: 250,
                tendered_minor: Some(250),
                external_reference: None,
            }]
        };

        let mut c1 = Cart::new(
            BRANCH.into(),
            DEVICE.into(),
            shift_id.clone(),
            CASHIER.into(),
        );
        c1.lines.push(water_line("1"));
        let r1 = finalize_sale(&pool, &c1, single_water_payment(), "idem-seq1", None, false, None, false)
            .await
            .unwrap();

        let mut c2 = Cart::new(
            BRANCH.into(),
            DEVICE.into(),
            shift_id.clone(),
            CASHIER.into(),
        );
        c2.lines.push(water_line("1"));
        let r2 = finalize_sale(&pool, &c2, single_water_payment(), "idem-seq2", None, false, None, false)
            .await
            .unwrap();

        // Numbers should differ and second > first (lexicographic on zero-padded counter)
        assert_ne!(r1.receipt_number, r2.receipt_number);
        assert!(
            r2.receipt_number > r1.receipt_number,
            "{} > {}",
            r2.receipt_number,
            r1.receipt_number
        );
    }
}
