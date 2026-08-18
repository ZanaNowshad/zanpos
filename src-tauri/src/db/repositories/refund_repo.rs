use crate::db::repositories::audit_hash;
use crate::domain::refund::{RefundItemInput, RefundResult, SaleForRefund, SaleItemForRefund};
use crate::domain::sale::{PaymentSummary, SaleItemSummary, SaleResult};
use crate::errors::{AppError, AppResult};
use crate::inventory::movements;
use sqlx::{Row, SqlitePool};
use ulid::Ulid;

pub async fn get_sale_by_receipt(
    pool: &SqlitePool,
    receipt_number: &str,
) -> AppResult<SaleForRefund> {
    // Receipt numbers are formatted as "{branch_code}-{device_code}-{seq:08}" (see sale_repo::next_receipt_number).
    // The branch_code + device_code prefix makes them globally unique across all terminals in the system,
    // so a lookup by receipt_number alone is safe and will never return the wrong sale.
    // No additional branch_id / device_id filter is required here.
    let sale_row = sqlx::query(
        "SELECT s.sale_id, s.receipt_number, s.net_total_minor, s.currency, s.sold_at, s.status,
                s.origin_device_id,
                u.display_name as cashier_name
         FROM sales s JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.receipt_number = ?",
    )
    .bind(receipt_number)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Receipt {} not found", receipt_number)))?;

    let sale_id: String = sale_row.get("sale_id");

    let item_rows = sqlx::query(
        "SELECT sale_item_id, product_name_snapshot, quantity, unit_price_minor, line_total_minor
         FROM sale_items WHERE sale_id = ? AND voided = 0",
    )
    .bind(&sale_id)
    .fetch_all(pool)
    .await?;

    let items = item_rows
        .iter()
        .map(|r| SaleItemForRefund {
            sale_item_id: r.get("sale_item_id"),
            product_name_snapshot: r.get("product_name_snapshot"),
            quantity: r.get("quantity"),
            unit_price_minor: r.get("unit_price_minor"),
            line_total_minor: r.get("line_total_minor"),
        })
        .collect();

    Ok(SaleForRefund {
        sale_id,
        receipt_number: sale_row.get("receipt_number"),
        net_total_minor: sale_row.get("net_total_minor"),
        currency: sale_row.get("currency"),
        sold_at: sale_row.get("sold_at"),
        cashier_name: sale_row.get("cashier_name"),
        status: sale_row.get("status"),
        origin_device_id: sale_row.get("origin_device_id"),
        items,
    })
}

pub async fn get_sale_result_by_receipt(
    pool: &SqlitePool,
    receipt_number: &str,
) -> AppResult<SaleResult> {
    let sale_row = sqlx::query(
        "SELECT s.sale_id, s.receipt_number, s.net_total_minor, s.tax_total_minor,
                s.discount_total_minor, s.currency, s.sold_at, s.business_date, s.created_offline,
                u.display_name as cashier_name, b.name as branch_name
         FROM sales s
         JOIN users u ON u.user_id = s.cashier_user_id
         JOIN branches b ON b.branch_id = s.branch_id
         WHERE s.receipt_number = ?",
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

    let items = item_rows
        .iter()
        .map(|r| SaleItemSummary {
            product_name: r.get("product_name_snapshot"),
            quantity: r.get("quantity"),
            unit_price_minor: r.get("unit_price_minor"),
            line_total_minor: r.get("line_total_minor"),
            tax_amount_minor: r.get("tax_amount_minor"),
        })
        .collect();

    let payment_rows = sqlx::query(
        "SELECT payment_method, amount_minor, change_minor FROM payments WHERE sale_id = ?",
    )
    .bind(&sale_id)
    .fetch_all(pool)
    .await?;

    let payments = payment_rows
        .iter()
        .map(|r| PaymentSummary {
            method: r.get("payment_method"),
            amount_minor: r.get("amount_minor"),
            change_minor: r.get("change_minor"),
        })
        .collect();

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
        low_stock_alerts: vec![],
        delivery: None,
    })
}

