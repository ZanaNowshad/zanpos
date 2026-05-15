use tauri::State;
use ulid::Ulid;
use crate::domain::cart::{Cart, CartLine};
use crate::domain::sale::{PaymentInput, SaleResult};
use crate::db::repositories::{product_repo, sale_repo};
use crate::errors::AppError;
use crate::AppState;

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

#[tauri::command]
pub async fn pos_add_item(
    input: AddItemInput,
    state: State<'_, AppState>,
) -> Result<Cart, AppError> {
    let product = product_repo::get_product_by_id(&state.db, &input.product_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Product {} not found", input.product_id)))?;

    let qty = input.quantity.as_deref().unwrap_or("1");
    let mut cart = input.cart;
    let line = CartLine::new(
        Some(product.product.product_id),
        product.product.name,
        product.product.sku,
        product.product.barcode,
        qty,
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
    let qty: f64 = input.quantity.parse().map_err(|_| AppError::Validation("Invalid quantity".into()))?;
    if qty <= 0.0 {
        return Err(AppError::Validation("Quantity must be positive".into()));
    }
    let mut cart = input.cart;
    if let Some(line) = cart.lines.iter_mut().find(|l| l.cart_line_id == input.cart_line_id) {
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
}

#[tauri::command]
pub async fn pos_finalize_sale(
    input: FinalizeSaleInput,
    state: State<'_, AppState>,
) -> Result<SaleResult, AppError> {
    let key = input.idempotency_key.unwrap_or_else(|| Ulid::new().to_string());
    sale_repo::finalize_sale(&state.db, &input.cart, input.payments, &key).await
}

// ─── Discount commands ────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct ApplyBillDiscountInput {
    pub cart: Cart,
    pub discount_minor: i64,
}

#[tauri::command]
pub async fn pos_apply_bill_discount(input: ApplyBillDiscountInput) -> Result<Cart, AppError> {
    let mut cart = input.cart;
    cart.bill_discount_minor = input.discount_minor.max(0);
    Ok(cart)
}

#[derive(serde::Deserialize)]
pub struct ApplyLineDiscountInput {
    pub cart: Cart,
    pub cart_line_id: String,
    pub discount_minor: i64,
}

#[tauri::command]
pub async fn pos_apply_line_discount(input: ApplyLineDiscountInput) -> Result<Cart, AppError> {
    let mut cart = input.cart;
    if let Some(line) = cart.lines.iter_mut().find(|l| l.cart_line_id == input.cart_line_id) {
        line.line_discount_minor = input.discount_minor.max(0);
        line.recalculate();
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
    if let Some(line) = cart.lines.iter_mut().find(|l| l.cart_line_id == input.cart_line_id) {
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
    let qty_f: f64 = qty.parse().map_err(|_| AppError::Validation("Invalid quantity".into()))?;
    if qty_f <= 0.0 {
        return Err(AppError::Validation("Quantity must be positive".into()));
    }
    let mut cart = input.cart;
    let line = CartLine::new(
        None,               // no product_id — open item
        input.name,
        None,               // no sku
        None,               // no barcode
        qty,
        input.price_minor,
        String::new(),      // no tax rule
        0,                  // 0 basis points = tax-exempt
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
    let now = chrono::Utc::now().to_rfc3339();
    let affected = sqlx::query(
        "UPDATE sales SET status = 'voided', updated_at = ?
         WHERE sale_id = ? AND status = 'completed'"
    )
    .bind(&now)
    .bind(&sale_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(
            "Sale not found or already voided".into()
        ));
    }

    // Record in audit log — best-effort, non-fatal
    let _ = sqlx::query(
        "INSERT INTO audit_log (log_id, entity_type, entity_id, action, actor_user_id, created_at)
         VALUES (?,?,?,?,?,?)"
    )
    .bind(ulid::Ulid::new().to_string())
    .bind("sale")
    .bind(&sale_id)
    .bind("void")
    .bind(&voided_by_user_id)
    .bind(&now)
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
