use crate::commands::rbac;
use crate::db::helpers;
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
    state: State<'_, AppState>,
) -> Result<StartCartResult, AppError> {
    crate::commands::rbac::require_any_role(&state.db, &input.cashier_user_id).await?;
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

#[tauri::command]
pub async fn pos_add_item(
    input: AddItemInput,
    state: State<'_, AppState>,
) -> Result<Cart, AppError> {
    crate::commands::rbac::require_any_role(&state.db, &input.cart.cashier_user_id).await?;
    let product = product_repo::get_product_by_id(&state.db, &input.product_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Product {} not found", input.product_id)))?;

    let qty_str = input.quantity.as_deref().unwrap_or("1");
    // Validate quantity using integer-only arithmetic (no f64 round-trips).
    if !crate::domain::money::qty_in_range(qty_str, 1_000_000) {
        return Err(AppError::Validation(format!(
            "Invalid quantity: {qty_str}"
        )));
    }
    // Decimal quantity check: if there is a non-zero fractional part and the
    // product does not allow decimal quantities, reject.
    if !product.product.allow_decimal_quantity {
        if let Some((_, frac)) = qty_str.split_once('.') {
            if frac.trim_end_matches('0').len() > 0 {
                return Err(AppError::Validation(
                    "This product does not allow decimal quantities".into()
                ));
            }
        }
    }
    let mut cart = input.cart;

    // Merge with an existing active line for the same product rather than duplicating.
    if let Some(existing) = cart.lines.iter_mut()
        .find(|l| !l.voided && l.product_id.as_deref() == Some(product.product.product_id.as_str()))
    {
        existing.quantity = crate::domain::money::add_decimal_qty_str(&existing.quantity, qty_str);
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
    crate::commands::rbac::require_any_role(&state.db, &input.cart.cashier_user_id).await?;
    let product = match product_repo::get_product_by_barcode(&state.db, &input.barcode).await? {
        Some(p) => p,
        None => {
            // Record unknown barcode for ghost resolution workflow
            // best-effort: record unknown barcode for admin resolution
            let _ = crate::commands::ghost_barcode_commands::ghost_record(
                input.barcode.clone(),
                input.cart.cashier_user_id.clone(),
                state.clone(),
            )
            .await;
            return Err(AppError::GhostBarcode(input.barcode));
        }
    };

    let mut cart = input.cart;

    // Merge with an existing active line for the same product rather than duplicating.
    if let Some(existing) = cart.lines.iter_mut()
        .find(|l| !l.voided && l.product_id.as_deref() == Some(product.product.product_id.as_str()))
    {
        existing.quantity = crate::domain::money::add_decimal_qty_str(&existing.quantity, "1");
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
pub async fn pos_update_quantity(
    input: UpdateQuantityInput,
    state: State<'_, AppState>,
) -> Result<Cart, AppError> {
    crate::commands::rbac::require_any_role(&state.db, &input.cart.cashier_user_id).await?;
    // Validate quantity using integer-only arithmetic (no f64 round-trips).
    if !crate::domain::money::qty_in_range(&input.quantity, 1_000_000) {
        return Err(AppError::Validation(format!(
            "Invalid quantity: {}", input.quantity
        )));
    }
    // Guard: reject decimal quantities for products that don't allow them.
    if let Some((_, frac)) = input.quantity.split_once('.') {
        if frac.trim_end_matches('0').len() > 0 {
            let product_id = input
                .cart
                .lines
                .iter()
                .find(|l| l.cart_line_id == input.cart_line_id)
                .and_then(|l| l.product_id.as_deref());
            if let Some(pid) = product_id {
                let allow_decimal: bool = sqlx::query_scalar(
                    "SELECT allow_decimal_quantity FROM products WHERE product_id = ?",
                )
                .bind(pid)
                .fetch_optional(&state.db)
                .await?
                .flatten()
                .unwrap_or(false);
                if !allow_decimal {
                    return Err(AppError::Validation(
                        "This product does not allow decimal quantities".into()
                    ));
                }
            }
        }
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
pub struct SetLinePriceInput {
    pub cart: Cart,
    pub cart_line_id: String,
    pub price_minor: i64,
    /// User authorizing the price override — must be manager or owner.
    pub authorized_by_user_id: String,
}

#[tauri::command]
pub async fn pos_set_line_price(
    input: SetLinePriceInput,
    state: State<'_, AppState>,
) -> Result<Cart, AppError> {
    rbac::manager_or_owner(&state.db, &input.authorized_by_user_id).await?;
    if input.price_minor <= 0 {
        return Err(AppError::Validation("Price must be positive".into()));
    }
    let mut cart = input.cart;
    if let Some(line) = cart
        .lines
        .iter_mut()
        .find(|l| l.cart_line_id == input.cart_line_id && !l.voided)
    {
        line.unit_price_minor = input.price_minor;
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
pub async fn pos_remove_line(
    input: RemoveLineInput,
    state: State<'_, AppState>,
) -> Result<Cart, AppError> {
    crate::commands::rbac::require_any_role(&state.db, &input.cart.cashier_user_id).await?;
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

    // Guard: confirm the submitted shift_id is an OPEN shift that belongs to this device.
    // Prevents stale-cart attacks and cross-device shift forgery from the frontend.
    let shift_device: Option<String> = sqlx::query_scalar(
        "SELECT device_id FROM shifts WHERE shift_id = ? AND status = 'open'",
    )
    .bind(&input.cart.shift_id)
    .fetch_optional(&state.db)
    .await?;
    match shift_device {
        Some(ref dev) if dev == &input.cart.device_id => {} // OK
        Some(_) => {
            return Err(AppError::Validation(
                "Shift does not belong to this device.".into(),
            ))
        }
        None => {
            return Err(AppError::Validation(
                "No open shift found for this sale. Please open a shift first.".into(),
            ))
        }
    }

    // Guard: verify the cashier_user_id from the cart is an active user.
    // Prevents a compromised frontend from attributing sales to any user.
    crate::commands::rbac::require_any_role(&state.db, &input.cart.cashier_user_id).await?;

    // Load the allow_negative_stock business flag.
    let flag_val: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'flag_allow_negative_stock'",
    )
    .fetch_optional(&state.db)
    .await
    .inspect_err(|e| tracing::warn!("Failed to read flag_allow_negative_stock: {e}; defaulting to false (stock guard ON)"))
    .ok()
    .flatten();
    let allow_negative_stock = flag_val.as_deref() == Some("1");

    let result = sale_repo::finalize_sale(
        &state.db,
        &input.cart,
        input.payments,
        &key,
        input.customer_id.as_deref(),
        created_offline,
        input.delivery,
        allow_negative_stock,
    )
    .await?;

    // Auto-print receipt if the business flag is enabled.
    // Batch all printer config keys into a single query (was 5 sequential round-trips).
    let printer_cfg: std::collections::HashMap<String, String> = sqlx::query(
        "SELECT key, value FROM app_config WHERE key IN (
         'flag_auto_print_receipt','thermal_printer_enabled','thermal_printer_port',
         'thermal_printer_baud','store_name')",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r: sqlx::sqlite::SqliteRow| {
        let k: String = r.get("key");
        let v: String = r.get("value");
        (k, v)
    })
    .collect();
    let auto_print = printer_cfg.get("flag_auto_print_receipt").map(|s| s.as_str());
    if auto_print == Some("1") {
        let enabled = printer_cfg.get("thermal_printer_enabled").map(|s| s.as_str()).unwrap_or("0");
        // Clone into owned Strings so the borrow is released before spawn_blocking.
        let port: String = printer_cfg.get("thermal_printer_port").cloned().unwrap_or_default();
        let baud_str = printer_cfg.get("thermal_printer_baud").map(|s| s.as_str()).unwrap_or("9600");
        if enabled == "1" && !port.trim().is_empty() {
            let store_name: String = printer_cfg.get("store_name").cloned().unwrap_or_default();
            let mut lines: Vec<String> = Vec::new();
            lines.push(format!("Receipt: {}", result.receipt_number));
            lines.push(format!("Date: {}", result.business_date));
            lines.push(format!("Cashier: {}", result.cashier_name));
            lines.push(String::new());
            for item in &result.items {
                lines.push(format!(
                    "{} x{} @ {} = {}",
                    item.product_name,
                    item.quantity,
                    crate::domain::money::format_minor(item.unit_price_minor, 3),
                    crate::domain::money::format_minor(item.line_total_minor, 3),
                ));
            }
            lines.push(String::new());
            lines.push(format!(
                "TOTAL: {} {}",
                crate::domain::money::format_minor(result.net_total_minor, 3),
                result.currency,
            ));
            let payload =
                crate::commands::thermal_commands::build_receipt_bytes(&store_name, &lines);
            let baud: u32 = baud_str.parse().unwrap_or(9600);
            let port_clone = port.clone();
            // Fire-and-forget: don't block the sale response on print completion
            tokio::task::spawn_blocking(move || {
                if let Err(e) = crate::commands::thermal_commands::write_to_port(
                    &port_clone,
                    baud,
                    payload,
                ) {
                    tracing::warn!("Auto-print failed: {}", e);
                }
            });
        }
    }

    Ok(result)
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

/// Load the two discount-policy flags in a single query to avoid 2 round-trips.
/// Returns `(require_discount_reason, cashier_can_discount)`.
/// Defaults: require_reason = true (fail-safe), cashier_can_discount = false (fail-safe).
async fn load_discount_flags(db: &sqlx::SqlitePool) -> (bool, bool) {
    let rows: Vec<(String, String)> = sqlx::query(
        "SELECT key, value FROM app_config WHERE key IN
         ('flag_require_discount_reason', 'flag_cashier_can_discount')",
    )
    .fetch_all(db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r: sqlx::sqlite::SqliteRow| {
        (r.get("key"), r.get("value"))
    })
    .collect();

    let find = |key: &str| -> Option<&str> {
        rows.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    };
    let require_discount_reason = find("flag_require_discount_reason") != Some("0"); // default true
    let cashier_can_discount = find("flag_cashier_can_discount") == Some("1");

    (require_discount_reason, cashier_can_discount)
}

#[tauri::command]
pub async fn pos_apply_bill_discount(
    input: ApplyBillDiscountInput,
    state: State<'_, AppState>,
) -> Result<Cart, AppError> {
    let discount = input.discount_minor.max(0);
    let (require_discount_reason, cashier_can_discount) =
        load_discount_flags(&state.db).await;

    // Upper-bound: discount cannot exceed the post-line-discount cart total
    if discount > 0 {
        let post_line = input.cart.post_line_total();
        if discount > post_line {
            return Err(AppError::Validation(format!(
                "Discount ({discount}) exceeds cart total ({post_line})"
            )));
        }
    }

    if discount > 0 && require_discount_reason && input.reason.trim().is_empty() {
        return Err(AppError::Validation(
            "A reason is required when applying a bill discount".into(),
        ));
    }
    if discount > 0 && !cashier_can_discount {
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
            actor_type: "user",
            created_at: &now,
            before_json: None,
            after_json: Some(&after_json),
            reason: None,
            previous_hash: &prev_hash,
        });
        let _ = sqlx::query(
            "INSERT INTO audit_logs
               (audit_log_id, event_type, entity_type, entity_id,
                actor_user_id, actor_type, device_id, origin_device_id, branch_id, after_json, created_at, hash, previous_hash)
             VALUES (?, 'BILL_DISCOUNT_APPLIED', 'cart', ?, ?, 'user', ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&log_id)
        .bind(&cart.cart_id)
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
    let (require_discount_reason, cashier_can_discount) =
        load_discount_flags(&state.db).await;

    // Upper-bound: discount cannot exceed the line's own subtotal (unit_price × qty)
    if discount > 0 {
        if let Some(line) = input.cart.lines.iter().find(|l| l.cart_line_id == input.cart_line_id) {
            let line_subtotal = crate::domain::money::mul_minor_by_qty(line.unit_price_minor, &line.quantity);
            if discount > line_subtotal {
                return Err(AppError::Validation(format!(
                    "Discount ({discount}) exceeds line subtotal ({line_subtotal})"
                )));
            }
        }
    }

    if discount > 0 && require_discount_reason && input.reason.trim().is_empty() {
        return Err(AppError::Validation(
            "A reason is required when applying a line discount".into(),
        ));
    }
    if discount > 0 && !cashier_can_discount {
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
                actor_type: "user",
                created_at: &now,
                before_json: None,
                after_json: Some(&after_json),
                reason: None,
                previous_hash: &prev_hash,
            });
            let _ = sqlx::query(
                "INSERT INTO audit_logs
                   (audit_log_id, event_type, entity_type, entity_id,
                    actor_user_id, actor_type, device_id, origin_device_id, branch_id, after_json, created_at, hash, previous_hash)
                 VALUES (?, 'LINE_DISCOUNT_APPLIED', 'cart', ?, ?, 'user', ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&log_id)
            .bind(&cart.cart_id)
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
pub async fn pos_set_line_note(
    input: SetLineNoteInput,
    state: State<'_, AppState>,
) -> Result<Cart, AppError> {
    crate::commands::rbac::require_any_role(&state.db, &input.cart.cashier_user_id).await?;
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
pub async fn pos_add_custom_item(
    input: AddCustomItemInput,
    state: State<'_, AppState>,
) -> Result<Cart, AppError> {
    crate::commands::rbac::require_any_role(&state.db, &input.cart.cashier_user_id).await?;
    if input.name.trim().is_empty() {
        return Err(AppError::Validation("Item name is required".into()));
    }
    if input.price_minor <= 0 {
        return Err(AppError::Validation("Price must be positive".into()));
    }
    let qty = input.quantity.as_deref().unwrap_or("1");
    // Validate quantity using integer-only arithmetic (no f64 round-trips).
    if !crate::domain::money::qty_in_range(qty, 1_000_000) {
        return Err(AppError::Validation(format!(
            "Invalid quantity: {qty}"
        )));
    }
    let mut cart = input.cart;
    let line = CartLine::new(
        None, // no product_id — open item
        input.name.clone(),
        None, // no sku
        None, // no barcode
        qty,
        input.price_minor,
        String::new(), // no tax rule
        0,             // 0 basis points = tax-exempt
        false,
    );
    cart.lines.push(line);

    // Audit log: custom items bypass the product catalogue so their cashier-set
    // price would otherwise be invisible. Write an immutable record here.
    let log_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let after_json = serde_json::json!({
        "cart_id":      &cart.cart_id,
        "name":         &input.name,
        "price_minor":  input.price_minor,
        "quantity":     qty,
    })
    .to_string();
    let prev_hash = audit_hash::fetch_last_hash(&state.db, &cart.device_id)
        .await
        .unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &log_id,
        event_type: "CUSTOM_ITEM_ADDED",
        entity_type: "cart",
        entity_id: &cart.cart_id,
        actor_user_id: &cart.cashier_user_id,
        actor_type: "user",
        created_at: &now,
        before_json: None,
        after_json: Some(&after_json),
        reason: None,
        previous_hash: &prev_hash,
    });
    let _ = sqlx::query(
        "INSERT INTO audit_logs
           (audit_log_id, event_type, entity_type, entity_id,
            actor_user_id, actor_type, device_id, origin_device_id, branch_id, after_json, created_at, hash, previous_hash)
         VALUES (?, 'CUSTOM_ITEM_ADDED', 'cart', ?, ?, 'user', ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&log_id)
    .bind(&cart.cart_id)
    .bind(&cart.cashier_user_id)
    .bind(&cart.device_id)
    .bind(&cart.device_id)
    .bind(&cart.branch_id)
    .bind(&after_json)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() { None } else { Some(prev_hash) })
    .execute(&state.db)
    .await;

    Ok(cart)
}

// ─── Void completed sale ──────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct VoidSaleResult {
    /// Always true when the command succeeds (sale status changed to voided).
    pub voided: bool,
    /// Non-null when stock restoration failed. The void was committed but inventory
    /// may be inaccurate — the manager should reconcile manually.
    pub stock_warning: Option<String>,
}

#[tauri::command]
pub async fn pos_void_sale(
    sale_id: String,
    voided_by_user_id: String,
    state: State<'_, AppState>,
) -> Result<VoidSaleResult, AppError> {
    // Voiding a completed sale is a manager/owner operation — not a cashier action.
    rbac::manager_or_owner(&state.db, &voided_by_user_id).await?;

    let now = chrono::Utc::now().to_rfc3339();

    // Merge UPDATE + SELECT into a single RETURNING query (saves one round-trip).
    let row = sqlx::query(
        "UPDATE sales SET status = 'voided', updated_at = ?, sync_status = 'pending'
         WHERE sale_id = ? AND status = 'completed'
         RETURNING branch_id, device_id",
    )
    .bind(&now)
    .bind(&sale_id)
    .fetch_optional(&state.db)
    .await?;

    let (branch_id, device_id) = match row {
        Some(r) => (
            r.get::<String, _>("branch_id"),
            r.get::<String, _>("device_id"),
        ),
        None => {
            return Err(AppError::NotFound(
                "Sale not found or already voided".into(),
            ))
        }
    };

    // Restore stock for all tracked items sold in the voided sale.
    // The void is already committed — if stock restore fails we surface a warning
    // to the caller instead of silently discarding the error, so the manager knows
    // to reconcile inventory manually.
    let stock_warning = match movements::return_void_sale(
        &state.db,
        &sale_id,
        &voided_by_user_id,
        &branch_id,
        &device_id,
    )
    .await
    {
        Ok(_) => None,
        Err(e) => {
            tracing::error!("Stock restoration failed after void of sale {sale_id}: {e}");
            Some(format!(
                "Sale voided but inventory could not be updated: {e}. \
                 Please reconcile stock levels manually."
            ))
        }
    };

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
        actor_type: "user",
        created_at: &now,
        before_json: None,
        after_json: None,
        reason: None,
        previous_hash: &prev_hash,
    });
    let _ = sqlx::query(
        "INSERT INTO audit_logs
           (audit_log_id, event_type, entity_type, entity_id,
            actor_user_id, actor_type, device_id, origin_device_id, branch_id, created_at, hash, previous_hash)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&audit_id)
    .bind("sale.voided")
    .bind("sale")
    .bind(&sale_id)
    .bind(&voided_by_user_id)
    .bind("user")
    .bind(&device_id)
    .bind(&device_id)
    .bind(&branch_id)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash.clone())
    })
    .execute(&state.db)
    .await;

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    Ok(VoidSaleResult { voided: true, stock_warning })
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
pub async fn pos_cart_summary(
    cart: Cart,
    state: State<'_, AppState>,
) -> Result<CartSummary, AppError> {
    crate::commands::rbac::require_any_role(&state.db, &cart.cashier_user_id).await?;
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
    crate::commands::rbac::require_any_role(&state.db, &cashier_user_id).await?;
    if line_count == 0 {
        return Ok(());
    } // nothing to record for empty carts

    let log_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let branch_id = helpers::active_branch_id(&state.db).await?;
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
        actor_type: "user",
        created_at: &now,
        before_json: None,
        after_json: Some(&detail),
        reason: None,
        previous_hash: &prev_hash,
    });

    sqlx::query(
        "INSERT INTO audit_logs
           (audit_log_id, event_type, entity_type, entity_id,
            actor_user_id, actor_type, device_id, origin_device_id, branch_id, after_json, created_at, hash, previous_hash)
         VALUES (?, 'CART_VOID', 'cart', ?, ?, 'user', ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&log_id)
    .bind(&cart_id)
    .bind(&cashier_user_id)
    .bind(&device_id)
    .bind(&device_id)
    .bind(&branch_id)
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

// ─── Load sale for edit ────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct LoadSaleForEditInput {
    pub receipt_number: String,
    pub branch_id: String,
    pub device_id: String,
    pub shift_id: String,
    pub cashier_user_id: String,
}

/// Load a completed sale back into a Cart for editing.
/// Queries the sale's line items and reconstructs them as custom items
/// with their original prices, quantities, and tax snapshots.
/// Single backend call — avoids the stale-closure race in the TS loop.
#[tauri::command]
pub async fn pos_load_sale_for_edit(
    input: LoadSaleForEditInput,
    state: State<'_, AppState>,
) -> Result<Cart, AppError> {
    crate::commands::rbac::require_any_role(&state.db, &input.cashier_user_id).await?;

    let rows = sqlx::query(
        "SELECT si.product_name_snapshot, si.quantity, si.unit_price_minor,
                si.sku_snapshot, si.barcode_snapshot,
                si.tax_rule_snapshot, si.line_discount_minor
         FROM sale_items si
         JOIN sales s ON s.sale_id = si.sale_id
         WHERE s.receipt_number = ? AND s.status != 'voided'
         ORDER BY si.created_at",
    )
    .bind(&input.receipt_number)
    .fetch_all(&state.db)
    .await?;

    if rows.is_empty() {
        return Err(AppError::NotFound(format!(
            "Sale {} not found or has no items",
            input.receipt_number
        )));
    }

    let mut cart = Cart::new(
        input.branch_id,
        input.device_id,
        input.shift_id,
        input.cashier_user_id,
    );

    for row in &rows {
        let product_name: String = row.get("product_name_snapshot");
        let quantity: String = row.get("quantity");
        let unit_price_minor: i64 = row.get("unit_price_minor");
        let sku: Option<String> = row.get("sku_snapshot");
        let barcode: Option<String> = row.get("barcode_snapshot");
        let tax_json: String = row.get("tax_rule_snapshot");
        let line_discount: i64 = row.get("line_discount_minor");

        let tax: serde_json::Value =
            serde_json::from_str(&tax_json).unwrap_or_default();
        let tax_rule_id = tax
            .get("rule_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let tax_bp = tax
            .get("rate_basis_points")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let tax_inclusive = tax
            .get("inclusive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut line = CartLine::new(
            None,
            product_name,
            sku,
            barcode,
            &quantity,
            unit_price_minor,
            tax_rule_id,
            tax_bp,
            tax_inclusive,
        );
        if line_discount > 0 {
            line.line_discount_minor = line_discount;
            line.recalculate();
        }
        cart.lines.push(line);
    }

    Ok(cart)
}
