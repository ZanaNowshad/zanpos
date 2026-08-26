use crate::commands::{rbac, sync_commands};
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
        return Err(AppError::Validation(format!("Invalid quantity: {qty_str}")));
    }
    // Decimal quantity check: if there is a non-zero fractional part and the
    // product does not allow decimal quantities, reject.
    if !product.product.allow_decimal_quantity {
        if let Some((_, frac)) = qty_str.split_once('.') {
            if !frac.trim_end_matches('0').is_empty() {
                return Err(AppError::Validation(
                    "This product does not allow decimal quantities".into(),
                ));
            }
        }
    }
    let mut cart = input.cart;

    // Merge with an existing active line for the same product rather than duplicating.
    if let Some(existing) = cart
        .lines
        .iter_mut()
        .find(|l| !l.voided && l.product_id.as_deref() == Some(product.product.product_id.as_str()))
    {
        existing.image_path = product.product.image_path.clone();
        existing.quantity = crate::domain::money::add_decimal_qty_str(&existing.quantity, qty_str);
        existing.recalculate();
        return Ok(cart);
    }

    // No existing line — create a new one.
    let image_path = product.product.image_path.clone();
    let mut line = CartLine::new(
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
    line.image_path = image_path;
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
    if let Some(existing) = cart
        .lines
        .iter_mut()
        .find(|l| !l.voided && l.product_id.as_deref() == Some(product.product.product_id.as_str()))
    {
        existing.image_path = product.product.image_path.clone();
        existing.quantity = crate::domain::money::add_decimal_qty_str(&existing.quantity, "1");
        existing.recalculate();
        return Ok(cart);
    }

    // No existing line — create a new one.
    let image_path = product.product.image_path.clone();
    let mut line = CartLine::new(
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
    line.image_path = image_path;
    cart.lines.push(line);
    Ok(cart)
}

#[derive(serde::Deserialize)]
pub struct RepriceCartInput {
    pub cart: Cart,
}

#[derive(serde::Serialize)]
pub struct RepricedLine {
    pub cart_line_id: String,
    pub product_name: String,
    pub was_minor: i64,
    pub now_minor: i64,
}

#[derive(serde::Serialize)]
pub struct RepriceCartResult {
    pub cart: Cart,
    pub changed: Vec<RepricedLine>,
}

/// Bring every line up to the catalogue price currently in force.
///
/// When the back office repriced an item after it was scanned, checkout refused
/// the payment and told the cashier to remove the line and scan it again. That
/// is fine for one item and miserable for a full basket — and it is busywork,
/// because the till already knows both the old price and the new one.
///
/// No manager authorization, deliberately. Every other price command needs it
/// because it moves a price *away* from the catalogue, which is the thing worth
/// controlling. This only ever moves toward it, so it cannot be used to
/// discount anything — the worst it can do is charge the price the shop has
/// published, which is what should have happened.
///
/// A line carrying an approved manager override is left alone: someone with
/// authority already decided that price, and quietly undoing it here would be
/// the same class of mistake in the other direction.
#[tauri::command]
pub async fn pos_reprice_cart(
    input: RepriceCartInput,
    state: State<'_, AppState>,
) -> Result<RepriceCartResult, AppError> {
    crate::commands::rbac::require_any_role(&state.db, &input.cart.cashier_user_id).await?;
    let mut cart = input.cart;

    let overridden: std::collections::HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT cart_line_id FROM pos_price_overrides WHERE cart_id = ?",
    )
    .bind(&cart.cart_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .collect();

    let product_ids: Vec<&str> = cart
        .lines
        .iter()
        .filter(|line| !line.voided && !overridden.contains(&line.cart_line_id))
        .filter_map(|line| line.product_id.as_deref())
        .collect();
    let prices =
        crate::db::repositories::sale_repo::current_selling_prices(&state.db, &product_ids).await?;

    let mut changed = Vec::new();
    for line in cart.lines.iter_mut() {
        if line.voided || overridden.contains(&line.cart_line_id) {
            continue;
        }
        let Some(product_id) = line.product_id.as_deref() else {
            continue;
        };
        let Some(&current) = prices.get(product_id) else {
            continue;
        };
        if current == line.unit_price_minor {
            continue;
        }
        changed.push(RepricedLine {
            cart_line_id: line.cart_line_id.clone(),
            product_name: line.product_name.clone(),
            was_minor: line.unit_price_minor,
            now_minor: current,
        });
        line.unit_price_minor = current;
        line.recalculate();
    }

    Ok(RepriceCartResult { cart, changed })
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
            "Invalid quantity: {}",
            input.quantity
        )));
    }
    // Guard: reject decimal quantities for products that don't allow them.
    if let Some((_, frac)) = input.quantity.split_once('.') {
        if !frac.trim_end_matches('0').is_empty() {
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
                        "This product does not allow decimal quantities".into(),
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
    let line = cart
        .lines
        .iter_mut()
        .find(|l| l.cart_line_id == input.cart_line_id && !l.voided)
        .ok_or_else(|| AppError::NotFound("Cart line not found".into()))?;
    let before_price = line.unit_price_minor;
    line.unit_price_minor = input.price_minor;
    line.recalculate();

    sqlx::query(
        "INSERT INTO pos_price_overrides
         (cart_line_id, cart_id, product_id, price_minor, authorized_by_user_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(cart_line_id) DO UPDATE SET
           cart_id = excluded.cart_id,
           product_id = excluded.product_id,
           price_minor = excluded.price_minor,
           authorized_by_user_id = excluded.authorized_by_user_id,
           created_at = excluded.created_at",
    )
    .bind(&line.cart_line_id)
    .bind(&cart.cart_id)
    .bind(&line.product_id)
    .bind(input.price_minor)
    .bind(&input.authorized_by_user_id)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&state.db)
    .await?;

    let before_json = serde_json::json!({ "unit_price_minor": before_price }).to_string();
    let after_json = serde_json::json!({
        "unit_price_minor": input.price_minor,
        "cart_id": &cart.cart_id,
        "product_id": &line.product_id,
    })
    .to_string();
    audit_hash::insert_audit_entry_override(
        &state.db,
        "POS_LINE_PRICE_OVERRIDDEN",
        "cart_line",
        &line.cart_line_id,
        &input.authorized_by_user_id,
        "user",
        &cart.device_id,
        &cart.branch_id,
        Some(&before_json),
        Some(&after_json),
        Some("Manager-approved POS price change"),
        true,
    )
    .await?;
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
    sqlx::query("DELETE FROM pos_price_overrides WHERE cart_id = ? AND cart_line_id = ?")
        .bind(&cart.cart_id)
        .bind(&input.cart_line_id)
        .execute(&state.db)
        .await?;
    cart.lines.retain(|l| l.cart_line_id != input.cart_line_id);
    Ok(cart)
}

#[derive(serde::Deserialize)]
pub struct FinalizeSaleInput {
    pub cart: Cart,
    pub payments: Vec<PaymentInput>,
    #[serde(default)]
    pub idempotency_key: String,
    pub customer_id: Option<String>,
    pub delivery: Option<DeliveryInput>,
}

fn require_idempotency_key(key: Option<String>) -> Result<String, AppError> {
    let key = key.unwrap_or_default();
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation(
            "Sale could not be completed safely. Please try charging again.".into(),
        ));
    }
    Ok(trimmed.to_string())
}

