use tauri::State;
use ulid::Ulid;
use crate::errors::AppError;
use crate::inventory::stock_repo::{self, StockLevel, StockMovementRow};
use crate::AppState;

const BRANCH_ID: &str = "01JBRANCH0000000000000001";

// ── inventory_get_levels ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn inventory_get_levels(
    state: State<'_, AppState>,
) -> Result<Vec<StockLevel>, AppError> {
    stock_repo::get_all_levels(&state.db).await
}

// ── inventory_get_low_stock ───────────────────────────────────────────────────

#[tauri::command]
pub async fn inventory_get_low_stock(
    state: State<'_, AppState>,
) -> Result<Vec<StockLevel>, AppError> {
    stock_repo::get_low_stock(&state.db).await
}

// ── inventory_get_movements ───────────────────────────────────────────────────

#[tauri::command]
pub async fn inventory_get_movements(
    state:      State<'_, AppState>,
    product_id: String,
) -> Result<Vec<StockMovementRow>, AppError> {
    stock_repo::get_movements(&state.db, &product_id).await
}

// ── inventory_receive_stock ───────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct ReceiveStockInput {
    pub product_id:          String,
    pub quantity:            String,   // decimal string
    pub notes:               Option<String>,
    pub received_by_user_id: String,
}

#[tauri::command]
pub async fn inventory_receive_stock(
    input: ReceiveStockInput,
    state: State<'_, AppState>,
) -> Result<StockLevel, AppError> {
    let qty: f64 = input.quantity.parse()
        .map_err(|_| AppError::Validation("Invalid quantity".into()))?;
    if qty <= 0.0 {
        return Err(AppError::Validation("Quantity must be positive".into()));
    }

    let now = chrono::Utc::now().to_rfc3339();

    // Upsert stock_levels
    sqlx::query(
        "INSERT INTO stock_levels (product_id, branch_id, quantity_on_hand, updated_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(product_id, branch_id) DO UPDATE SET
           quantity_on_hand = CAST(CAST(quantity_on_hand AS REAL) + ? AS TEXT),
           updated_at = excluded.updated_at"
    )
    .bind(&input.product_id)
    .bind(BRANCH_ID)
    .bind(&input.quantity)
    .bind(&now)
    .bind(&input.quantity)
    .execute(&state.db)
    .await?;

    // Get new quantity for movement record
    let new_qty: String = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?"
    )
    .bind(&input.product_id)
    .bind(BRANCH_ID)
    .fetch_one(&state.db)
    .await?;

    // Record movement
    sqlx::query(
        "INSERT INTO stock_movements
           (movement_id, product_id, branch_id, movement_type, quantity_delta,
            quantity_after, reference_type, notes, created_by_user_id, created_at)
         VALUES (?, ?, ?, 'receive', ?, ?, 'manual_receive', ?, ?, ?)"
    )
    .bind(Ulid::new().to_string())
    .bind(&input.product_id)
    .bind(BRANCH_ID)
    .bind(&input.quantity)
    .bind(&new_qty)
    .bind(&input.notes)
    .bind(&input.received_by_user_id)
    .bind(&now)
    .execute(&state.db)
    .await?;

    // Return updated level
    let levels = stock_repo::get_all_levels(&state.db).await?;
    levels.into_iter()
        .find(|l| l.product_id == input.product_id)
        .ok_or_else(|| AppError::NotFound(format!("Product {} not found", input.product_id)))
}

// ── inventory_adjust_stock ────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct AdjustStockInput {
    pub product_id:          String,
    pub new_quantity:        String,  // absolute quantity (count correction)
    pub notes:               Option<String>,
    pub adjusted_by_user_id: String,
}

#[tauri::command]
pub async fn inventory_adjust_stock(
    input: AdjustStockInput,
    state: State<'_, AppState>,
) -> Result<StockLevel, AppError> {
    let new_qty: f64 = input.new_quantity.parse()
        .map_err(|_| AppError::Validation("Invalid quantity".into()))?;
    if new_qty < 0.0 {
        return Err(AppError::Validation("Quantity cannot be negative".into()));
    }

    let now = chrono::Utc::now().to_rfc3339();

    // Get old quantity for delta calculation
    let old_qty_str: Option<String> = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?"
    )
    .bind(&input.product_id)
    .bind(BRANCH_ID)
    .fetch_optional(&state.db)
    .await?;

    let old_qty: f64 = old_qty_str.as_deref().unwrap_or("0").parse().unwrap_or(0.0);
    let delta = new_qty - old_qty;
    let delta_str = format!("{}", delta);

    // Upsert stock_levels
    sqlx::query(
        "INSERT INTO stock_levels (product_id, branch_id, quantity_on_hand, updated_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(product_id, branch_id) DO UPDATE SET
           quantity_on_hand = excluded.quantity_on_hand,
           updated_at = excluded.updated_at"
    )
    .bind(&input.product_id)
    .bind(BRANCH_ID)
    .bind(&input.new_quantity)
    .bind(&now)
    .execute(&state.db)
    .await?;

    // Record movement
    sqlx::query(
        "INSERT INTO stock_movements
           (movement_id, product_id, branch_id, movement_type, quantity_delta,
            quantity_after, reference_type, notes, created_by_user_id, created_at)
         VALUES (?, ?, ?, 'adjustment', ?, ?, 'count_correction', ?, ?, ?)"
    )
    .bind(Ulid::new().to_string())
    .bind(&input.product_id)
    .bind(BRANCH_ID)
    .bind(&delta_str)
    .bind(&input.new_quantity)
    .bind(&input.notes)
    .bind(&input.adjusted_by_user_id)
    .bind(&now)
    .execute(&state.db)
    .await?;

    let levels = stock_repo::get_all_levels(&state.db).await?;
    levels.into_iter()
        .find(|l| l.product_id == input.product_id)
        .ok_or_else(|| AppError::NotFound(format!("Product {} not found", input.product_id)))
}
