use crate::commands::rbac;
use crate::db::repositories::{audit_hash, product_repo, sale_repo};
use crate::domain::cart::{Cart, CartLine};
use crate::domain::delivery::DeliveryInput;
use crate::domain::sale::{PaymentInput, SaleResult};
use crate::errors::AppError;
use crate::inventory::movements;
use crate::AppState;
use sqlx::Row;
use tauri::State;
use ulid::Ulid;

#[derive(serde::Deserialize)]
pub struct StartCartInput {
    pub branch_id: String,
    pub device_id: String,
    pub shift_id: String,
    pub cashier_user_id: String,
}

#[derive(serde::Serialize)]
pub struct StartCartResult {
    pub cart: Cart,
}

#[tauri::command]
pub async fn pos_start_cart(
    input: StartCartInput,
    _state: State<'_, AppState>,
) -> Result<StartCartResult, AppError> {
    let cart = Cart::new(
        input.branch_id,
        input.device_id,
        input.shift_id,
        input.cashier_user_id,
    );
    Ok(StartCartResult { cart })
}

#[derive(serde::Deserialize)]
pub struct AddItemInput {
    pub cart: Cart,
    pub product_id: String,
    pub quantity: Option<String>,
}

/// Format a quantity float cleanly: whole numbers as integers, decimals as-is.
fn fmt_qty(qty: f64) -> String {
    if qty.fract() == 0.0 { format!("{}", qty as i64) } else { qty.to_string() }
}

#[tauri::command]
pub async fn pos_add_item(
    input: AddItemInput,
    state: State<'_, AppState>,
) -> Result<Cart, AppError> {
    let product = product_repo::get_product_by_id(&state.db, &input.product_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Product {} not found", input.product_id)))?;

    let qty_str = input.quantity.as_deref().unwrap_or("1");
    let qty_to_add: f64 = qty_str.parse().unwrap_or(1.0);
    let mut cart = input.cart;

    // Merge with an existing active line for the same product rather than duplicating.
    if let Some(existing) = cart.lines.iter_mut()
        .find(|l| !l.voided && l.product_id.as_deref() == Some(product.product.product_id.as_str()))
    {
        let current: f64 = existing.quantity.parse().unwrap_or(1.0);
        existing.quantity = fmt_qty(current + qty_to_add);
        existing.recalculate();
        return Ok(cart);
    }

    // No existing line — create a new one.
    let line = CartLine::new(
        Some(product.product.product_id),
        product.product.name,
        product.product.sku,
        product.product.barcode,
        qty_str,
        product.price_minor,
        product.product.tax_rule_id.unwrap_or_default(),
        product.tax_rate_basis_points,
        product.tax_inclusive,
    );
    cart.lines.push(line);
    Ok(cart)
}

#[derive(serde::Deserialize)]
pub struct AddItemByBarcodeInput {
    pub cart: Cart,
    pub barcode: String,
}

#[tauri::command]
pub async fn pos_add_item_by_barcode(
    input: AddItemByBarcodeInput,
    state: State<'_, AppState>,
) -> Result<Cart, AppError> {
    let product = product_repo::get_product_by_barcode(&state.db, &input.barcode)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Barcode {} not found", input.barcode)))?;

    let mut cart = input.cart;

    // Merge with an existing active line for the same product rather than duplicating.
    if let Some(existing) = cart.lines.iter_mut()
        .find(|l| !l.voided && l.product_id.as_deref() == Some(product.product.product_id.as_str()))
    {
        let current: f64 = existing.quantity.parse().unwrap_or(1.0);
        existing.quantity = fmt_qty(current + 1.0);
        existing.recalculate();
        return Ok(cart);
    }

    // No existing line — create a new one.
    let line = CartLine::new(
        Some(product.product.product_id),
        product.product.name,
        product.product.sku,
        product.product.barcode,
        "1",
        product.price_minor,
        product.product.tax_rule_id.unwrap_or_default(),
        product.tax_rate_basis_points,
        product.tax_inclusive,
    );
    cart.lines.push(line);
    Ok(cart)
}

#[derive(serde::Deserialize)]
pub struct UpdateQuantityInput {
    pub cart: Cart,
    pub cart_line_id: String,
    pub quantity: String,
}

#[tauri::command]
pub async fn pos_update_quantity(input: UpdateQuantityInput) -> Result<Cart, AppError> {
    let qty: f64 = input
        .quantity
        .parse()
        .map_err(|_| AppError::Validation("Invalid quantity".into()))?;
    if qty <= 0.0 {
        return Err(AppError::Validation("Quantity must be positive".into()));
    }
    let mut cart = input.cart;
    if let Some(line) = cart
        .lines
        .iter_mut()
        .find(|l| l.cart_line_id == input.cart_line_id)
    {
        line.quantity = input.quantity;
        line.recalculate();
    }
    Ok(cart)
}

#[derive(serde::Deserialize)]
pub struct RemoveLineInput {
    pub cart: Cart,
    pub cart_line_id: String,
}

#[tauri::command]
pub async fn pos_remove_line(input: RemoveLineInput) -> Result<Cart, AppError> {
    let mut cart = input.cart;
    cart.lines.retain(|l| l.cart_line_id != input.cart_line_id);
    Ok(cart)
}

#[derive(serde::Deserialize)]
pub struct FinalizeSaleInput {
    pub cart: Cart,
    pub payments: Vec<PaymentInput>,
    pub idempotency_key: Option<String>,
    pub customer_id: Option<String>,
    pub delivery: Option<DeliveryInput>,
}

#[tauri::command]
pub async fn pos_finalize_sale(
    input: FinalizeSaleInput,
    state: State<'_, AppState>,
) -> Result<SaleResult, AppError> {
    let key = input
        .idempotency_key
        .unwrap_or_else(|| Ulid::new().to_string());
    let created_offline = !state.sync_worker.state.lock().await.online;

    // Load the allow_negative_stock business flag.
    let flag_val: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'flag_allow_negative_stock'",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let allow_negative_stock = flag_val.as_deref() == Some("1");

    sale_repo::finalize_sale(
        &state.db,
        &input.cart,
        input.payments,
        &key,
        input.customer_id.as_deref(),
        created_offline,
        input.delivery,
        allow_negative_stock,
    )
    .await
}

// ─── Discount commands ────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct ApplyBillDiscountInput {
    pub cart: Cart,
    pub discount_minor: i64,
    /// Non-empty reason required when discount_minor > 0 (goes to audit log).
    pub reason: String,
    /// User authorizing the discount — must be manager or owner.
    pub authorized_by_user_id: String,
}