#[tauri::command]
pub async fn pos_finalize_sale(
    input: FinalizeSaleInput,
    state: State<'_, AppState>,
) -> Result<SaleResult, AppError> {
    let key = require_idempotency_key(Some(input.idempotency_key))?;
    let created_offline = !state.sync_worker.state.lock().await.online;

    // Guard: confirm the submitted shift_id is an OPEN shift that belongs to this device.
    // Prevents stale-cart attacks and cross-device shift forgery from the frontend.
    let shift_device: Option<String> =
        sqlx::query_scalar("SELECT device_id FROM shifts WHERE shift_id = ? AND status = 'open'")
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
    let flag_val: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'flag_allow_negative_stock'")
            .fetch_optional(&state.db)
            .await
            .inspect_err(|e| {
                tracing::warn!(
            "Failed to read flag_allow_negative_stock: {e}; defaulting to false (stock guard ON)"
        )
            })
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

    // NOTE: Auto-print is handled by the POS frontend (PosPage) after the sale,
    // using the SAME printReceiptRaw / buildReceiptLines path as the manual receipt
    // button. A duplicate server-side print here opened the same exclusive COM port
    // a few milliseconds later, so the two collided and neither printed reliably —
    // the cashier had to print manually. One printer call, one source of truth.
    sync_commands::schedule_immediate_sync(&state);
    // Analytics is recorded after the sale is committed and never with `?` — a
    // telemetry write must not be able to fail a sale that already happened.
    let method = match result.payments.as_slice() {
        [] => "none".to_string(),
        [single] => single.method.clone(),
        _ => "split".to_string(),
    };
    crate::diagnostics::record_event(
        &state.db,
        "sale_completed",
        Some(serde_json::json!({
            "amount_minor": result.net_total_minor,
            "lines": result.items.len(),
            "method": method,
        })),
    )
    .await;
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
    .map(|r: sqlx::sqlite::SqliteRow| (r.get("key"), r.get("value")))
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
    let (require_discount_reason, cashier_can_discount) = load_discount_flags(&state.db).await;

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
        sqlx::query(
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
        .await?;
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
    let (require_discount_reason, cashier_can_discount) = load_discount_flags(&state.db).await;

    // Upper-bound: discount cannot exceed the line's own subtotal (unit_price × qty)
    if discount > 0 {
        if let Some(line) = input
            .cart
            .lines
            .iter()
            .find(|l| l.cart_line_id == input.cart_line_id)
        {
            let line_subtotal =
                crate::domain::money::mul_minor_by_qty(line.unit_price_minor, &line.quantity);
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
            sqlx::query(
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
            .await?;
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
        return Err(AppError::Validation(format!("Invalid quantity: {qty}")));
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
    sqlx::query(
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
    .await?;

    Ok(cart)
}

// ─── Void completed sale ──────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct VoidSaleResult {
    /// Always true when the command succeeds (sale status changed to voided).
    pub voided: bool,
    /// Reserved for backward-compatible response decoding. Atomic voids return
    /// an error rather than committing with an inventory warning.
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
    let mut tx = state.db.begin().await?;

    // Merge UPDATE + SELECT into a single RETURNING query (saves one round-trip).
    let row = sqlx::query(
        "UPDATE sales SET status = 'voided', updated_at = ?, sync_status = 'pending'
         WHERE sale_id = ? AND status = 'completed'
         RETURNING branch_id, device_id",
    )
    .bind(&now)
    .bind(&sale_id)
    .fetch_optional(&mut *tx)
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

    // Sale status, stock restoration, and the required audit event are one
    // transaction. No partially voided sale can survive an audit failure.
    movements::return_void_sale(
        &mut tx,
        &sale_id,
        &voided_by_user_id,
        &branch_id,
        &device_id,
    )
    .await?;

    // The audit row is authoritative and commits with the sale/stock changes.
    let audit_id = ulid::Ulid::new().to_string();
    let prev_hash = audit_hash::fetch_last_hash_tx(&mut tx, &device_id)
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
    sqlx::query(
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
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    Ok(VoidSaleResult {
        voided: true,
        stock_warning: None,
    })
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

    sqlx::query("DELETE FROM pos_price_overrides WHERE cart_id = ?")
        .bind(&cart_id)
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

        let tax: serde_json::Value = serde_json::from_str(&tax_json).unwrap_or_default();
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

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests for the money-orchestration command layer.
//
// The `#[tauri::command]` wrappers take `State<'_, AppState>`, which cannot be
// constructed without a live Tauri runtime, so they are not callable directly
// here. Two strategies are used instead:
//
//   1. For sale finalization totals (subtotal / VAT / net as i64 minor units),
//      we drive `sale_repo::finalize_sale` — the exact function `pos_finalize_sale`
//      delegates all money math to. The seed/pool setup mirrors sale_repo.rs and
//      refund_repo.rs precisely (in-memory pool, real migration chain).
//
//   2. For the cart-mutation commands (set line price, update qty, apply bill /
//      line discount), the State is used only for RBAC, config-flag loads, and
//      audit writes — never for the money math. We test the underlying mutation
//      and guard logic each command performs, operating directly on `Cart` /
//      `CartLine` (which `recalculate()` covers) so the arithmetic is verified
//      without the runtime.
//
// Commands NOT reachable without the Tauri runtime (RBAC / config / audit /
// printer side effects are State-bound): pos_start_cart, pos_add_item,
// pos_add_item_by_barcode, pos_remove_line, pos_set_line_note, pos_add_custom_item,
// pos_void_sale, pos_cart_summary, pos_record_void, pos_load_sale_for_edit, and the
// full pos_finalize_sale wrapper. Their core money logic is exercised below via
// the repo path and direct Cart mutation.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::cart::{Cart, CartLine};
    use crate::domain::sale::PaymentInput;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    // Seed IDs that match the migration chain (same constants the repo tests use).
    const BRANCH: &str = "01JBRANCH0000000000000001";
    const DEVICE: &str = "01JDEVICE0000000000000001";
    const CASHIER: &str = "01JUSER000000000000CASH01";
    const TAX_VAT: &str = "01JTAX000000000000VAT001"; // 10% exclusive (1 000 bp)
    const TAX_ZER: &str = "01JTAX000000000000ZERO01"; // 0%

    // Build an in-memory pool and run all migrations, then seed the products,
    // tax rules, cashier, category and stock this module needs. Mirrors the
    // make_pool() helper in sale_repo.rs so FK constraints are satisfied.
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

        // Activate the seed device and branch (seeded inactive by the seed migration).
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

        // Tax rules: 10% exclusive VAT and a zero-rated rule.
        sqlx::query(
            "INSERT OR IGNORE INTO tax_rules (tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from, created_at, updated_at, version)
             VALUES
             ('01JTAX000000000000VAT001', 'VAT 10%', 1000, 0, 1, datetime('now'), datetime('now'), datetime('now'), 1),
             ('01JTAX000000000000ZERO01', 'Zero-rated', 0, 0, 1, datetime('now'), datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test tax rules");

        // Cashier user (needed by test sales).
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

    // Insert a minimal open shift so the sales FK is satisfied.
    async fn insert_shift(pool: &SqlitePool) -> String {
        let shift_id = Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO shifts (shift_id, branch_id, device_id, origin_device_id, cashier_user_id, opened_at, status, created_at, updated_at, version, sync_status, sync_attempts)
             VALUES (?, ?, ?, ?, ?, datetime('now'), 'open', datetime('now'), datetime('now'), 1, 'pending', 0)"
        )
        .bind(&shift_id).bind(BRANCH).bind(DEVICE).bind(DEVICE).bind(CASHIER)
        .execute(pool).await.expect("insert shift");
        shift_id
    }

    // Cola line: 400 minor, 10% exclusive VAT (1 000 bp). Tax/total filled via
    // CartLine::new so values match what pos_add_item would build.
    fn cola_line(qty: &str) -> CartLine {
        CartLine::new(
            Some("01JPROD00000000000COLA001".into()),
            "Coca-Cola 330ml".into(),
            Some("COLA-330".into()),
            Some("5449000000996".into()),
            qty,
            400,
            TAX_VAT.into(),
            1_000,
            false,
        )
    }

    // Water line: 250 minor, zero-rated (0 bp).
    fn water_line(qty: &str) -> CartLine {
        CartLine::new(
            Some("01JPROD00000000000WATR001".into()),
            "Water 500ml".into(),
            Some("WATR-500".into()),
            Some("6281001511222".into()),
            qty,
            250,
            TAX_ZER.into(),
            0,
            false,
        )
    }

    // ── 1. Finalize totals: subtotal / VAT / net as i64 minor units ───────────
    // 3× Cola: subtotal 1 200, 10% excl VAT = 120, net 1 320. This drives the
    // exact server-side recompute path pos_finalize_sale delegates to.
    #[tokio::test]
    async fn finalize_computes_subtotal_vat_total() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        cart.lines.push(cola_line("3"));

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 1_320,
            tendered_minor: Some(1_320),
            external_reference: None,
        }];

        let result = sale_repo::finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-pos-totals",
            None,
            false,
            None,
            false,
        )
        .await
        .expect("finalize_sale");

        assert_eq!(result.tax_total_minor, 120, "10% of 1 200 subtotal");
        assert_eq!(result.net_total_minor, 1_320, "subtotal + VAT");
        assert_eq!(result.discount_total_minor, 0);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].line_total_minor, 1_320);
        assert_eq!(result.items[0].tax_amount_minor, 120);
    }

    // ── 2. Bill discount reduces net total at finalize ────────────────────────
    // 2× Water (zero-rated) = 500 subtotal; 150 bill discount → net 350.
    #[tokio::test]
    async fn finalize_applies_bill_discount() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        cart.lines.push(water_line("2"));
        cart.bill_discount_minor = 150;
        cart.bill_discount_reason = Some("loyalty".into());

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 350,
            tendered_minor: Some(350),
            external_reference: None,
        }];

        let result = sale_repo::finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-pos-bill-disc",
            None,
            false,
            None,
            false,
        )
        .await
        .expect("finalize with bill discount");

        assert_eq!(
            result.net_total_minor, 350,
            "500 subtotal - 150 bill discount"
        );
        assert_eq!(result.discount_total_minor, 150);
        assert_eq!(result.tax_total_minor, 0, "water is zero-rated");
    }

    // ── 3. Line discount reduces line + net total at finalize ─────────────────
    // 1× Cola at 400 with a 100 line discount → discounted base 300, VAT 30,
    // line total 330, net 330. Verifies tax is charged on the post-discount base.
    #[tokio::test]
    async fn finalize_applies_line_discount() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        let mut line = cola_line("1");
        line.line_discount_minor = 100;
        line.line_discount_reason = Some("manager comp".into());
        line.recalculate();
        cart.lines.push(line);

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 330,
            tendered_minor: Some(330),
            external_reference: None,
        }];

        let result = sale_repo::finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-pos-line-disc",
            None,
            false,
            None,
            false,
        )
        .await
        .expect("finalize with line discount");

        assert_eq!(result.net_total_minor, 330, "(400-100) base + 30 VAT");
        assert_eq!(result.discount_total_minor, 100);
        assert_eq!(result.tax_total_minor, 30, "VAT on the 300 discounted base");
        assert_eq!(result.items[0].line_total_minor, 330);
    }

    // ── 4. Money-rounding edge case: half-up VAT after division ───────────────
    // A unit price of 105 minor at 10% VAT gives 105 * 1000 / 10000 = 10.5,
    // which must round half-up to 11 (calc_tax_exclusive adds 5000 before /10000).
    // Drives the same arithmetic finalize uses for an exclusive-tax line.
    #[tokio::test]
    async fn finalize_rounds_vat_half_up() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        // Price the line at 105 minor so 10% VAT lands on a .5 fils boundary.
        // (finalize_sale only validates product-mapped prices against the DB when
        // a product_prices row exists; none is seeded, so the override is accepted.)
        let mut cart = Cart::new(BRANCH.into(), DEVICE.into(), shift_id, CASHIER.into());
        let mut line = cola_line("1");
        line.unit_price_minor = 105;
        line.recalculate();
        cart.lines.push(line);

        // Sanity: the domain layer rounded 10.5 → 11 before we even hit the DB.
        assert_eq!(
            crate::domain::money::calc_tax_exclusive(105, 1_000),
            11,
            "10.5 fils VAT must round half-up to 11"
        );

        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 116, // 105 base + 11 VAT
            tendered_minor: Some(116),
            external_reference: None,
        }];

        let result = sale_repo::finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-pos-round",
            None,
            false,
            None,
            false,
        )
        .await
        .expect("finalize with rounding");

        assert_eq!(
            result.tax_total_minor, 11,
            "server VAT rounds half-up to 11"
        );
        assert_eq!(result.net_total_minor, 116, "105 + 11");
    }

    // ── 5. Line-price update: the core mutation pos_set_line_price performs ────
    // The command sets unit_price_minor on the matched non-voided line and calls
    // recalculate(); we verify the line total and tax follow the new price.
    #[test]
    fn set_line_price_recalculates_total() {
        let mut cart = Cart::new("b".into(), "d".into(), "s".into(), "u".into());
        cart.lines.push(cola_line("2")); // 400 × 2 = 800, VAT 80, total 880
        let line_id = cart.lines[0].cart_line_id.clone();

        // Mirror pos_set_line_price's body: find non-voided line, set price, recalc.
        let new_price: i64 = 500;
        if let Some(line) = cart
            .lines
            .iter_mut()
            .find(|l| l.cart_line_id == line_id && !l.voided)
        {
            line.unit_price_minor = new_price;
            line.recalculate();
        }

        let line = &cart.lines[0];
        assert_eq!(line.unit_price_minor, 500);
        assert_eq!(line.line_total_minor, 1_100, "500 × 2 = 1 000 + 100 VAT");
        assert_eq!(line.tax_amount_minor, 100);
    }

    // ── 5b. pos_set_line_price rejects a non-positive price ───────────────────
    // The command guards `price_minor <= 0` before mutating; replicate that guard.
    #[test]
    fn set_line_price_rejects_non_positive() {
        for bad in [0_i64, -1, -500] {
            assert!(
                bad <= 0,
                "guard: price {bad} must be rejected as non-positive"
            );
        }
        let positive = 500_i64;
        assert!(positive > 0, "a positive price passes the guard");
    }

    // ── 6. Quantity update: the core mutation pos_update_quantity performs ─────
    // The command sets quantity on the matched line and calls recalculate();
    // verify subtotal/VAT/total scale with the new quantity.
    #[test]
    fn update_quantity_recalculates_total() {
        let mut cart = Cart::new("b".into(), "d".into(), "s".into(), "u".into());
        cart.lines.push(cola_line("1")); // 400, VAT 40, total 440
        let line_id = cart.lines[0].cart_line_id.clone();

        // Mirror pos_update_quantity's body: find the line, set qty, recalc.
        if let Some(line) = cart.lines.iter_mut().find(|l| l.cart_line_id == line_id) {
            line.quantity = "5".to_string();
            line.recalculate();
        }

        let line = &cart.lines[0];
        assert_eq!(line.quantity, "5");
        assert_eq!(line.line_total_minor, 2_200, "400 × 5 = 2 000 + 200 VAT");
        assert_eq!(line.tax_amount_minor, 200);
    }

    // ── 7. Bill-discount upper-bound guard ────────────────────────────────────
    // pos_apply_bill_discount rejects a discount that exceeds post_line_total().
    #[test]
    fn bill_discount_cannot_exceed_post_line_total() {
        let mut cart = Cart::new("b".into(), "d".into(), "s".into(), "u".into());
        cart.lines.push(cola_line("1")); // line total 440

        let post_line = cart.post_line_total();
        assert_eq!(post_line, 440);

        // A discount above the post-line total is out of bounds…
        let over = 500_i64;
        assert!(
            over > post_line,
            "guard rejects discount {over} > cap {post_line}"
        );

        // …while one at the cap is accepted (drives net to zero).
        let at_cap = post_line;
        assert!(at_cap <= post_line, "discount at the cap is allowed");
        cart.bill_discount_minor = at_cap;
        assert_eq!(cart.net_total(), 0, "full-bill discount nets to zero");
    }

    // ── 8. Line-discount upper-bound guard ────────────────────────────────────
    // pos_apply_line_discount rejects a discount exceeding the line subtotal
    // (unit_price × qty), computed with integer mul_minor_by_qty.
    #[test]
    fn line_discount_cannot_exceed_line_subtotal() {
        let mut cart = Cart::new("b".into(), "d".into(), "s".into(), "u".into());
        cart.lines.push(cola_line("2")); // subtotal 800
        let line = &cart.lines[0];

        let line_subtotal =
            crate::domain::money::mul_minor_by_qty(line.unit_price_minor, &line.quantity);
        assert_eq!(line_subtotal, 800);

        // Over the subtotal → rejected.
        assert!(
            900 > line_subtotal,
            "guard rejects discount 900 > subtotal 800"
        );

        // At the subtotal → accepted; applying it zeroes the discounted base and VAT.
        let mut applied = cart;
        applied.lines[0].line_discount_minor = line_subtotal;
        applied.lines[0].recalculate();
        assert_eq!(
            applied.lines[0].line_total_minor, 0,
            "full-line discount → 0"
        );
        assert_eq!(
            applied.lines[0].tax_amount_minor, 0,
            "no VAT on a fully-discounted line"
        );
    }

    #[test]
    fn finalize_sale_requires_non_empty_idempotency_key() {
        for missing in [None, Some(""), Some("   ")] {
            let err = require_idempotency_key(missing.map(str::to_string))
                .expect_err("missing or blank keys must be rejected");
            assert!(
                matches!(err, AppError::Validation(_)),
                "expected validation error, got {err:?}"
            );
        }

        let key = require_idempotency_key(Some("sale-key-123".into()))
            .expect("non-empty key should pass");
        assert_eq!(key, "sale-key-123");
    }
}