pub async fn create_refund(
    pool: &SqlitePool,
    original_sale_id: &str,
    items: Vec<RefundItemInput>,
    reason: &str,
    reason_code: &str,
    created_by_user_id: &str,
    override_used: bool,
) -> AppResult<RefundResult> {
    if items.is_empty() {
        return Err(AppError::Validation("No items selected for refund".into()));
    }

    // Acquire a single connection and start with BEGIN IMMEDIATE.
    // IMMEDIATE acquires the SQLite write lock upfront, serialising concurrent
    // refund attempts on the same sale — preventing double-refund races.
    let mut conn = pool.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;

    // Inline rollback helper (called on any early-return error path).
    macro_rules! abort {
        ($e:expr) => {{
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err($e);
        }};
    }

    // ── Guard: original sale must exist and must not be voided ────────────────
    let sale_status_row = sqlx::query(
        "SELECT s.status, s.currency, s.device_id, s.branch_id,
                b.branch_code, d.device_code
         FROM sales s
         JOIN branches b ON b.branch_id = s.branch_id
         JOIN devices d  ON d.device_id  = s.device_id
         WHERE s.sale_id = ?",
    )
    .bind(original_sale_id)
    .fetch_optional(&mut *conn)
    .await;

    let sale_row = match sale_status_row {
        Ok(Some(r)) => r,
        Ok(None) => abort!(AppError::NotFound("Original sale not found".into())),
        Err(e) => abort!(AppError::Database(e)),
    };

    let sale_status: String = sale_row.get("status");
    if sale_status == "voided" {
        abort!(AppError::Validation("Cannot refund a voided sale".into()));
    }

    // ── Guard: per-item ceiling + double-refund prevention (atomic) ───────────
    // Uses the `refunded_amount_minor` column added in migration 0013.
    // The UPDATE is a single atomic SQL statement: if already_refunded + new_amount
    // would exceed line_total_minor, the WHERE clause fails and rows_affected == 0.
    for item in &items {
        if item.refund_amount_minor <= 0 {
            abort!(AppError::Validation(format!(
                "Refund amount must be positive for '{}'",
                item.product_name_snapshot
            )));
        }

        // Validate quantity string so return_refund stock credit receives
        // a sane value and garbage strings don't silently zero out stock.
        let _refund_qty: f64 = match item.quantity.parse::<f64>() {
            Ok(q) if q > 0.0 => q,
            _ => abort!(AppError::Validation(format!(
                "Invalid refund quantity '{}' for '{}'",
                item.quantity, item.product_name_snapshot
            ))),
        };

        let rows = sqlx::query(
            "UPDATE sale_items
             SET refunded_amount_minor = refunded_amount_minor + ?
             WHERE sale_item_id = ? AND sale_id = ?
               AND (refunded_amount_minor + ?) <= line_total_minor",
        )
        .bind(item.refund_amount_minor)
        .bind(&item.sale_item_id)
        .bind(original_sale_id)
        .bind(item.refund_amount_minor)
        .execute(&mut *conn)
        .await;

        let affected = match rows {
            Ok(r) => r.rows_affected(),
            Err(e) => abort!(AppError::Database(e)),
        };

        if affected == 0 {
            // Either item not found on this sale, or refund would exceed remaining balance.
            // Check which case to give a useful error.
            let exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM sale_items WHERE sale_item_id = ? AND sale_id = ?",
            )
            .bind(&item.sale_item_id)
            .bind(original_sale_id)
            .fetch_optional(&mut *conn)
            .await
            .unwrap_or(None);

            if exists.is_none() {
                abort!(AppError::NotFound(format!(
                    "Sale item '{}' not found on sale {}",
                    item.product_name_snapshot, original_sale_id
                )));
            } else {
                abort!(AppError::Validation(format!(
                    "Refund amount for '{}' exceeds remaining refundable balance",
                    item.product_name_snapshot
                )));
            }
        }
    }

    let sale_row = sale_row; // already fetched above

    let currency: String = sale_row.get("currency");
    let device_id: String = sale_row.get("device_id");
    let branch_id: String = sale_row.get("branch_id");
    let branch_code: String = sale_row.get("branch_code");
    let device_code: String = sale_row.get("device_code");

    let refund_total: i64 = items.iter().map(|i| i.refund_amount_minor).sum();
    let refund_id = Ulid::new().to_string();

    // Atomic per-device counter — shared with sales so receipt numbers are a single
    // device-scoped sequence. UPDATE...RETURNING prevents races.
    let seq: i64 = match sqlx::query_scalar(
        "UPDATE devices SET next_receipt_seq = next_receipt_seq + 1
         WHERE device_id = ?
         RETURNING next_receipt_seq",
    )
    .bind(&device_id)
    .fetch_one(&mut *conn)
    .await
    {
        Ok(s) => s,
        Err(e) => abort!(AppError::Database(e)),
    };
    let refund_receipt_number = format!("{}-{}-{:08}", branch_code, device_code, seq);

    let now = chrono::Utc::now().to_rfc3339();
    let idempotency_key = format!("refund-{}", refund_id);

    // Validate reason_code is one of the accepted values; fall back to 'other'
    let safe_reason_code = match reason_code {
        "customer_return" | "defective" | "wrong_item" | "exchange" | "other" => reason_code,
        _ => "other",
    };

    if let Err(e) = sqlx::query(
        "INSERT INTO refunds (refund_id, original_sale_id, origin_device_id, refund_receipt_number, reason,
         return_reason_code, refund_total_minor, currency, created_by_user_id, idempotency_key,
         created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&refund_id)
    .bind(original_sale_id)
    .bind(&device_id)
    .bind(&refund_receipt_number)
    .bind(reason)
    .bind(safe_reason_code)
    .bind(refund_total)
    .bind(&currency)
    .bind(created_by_user_id)
    .bind(&idempotency_key)
    .bind(&now)
    .bind(&now)
    .execute(&mut *conn)
    .await
    {
        abort!(AppError::Database(e));
    }

    for item in &items {
        let refund_item_id = Ulid::new().to_string();
        if let Err(e) = sqlx::query(
            "INSERT INTO refund_items
             (refund_item_id, refund_id, origin_device_id, sale_item_id, product_name_snapshot,
              quantity, unit_price_minor, refund_amount_minor, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&refund_item_id)
        .bind(&refund_id)
        .bind(&device_id)
        .bind(&item.sale_item_id)
        .bind(&item.product_name_snapshot)
        .bind(&item.quantity)
        .bind(item.unit_price_minor)
        .bind(item.refund_amount_minor)
        .bind(&now)
        .bind(&now)
        .execute(&mut *conn)
        .await
        {
            abort!(AppError::Database(e));
        }
    }

    // Determine if all non-voided sale items are now fully refunded.
    let unreffunded_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sale_items
         WHERE sale_id = ? AND voided = 0
           AND refunded_amount_minor < line_total_minor",
    )
    .bind(original_sale_id)
    .fetch_one(&mut *conn)
    .await
    .unwrap_or(1); // default to 1 (not fully refunded) on error

    let new_status = if unreffunded_count == 0 {
        "refunded"
    } else {
        "partially_refunded"
    };

    if let Err(e) = sqlx::query(
        "UPDATE sales SET status = ?, updated_at = ?, sync_status = 'pending'
         WHERE sale_id = ? AND status IN ('completed', 'partially_refunded')",
    )
    .bind(new_status)
    .bind(&now)
    .bind(original_sale_id)
    .execute(&mut *conn)
    .await
    {
        abort!(AppError::Database(e));
    }

    let audit_id = Ulid::new().to_string();
    let after_json =
        serde_json::json!({ "refund_id": &refund_id, "total_minor": refund_total }).to_string();
    // Fetch the latest hash from within the open transaction so the chain
    // is consistent with any audit rows we are about to insert.
    let prev_hash: String = sqlx::query_scalar(
        "SELECT hash FROM audit_logs
         WHERE device_id = ? AND length(hash) = 64
         ORDER BY created_at DESC, audit_log_id DESC
         LIMIT 1",
    )
    .bind(&device_id)
    .fetch_optional(&mut *conn)
    .await
    .unwrap_or(None)
    .flatten()
    .unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &audit_id,
        event_type: "refund.created",
        entity_type: "refund",
        entity_id: &refund_id,
        actor_user_id: created_by_user_id,
        actor_type: "user",
        created_at: &now,
        before_json: None,
        after_json: Some(&after_json),
        reason: None,
        previous_hash: &prev_hash,
    });
    if let Err(e) = sqlx::query(
        "INSERT INTO audit_logs (audit_log_id, event_type, entity_type, entity_id,
         actor_user_id, actor_type, device_id, origin_device_id, branch_id, after_json, created_at, hash, previous_hash, override_used)
         VALUES (?, 'refund.created', 'refund', ?, ?, 'user', ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&audit_id)
    .bind(&refund_id)
    .bind(created_by_user_id)
    .bind(&device_id)
    .bind(&device_id)
    .bind(&branch_id)
    .bind(&after_json)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash.clone())
    })
    .bind(override_used as i64)
    .execute(&mut *conn)
    .await
    {
        abort!(AppError::Database(e));
    }

    if let Err(e) = sqlx::query("COMMIT").execute(&mut *conn).await {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(AppError::Database(e));
    }
    tracing::info!("Refund created: {} ({})", refund_id, refund_receipt_number);

    // Fetch the original sale's branch_id and device_id for inventory and sync.
    // We do this once and reuse for both stock return and sync tracking.
    let sale_meta = sqlx::query("SELECT device_id, branch_id FROM sales WHERE sale_id = ?")
        .bind(original_sale_id)
        .fetch_optional(pool)
        .await;

    // Return stock for refunded items (after commit; stock credit failure does NOT
    // roll back the already-committed refund — the refund is accepted regardless).
    // Errors are logged so operators can reconcile manually if needed.
    // Pass real branch/device from the original sale so movements carry correct identity.
    if let Ok(Some(ref meta)) = sale_meta {
        let device_id: String = meta.get("device_id");
        let branch_id_str: String = meta.get("branch_id");
        if let Err(e) = movements::return_refund(
            pool,
            &refund_id,
            created_by_user_id,
            &branch_id_str,
            &device_id,
        )
        .await
        {
            // Stock credit failed — refund is still valid; stock may need manual correction.
            tracing::error!(
                refund_id = %refund_id,
                sale_id = %original_sale_id,
                "return_refund: stock credit failed after refund commit — manual reconciliation may be required: {e}"
            );
        }
    } else {
        // Sale meta unavailable — stock credit skipped; log for manual reconciliation.
        tracing::error!(
            refund_id = %refund_id,
            sale_id = %original_sale_id,
            "return_refund: could not resolve branch/device — stock NOT credited for refund"
        );
    }

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    Ok(RefundResult {
        refund_id,
        refund_receipt_number,
        refund_total_minor: refund_total,
        currency,
        created_at: now,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Integration tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::sale_repo;
    use crate::domain::cart::{Cart, CartLine};
    use crate::domain::sale::PaymentInput;
    use sqlx::sqlite::SqlitePoolOptions;

    const BRANCH: &str = "01JBRANCH0000000000000001";
    const DEVICE: &str = "01JDEVICE0000000000000001";
    const CASHIER: &str = "01JUSER000000000000CASH01";
    const COLA_ID: &str = "01JPROD00000000000COLA001";
    const TAX_VAT: &str = "01JTAX000000000000VAT001";

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
        // Activate seed device and branch
        sqlx::query(
            "UPDATE devices SET is_active = 1 WHERE device_id = '01JDEVICE0000000000000001'",
        )
        .execute(&pool)
        .await
        .ok();
        sqlx::query(
            "UPDATE branches SET is_active = 1 WHERE branch_id = '01JBRANCH0000000000000001'",
        )
        .execute(&pool)
        .await
        .ok();

        // Seed tax rules needed by the test product
        sqlx::query(
            "INSERT OR IGNORE INTO tax_rules (tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from, created_at, updated_at, version)
             VALUES ('01JTAX000000000000VAT001', 'VAT 10%', 1000, 0, 1, datetime('now'), datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test tax rule");

        // Seed cashier user
        sqlx::query(
            "INSERT OR IGNORE INTO users (user_id, branch_id, display_name, username, pin_hash, role_id, is_active, created_at, updated_at, version)
             VALUES ('01JUSER000000000000CASH01', '01JBRANCH0000000000000001', 'Test Cashier', 'cashier_test', 'PLAIN:1234', '01JROLES000000000000000003', 1, datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test cashier");

        // Product this test module needs
        sqlx::query(
            "INSERT OR IGNORE INTO categories (category_id, name, sort_order, is_active, created_at, updated_at, version)
             VALUES ('01JCAT000000000000DRINK01', 'Drinks', 1, 1, datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test category");

        sqlx::query(
            "INSERT OR IGNORE INTO products
             (product_id, category_id, name, sku, barcode, description, track_inventory, is_active, tax_rule_id, currency, created_at, updated_at, version)
             VALUES ('01JPROD00000000000COLA001', '01JCAT000000000000DRINK01', 'Coca-Cola 330ml', 'COLA-330', '5449000000996', NULL, 1, 1, '01JTAX000000000000VAT001', 'BHD', datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test product");

        sqlx::query(
            "INSERT OR IGNORE INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, updated_at, created_at, sync_status, sync_attempts)
             VALUES ('SL-TEST-COLA', '01JPROD00000000000COLA001', '01JBRANCH0000000000000001', '1000', datetime('now'), datetime('now'), 'synced', 0)"
        ).execute(&pool).await.expect("seed test stock");

        pool
    }

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

    /// Create a real sale and return (sale_id, sale_item_id) of the first item.
    async fn create_test_sale(pool: &SqlitePool, shift_id: &str, key: &str) -> (String, String) {
        let qty_f = 2.0_f64;
        let subtotal = (400.0 * qty_f) as i64;
        let tax = subtotal * 1_000 / 10_000;
        let line = CartLine {
            cart_line_id: ulid::Ulid::new().to_string(),
            product_id: Some(COLA_ID.into()),
            product_name: "Coca-Cola 330ml".into(),
            sku: Some("COLA330".into()),
            barcode: None,
            image_path: None,
            quantity: "2".to_string(),
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
        };

        let mut cart = Cart::new(
            BRANCH.into(),
            DEVICE.into(),
            shift_id.into(),
            CASHIER.into(),
        );
        cart.lines.push(line);

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: subtotal + tax,
            tendered_minor: Some(subtotal + tax),
            external_reference: None,
        }];

        let result = sale_repo::finalize_sale(pool, &cart, payments, key, None, false, None, false)
            .await
            .expect("finalize_sale in test setup");

        // Retrieve the sale_item_id for the inserted item
        let item_id: String =
            sqlx::query_scalar("SELECT sale_item_id FROM sale_items WHERE sale_id = ? LIMIT 1")
                .bind(&result.sale_id)
                .fetch_one(pool)
                .await
                .expect("fetch sale_item_id");

        (result.sale_id, item_id)
    }

    // ── 1. Full refund of a sale succeeds and returns correct total ───────────
    #[tokio::test]
    async fn test_create_refund_happy_path() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;
        let (sale_id, item_id) = create_test_sale(&pool, &shift_id, "idem-refund-1").await;

        // Refund the full 2× Cola at 880 minor
        let items = vec![RefundItemInput {
            sale_item_id: item_id,
            product_name_snapshot: "Coca-Cola 330ml".into(),
            quantity: "2".to_string(),
            unit_price_minor: 440, // (400 + 40 tax) per unit
            refund_amount_minor: 880,
        }];

        let result = create_refund(
            &pool,
            &sale_id,
            items,
            "Defective product",
            "defective",
            CASHIER,
            false,
        )
        .await
        .expect("create_refund");

        assert_eq!(result.refund_total_minor, 880);
        assert!(!result.refund_receipt_number.is_empty());
        assert!(
            result.refund_receipt_number.starts_with("MAIN-POS01-"),
            "receipt must be device-scoped: {{branch}}-{{device}}-{{seq}}"
        );
    }

    // ── 2. Refund with empty items is rejected ────────────────────────────────
    #[tokio::test]
    async fn test_create_refund_empty_items_rejected() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;
        let (sale_id, _) = create_test_sale(&pool, &shift_id, "idem-refund-empty").await;

        let err = create_refund(&pool, &sale_id, vec![], "test", "other", CASHIER, false)
            .await
            .unwrap_err();

        assert!(
            matches!(err, AppError::Validation(_)),
            "empty items must fail validation"
        );
    }

    // ── 3. Refund of a non-existent sale returns NotFound ─────────────────────
    #[tokio::test]
    async fn test_create_refund_unknown_sale() {
        let pool = make_pool().await;

        let items = vec![RefundItemInput {
            sale_item_id: "fake-item".into(),
            product_name_snapshot: "Ghost Product".into(),
            quantity: "1".to_string(),
            unit_price_minor: 100,
            refund_amount_minor: 100,
        }];

        let err = create_refund(
            &pool,
            "FAKE-SALE-ID",
            items,
            "test",
            "other",
            CASHIER,
            false,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, AppError::NotFound(_)),
            "unknown sale must return NotFound"
        );
    }

    // ── 4. Refund receipt number is sequential ────────────────────────────────
    #[tokio::test]
    async fn test_refund_receipt_sequential() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;
        let (sale_id, item_id) = create_test_sale(&pool, &shift_id, "idem-refund-seq").await;

        let make_item = || RefundItemInput {
            sale_item_id: item_id.clone(),
            product_name_snapshot: "Coca-Cola 330ml".into(),
            quantity: "1".to_string(),
            unit_price_minor: 440,
            refund_amount_minor: 440,
        };

        // Create two refunds on the same sale (partial refunds)
        let r1 = create_refund(
            &pool,
            &sale_id,
            vec![make_item()],
            "reason 1",
            "other",
            CASHIER,
            false,
        )
        .await
        .expect("refund 1");
        let r2 = create_refund(
            &pool,
            &sale_id,
            vec![make_item()],
            "reason 2",
            "other",
            CASHIER,
            false,
        )
        .await
        .expect("refund 2");

        assert_ne!(r1.refund_receipt_number, r2.refund_receipt_number);
        // Second refund number should be higher (sequential counter)
        assert!(r2.refund_receipt_number > r1.refund_receipt_number);
    }

    // ── 5. Partial refund marks sale as partially_refunded ────────────────────
    #[tokio::test]
    async fn test_partial_refund_updates_sale_status() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;
        let (sale_id, item_id) = create_test_sale(&pool, &shift_id, "idem-refund-status").await;

        let items = vec![RefundItemInput {
            sale_item_id: item_id,
            product_name_snapshot: "Coca-Cola 330ml".into(),
            quantity: "1".to_string(),
            unit_price_minor: 440,
            refund_amount_minor: 440,
        }];

        create_refund(
            &pool,
            &sale_id,
            items,
            "partial return",
            "customer_return",
            CASHIER,
            false,
        )
        .await
        .expect("partial refund");

        let status: String = sqlx::query_scalar("SELECT status FROM sales WHERE sale_id = ?")
            .bind(&sale_id)
            .fetch_one(&pool)
            .await
            .expect("fetch sale status");

        assert_eq!(status, "partially_refunded");
    }

    // ── T7. Refund amount > original line total is rejected ───────────────────
    #[tokio::test]
    async fn test_refund_ceiling_enforced() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;
        let (sale_id, item_id) = create_test_sale(&pool, &shift_id, "idem-t7-ceiling").await;

        // The line_total_minor for 2× Cola at 440 each = 880.
        // Attempt to refund 999 — exceeds ceiling.
        let items = vec![RefundItemInput {
            sale_item_id: item_id,
            product_name_snapshot: "Coca-Cola 330ml".into(),
            quantity: "2".to_string(),
            unit_price_minor: 440,
            refund_amount_minor: 999, // > 880 line_total
        }];

        let err = create_refund(
            &pool,
            &sale_id,
            items,
            "test ceiling",
            "other",
            CASHIER,
            false,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, AppError::Validation(_)),
            "refund > original line total must be rejected: got {err:?}"
        );
    }

    // ── T9. Refund on a voided sale is rejected ───────────────────────────────
    #[tokio::test]
    async fn test_refund_on_voided_sale_rejected() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;
        let (sale_id, item_id) = create_test_sale(&pool, &shift_id, "idem-t9-void").await;

        // Void the sale first
        sqlx::query("UPDATE sales SET status = 'voided' WHERE sale_id = ?")
            .bind(&sale_id)
            .execute(&pool)
            .await
            .expect("void sale");

        let items = vec![RefundItemInput {
            sale_item_id: item_id,
            product_name_snapshot: "Coca-Cola 330ml".into(),
            quantity: "1".to_string(),
            unit_price_minor: 440,
            refund_amount_minor: 440,
        }];

        let err = create_refund(
            &pool,
            &sale_id,
            items,
            "refund void",
            "other",
            CASHIER,
            false,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, AppError::Validation(_)),
            "refund on voided sale must be rejected: got {err:?}"
        );
    }

    // ── T7b. Double refund on same item is rejected ───────────────────────────
    #[tokio::test]
    async fn test_double_refund_prevented() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;
        let (sale_id, item_id) = create_test_sale(&pool, &shift_id, "idem-t7b-double").await;

        let make_item = || RefundItemInput {
            sale_item_id: item_id.clone(),
            product_name_snapshot: "Coca-Cola 330ml".into(),
            quantity: "2".to_string(),
            unit_price_minor: 440,
            refund_amount_minor: 880, // full refund
        };

        // First full refund must succeed
        create_refund(
            &pool,
            &sale_id,
            vec![make_item()],
            "first",
            "other",
            CASHIER,
            false,
        )
        .await
        .expect("first refund should succeed");

        // Second full refund must fail — item fully refunded
        let err = create_refund(
            &pool,
            &sale_id,
            vec![make_item()],
            "second",
            "other",
            CASHIER,
            false,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(err, AppError::Validation(_)),
            "double refund must be rejected: got {err:?}"
        );
    }

    // ── 6. Unknown reason_code defaults to 'other' ────────────────────────────
    #[tokio::test]
    async fn test_unknown_reason_code_defaults_to_other() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;
        let (sale_id, item_id) = create_test_sale(&pool, &shift_id, "idem-refund-code").await;

        let items = vec![RefundItemInput {
            sale_item_id: item_id,
            product_name_snapshot: "Coca-Cola 330ml".into(),
            quantity: "1".to_string(),
            unit_price_minor: 440,
            refund_amount_minor: 440,
        }];

        create_refund(
            &pool,
            &sale_id,
            items,
            "reason",
            "INVALID_CODE",
            CASHIER,
            false,
        )
        .await
        .expect("refund with invalid code");

        let code: String = sqlx::query_scalar(
            "SELECT return_reason_code FROM refunds WHERE original_sale_id = ? LIMIT 1",
        )
        .bind(&sale_id)
        .fetch_one(&pool)
        .await
        .expect("fetch reason_code");

        assert_eq!(
            code, "other",
            "invalid reason_code must be normalised to 'other'"
        );
    }
}