#[tauri::command]
pub async fn pos_apply_bill_discount(
    input: ApplyBillDiscountInput,
    state: State<'_, AppState>,
) -> Result<Cart, AppError> {
    let discount = input.discount_minor.max(0);
    if discount > 0 && input.reason.trim().is_empty() {
        return Err(AppError::Validation(
            "A reason is required when applying a bill discount".into(),
        ));
    }
    // Discounts require manager or owner authorization — cashiers cannot apply them.
    if discount > 0 {
        rbac::manager_or_owner(&state.db, &input.authorized_by_user_id).await?;
    }
    let mut cart = input.cart;
    cart.bill_discount_minor = discount;
    cart.bill_discount_reason = if discount > 0 {
        Some(input.reason.clone())
    } else {
        None
    };

    // Write audit entry so the discount reason is in the immutable log.
    if discount > 0 {
        let log_id = Ulid::new().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let after_json = serde_json::json!({
            "cart_id": &cart.cart_id,
            "discount_minor": discount,
            "reason": &input.reason,
        })
        .to_string();
        let prev_hash = audit_hash::fetch_last_hash(&state.db, &cart.device_id)
            .await
            .unwrap_or_default();
        let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
            audit_log_id: &log_id,
            event_type: "BILL_DISCOUNT_APPLIED",
            entity_type: "cart",
            entity_id: &cart.cart_id,
            actor_user_id: &cart.cashier_user_id,
            created_at: &now,
            after_json: Some(&after_json),
            previous_hash: &prev_hash,
        });
        let _ = sqlx::query(
            "INSERT INTO audit_logs
               (audit_log_id, event_type, entity_type, entity_id,
                actor_user_id, actor_type, after_json, created_at, hash, previous_hash)
             VALUES (?, 'BILL_DISCOUNT_APPLIED', 'cart', ?, ?, 'user', ?, ?, ?, ?)",
        )
        .bind(&log_id)
        .bind(&cart.cart_id)
        .bind(&cart.cashier_user_id)
        .bind(&after_json)
        .bind(&now)
        .bind(&hash)
        .bind(if prev_hash.is_empty() {
            None
        } else {
            Some(prev_hash.clone())
        })
        .execute(&state.db)
        .await;
    }

    Ok(cart)
}