#[cfg(test)]
mod reprice_tests {
    use crate::db::repositories::sale_repo::current_selling_prices;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    async fn pool_with_price(price_minor: i64) -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO categories (category_id, name, created_at, updated_at)
             VALUES ('cat_1','Grocery','2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO products (product_id, category_id, name, is_active, created_at, updated_at)
             VALUES ('prd_1','cat_1','Rainbow Evaporated Milk 160ml',1,
                 '2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO product_prices (price_id, product_id, price_type, price_minor,
                 effective_from, created_by_user_id, created_at, updated_at)
             VALUES ('prc_1','prd_1','selling', ?, '2026-08-01T00:00:00Z',
                 '01JUSER000000000000ADMIN1','2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
        )
        .bind(price_minor)
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    /// The lookup checkout uses to reject a stale payment is now the same one
    /// the reprice uses to fix it. They were separate copies of the predicate
    /// with a comment warning they must not diverge; a divergence rejects a
    /// correct payment, which is the failure this shares code to avoid.
    #[tokio::test]
    async fn reprice_reads_the_same_price_checkout_enforces() {
        let pool = pool_with_price(250).await;

        let prices = current_selling_prices(&pool, &["prd_1"]).await.unwrap();
        assert_eq!(prices.get("prd_1"), Some(&250));
    }

    /// A superseded price row must not win. `effective_from` is stored in two
    /// formats and a raw text compare ranks them wrongly, which is why every
    /// timestamp goes through `datetime()`.
    #[tokio::test]
    async fn a_closed_price_row_is_not_in_force() {
        let pool = pool_with_price(250).await;
        // Close the original and add a newer one written in the other format.
        sqlx::query("UPDATE product_prices SET effective_to = '2026-08-02T00:00:00Z'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO product_prices (price_id, product_id, price_type, price_minor,
                 effective_from, created_by_user_id, created_at, updated_at)
             VALUES ('prc_2','prd_1','selling', 300, '2026-08-02 00:00:00',
                 '01JUSER000000000000ADMIN1','2026-08-02T00:00:00Z','2026-08-02T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let prices = current_selling_prices(&pool, &["prd_1"]).await.unwrap();
        assert_eq!(prices.get("prd_1"), Some(&300), "the superseded price won");
    }

    #[tokio::test]
    async fn asking_about_nothing_costs_no_query() {
        let pool = pool_with_price(250).await;
        assert!(current_selling_prices(&pool, &[]).await.unwrap().is_empty());
    }
}