#[derive(serde::Deserialize)]
pub struct ApplyLineDiscountInput {
    pub cart: Cart,
    pub cart_line_id: String,
    pub discount_minor: i64,
    /// Non-empty reason required when discount_minor > 0 (goes to audit log).
    pub reason: String,
    /// User authorizing the discount — must be manager or owner.
    pub authorized_by_user_id: String,
}

#[tauri::command]
pub async fn pos_apply_line_discount(
    input: ApplyLineDiscountInput,
    state: State<'_, AppState>,
) -> Result<Cart, AppError> {
    let discount = input.discount_minor.max(0);
    if discount > 0 && input.reason.trim().is_empty() {
        return Err(AppError::Validation(
            "A reason is required when applying a line discount".into(),
        ));
    }
    // Discounts require manager or owner authorization — cashiers cannot apply them.
    if discount > 0 {
        rbac::manager_or_owner(&state.db, &input.authorized_by_user_id).await?;
    }
    let mut cart = input.cart;
    if let Some(line) = cart
        .lines
        .iter_mut()
        .find(|l| l.cart_line_id == input.cart_line_id)
    {
        line.line_discount_minor = discount;
        line.line_discount_reason = if discount > 0 {
            Some(input.reason.clone())
        } else {
            None
        };
        line.recalculate();

        // Write audit entry for the line discount.
        if discount > 0 {
            let log_id = Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            let after_json = serde_json::json!({
                "cart_id": &cart.cart_id,
                "cart_line_id": &input.cart_line_id,
                "discount_minor": discount,
                "reason": &input.reason,
            })
            .to_string();
            let prev_hash = audit_hash::fetch_last_hash(&state.db, &cart.device_id)
                .await
                .unwrap_or_default();
            let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
                audit_log_id: &log_id,
                event_type: "LINE_DISCOUNT_APPLIED",
                entity_type: "cart",
                entity_id: &cart.cart_id,
                actor_user_id: &cart.cashier_user_id,
                created_at: &now,
                after_json: Some(&after_json),
                previous_hash: &prev_hash,
            });
            let _ = sqlx::query(
                "INSERT INTO audit_logs
                   (audit_log_id, event_type, entity_type, entity_id,
                    actor_user_id, actor_type, after_json, created_at, hash, previous_hash)
                 VALUES (?, 'LINE_DISCOUNT_APPLIED', 'cart', ?, ?, 'user', ?, ?, ?, ?)",
            )
            .bind(&log_id)
            .bind(&cart.cart_id)
            .bind(&cart.cashier_user_id)
            .bind(&after_json)
            .bind(&now)
            .bind(&hash)
            .bind(if prev_hash.is_empty() {
                None
            } else {
                Some(prev_hash.clone())
            })
            .execute(&state.db)
            .await;
        }
    }
    Ok(cart)
}

#[derive(serde::Deserialize)]
pub struct SetLineNoteInput {
    pub cart: Cart,
    pub cart_line_id: String,
    pub note: Option<String>,
}

#[tauri::command]
pub async fn pos_set_line_note(input: SetLineNoteInput) -> Result<Cart, AppError> {
    let mut cart = input.cart;
    if let Some(line) = cart
        .lines
        .iter_mut()
        .find(|l| l.cart_line_id == input.cart_line_id)
    {
        line.note = input.note.filter(|n| !n.is_empty());
    }
    Ok(cart)
}

// ─── Custom open-price item ───────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct AddCustomItemInput {
    pub cart: Cart,
    pub name: String,
    pub price_minor: i64,
    pub quantity: Option<String>,
}

#[tauri::command]
pub async fn pos_add_custom_item(input: AddCustomItemInput) -> Result<Cart, AppError> {
    if input.name.trim().is_empty() {
        return Err(AppError::Validation("Item name is required".into()));
    }
    if input.price_minor <= 0 {
        return Err(AppError::Validation("Price must be positive".into()));
    }
    let qty = input.quantity.as_deref().unwrap_or("1");
    let qty_f: f64 = qty
        .parse()
        .map_err(|_| AppError::Validation("Invalid quantity".into()))?;
    if qty_f <= 0.0 {
        return Err(AppError::Validation("Quantity must be positive".into()));
    }
    let mut cart = input.cart;
    let line = CartLine::new(
        None, // no product_id — open item
        input.name,
        None, // no sku
        None, // no barcode
        qty,
        input.price_minor,
        String::new(), // no tax rule
        0,             // 0 basis points = tax-exempt
        false,
    );
    cart.lines.push(line);
    Ok(cart)
}

// ─── Void completed sale ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn pos_void_sale(
    sale_id: String,
    voided_by_user_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    // Voiding a completed sale is a manager/owner operation — not a cashier action.
    rbac::manager_or_owner(&state.db, &voided_by_user_id).await?;

    let now = chrono::Utc::now().to_rfc3339();
    let affected = sqlx::query(
        "UPDATE sales SET status = 'voided', updated_at = ?
         WHERE sale_id = ? AND status = 'completed'",
    )
    .bind(&now)
    .bind(&sale_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(
            "Sale not found or already voided".into(),
        ));
    }

    // Fetch branch_id + device_id needed for inventory restoration and audit chain
    let (branch_id, device_id): (String, String) = {
        let row = sqlx::query("SELECT branch_id, device_id FROM sales WHERE sale_id = ?")
            .bind(&sale_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

        match row {
            Some(r) => (
                r.get::<String, _>("branch_id"),
                r.get::<String, _>("device_id"),
            ),
            None => (String::new(), String::new()),
        }
    };

    // Restore stock for all tracked items sold in the voided sale.
    // Best-effort: a failure here should not undo the void itself, but we log it.
    if let Err(e) = movements::return_void_sale(
        &state.db,
        &sale_id,
        &voided_by_user_id,
        &branch_id,
        &device_id,
    )
    .await
    {
        tracing::error!("Stock restoration failed after void of sale {sale_id}: {e}");
    }

    // Record in audit log — best-effort, non-fatal
    let audit_id = ulid::Ulid::new().to_string();
    let prev_hash = audit_hash::fetch_last_hash(&state.db, &device_id)
        .await
        .unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &audit_id,
        event_type: "sale.voided",
        entity_type: "sale",
        entity_id: &sale_id,
        actor_user_id: &voided_by_user_id,
        created_at: &now,
        after_json: None,
        previous_hash: &prev_hash,
    });
    let _ = sqlx::query(
        "INSERT INTO audit_logs
           (audit_log_id, event_type, entity_type, entity_id,
            actor_user_id, actor_type, created_at, hash, previous_hash)
         VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind(&audit_id)
    .bind("sale.voided")
    .bind("sale")
    .bind(&sale_id)
    .bind(&voided_by_user_id)
    .bind("user")
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash.clone())
    })
    .execute(&state.db)
    .await;

    Ok(())
}

// ─── Cart summary ─────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct CartSummary {
    pub gross_total_minor: i64,
    pub tax_total_minor: i64,
    pub discount_total_minor: i64,
    pub net_total_minor: i64,
    pub line_count: usize,
}

#[tauri::command]
pub async fn pos_cart_summary(cart: Cart) -> Result<CartSummary, AppError> {
    Ok(CartSummary {
        gross_total_minor: cart.gross_total(),
        tax_total_minor: cart.tax_total(),
        discount_total_minor: cart.discount_total(),
        net_total_minor: cart.net_total(),
        line_count: cart.lines.iter().filter(|l| !l.voided).count(),
    })
}

/// Record an audit event when a cashier clears a non-empty cart before tendering.
/// This provides an immutable trail of pre-tender voids (basket abandonments).
/// Safe to call even if the cart is empty — no-ops without writing.
#[tauri::command]
pub async fn pos_record_void(
    cart_id: String,
    device_id: String,
    cashier_user_id: String,
    line_count: usize,
    net_total_minor: i64,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    if line_count == 0 {
        return Ok(());
    } // nothing to record for empty carts

    let log_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let detail = serde_json::json!({
        "cart_id": cart_id,
        "line_count": line_count,
        "net_total_minor": net_total_minor,
    })
    .to_string();

    let prev_hash = audit_hash::fetch_last_hash(&state.db, &device_id)
        .await
        .unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &log_id,
        event_type: "CART_VOID",
        entity_type: "cart",
        entity_id: &cart_id,
        actor_user_id: &cashier_user_id,
        created_at: &now,
        after_json: Some(&detail),
        previous_hash: &prev_hash,
    });

    sqlx::query(
        "INSERT INTO audit_logs
           (audit_log_id, event_type, entity_type, entity_id,
            actor_user_id, actor_type, after_json, created_at, hash, previous_hash)
         VALUES (?, 'CART_VOID', 'cart', ?, ?, 'user', ?, ?, ?, ?)",
    )
    .bind(&log_id)
    .bind(&cart_id)
    .bind(&cashier_user_id)
    .bind(&detail)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash.clone())
    })
    .execute(&state.db)
    .await?;

    Ok(())
}
