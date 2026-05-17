use tauri::State;
use ulid::Ulid;
use sqlx::Row;
use crate::errors::{AppError, AppResult};
use crate::inventory::stock_repo::{self, StockLevel, StockMovementRow};
use crate::commands::rbac;
use crate::AppState;

/// Resolve the active branch_id from the database at runtime.
/// Replaces the old compile-time constant so multi-branch or post-wizard IDs work.
async fn active_branch_id(state: &AppState) -> AppResult<String> {
    let row = sqlx::query(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No active branch configured".into()))?;
    Ok(row.get("branch_id"))
}

/// Resolve the active device_id from the database at runtime.
async fn active_device_id(state: &AppState) -> AppResult<String> {
    let row = sqlx::query(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No active device configured".into()))?;
    Ok(row.get("device_id"))
}

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
    rbac::manager_or_owner(&state.db, &input.received_by_user_id).await?;
    let qty: f64 = input.quantity.parse()
        .map_err(|_| AppError::Validation("Invalid quantity".into()))?;
    if qty <= 0.0 {
        return Err(AppError::Validation("Quantity must be positive".into()));
    }

    let branch_id = active_branch_id(&state).await?;
    let device_id = active_device_id(&state).await?;
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
    .bind(&branch_id)
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
    .bind(&branch_id)
    .fetch_one(&state.db)
    .await?;

    // Record movement
    sqlx::query(
        "INSERT INTO stock_movements
           (movement_id, product_id, branch_id, device_id, movement_type, quantity_delta,
            quantity_after, reference_type, notes, created_by_user_id, created_at)
         VALUES (?, ?, ?, ?, 'receive', ?, ?, 'manual_receive', ?, ?, ?)"
    )
    .bind(Ulid::new().to_string())
    .bind(&input.product_id)
    .bind(&branch_id)
    .bind(&device_id)
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
    rbac::manager_or_owner(&state.db, &input.adjusted_by_user_id).await?;
    let new_qty: f64 = input.new_quantity.parse()
        .map_err(|_| AppError::Validation("Invalid quantity".into()))?;
    if new_qty < 0.0 {
        return Err(AppError::Validation("Quantity cannot be negative".into()));
    }

    let branch_id = active_branch_id(&state).await?;
    let device_id = active_device_id(&state).await?;
    let now = chrono::Utc::now().to_rfc3339();

    // Get old quantity for delta calculation
    let old_qty_str: Option<String> = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?"
    )
    .bind(&input.product_id)
    .bind(&branch_id)
    .fetch_optional(&state.db)
    .await?;

    let old_qty: f64 = old_qty_str.as_deref().unwrap_or("0").parse().unwrap_or(0.0);
    let delta = new_qty - old_qty;
    let delta_str = delta.to_string();

    // Upsert stock_levels
    sqlx::query(
        "INSERT INTO stock_levels (product_id, branch_id, quantity_on_hand, updated_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(product_id, branch_id) DO UPDATE SET
           quantity_on_hand = excluded.quantity_on_hand,
           updated_at = excluded.updated_at"
    )
    .bind(&input.product_id)
    .bind(&branch_id)
    .bind(&input.new_quantity)
    .bind(&now)
    .execute(&state.db)
    .await?;

    // Record movement
    sqlx::query(
        "INSERT INTO stock_movements
           (movement_id, product_id, branch_id, device_id, movement_type, quantity_delta,
            quantity_after, reference_type, notes, created_by_user_id, created_at)
         VALUES (?, ?, ?, ?, 'adjustment', ?, ?, 'count_correction', ?, ?, ?)"
    )
    .bind(Ulid::new().to_string())
    .bind(&input.product_id)
    .bind(&branch_id)
    .bind(&device_id)
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

// ── inventory_bulk_stock_take ─────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct BulkStockTakeEntry {
    pub product_id:   String,
    pub new_quantity: f64,
    pub notes:        Option<String>,
}

#[derive(serde::Serialize)]
pub struct BulkStockTakeResult {
    pub updated: usize,
    pub errors:  Vec<String>,
}

#[tauri::command]
pub async fn inventory_bulk_stock_take(
    entries:         Vec<BulkStockTakeEntry>,
    actor_user_id:   String,
    state:           State<'_, AppState>,
) -> Result<BulkStockTakeResult, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    if entries.is_empty() {
        return Ok(BulkStockTakeResult { updated: 0, errors: vec![] });
    }

    let branch_id = active_branch_id(&state).await?;
    let device_id = active_device_id(&state).await?;
    let now = chrono::Utc::now().to_rfc3339();

    let mut updated = 0usize;
    let mut errors  = Vec::<String>::new();

    for entry in &entries {
        if entry.new_quantity < 0.0 {
            errors.push(format!("Product {}: quantity cannot be negative", entry.product_id));
            continue;
        }

        let new_qty_str = entry.new_quantity.to_string();

        // Get old quantity for delta
        let old_qty_str: Option<String> = sqlx::query_scalar(
            "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?"
        )
        .bind(&entry.product_id)
        .bind(&branch_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

        let old_qty: f64 = old_qty_str.as_deref().unwrap_or("0").parse().unwrap_or(0.0);
        let delta = entry.new_quantity - old_qty;
        let delta_str = delta.to_string();

        // Upsert stock level
        let upsert = sqlx::query(
            "INSERT INTO stock_levels (product_id, branch_id, quantity_on_hand, updated_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(product_id, branch_id) DO UPDATE SET
               quantity_on_hand = excluded.quantity_on_hand,
               updated_at = excluded.updated_at"
        )
        .bind(&entry.product_id)
        .bind(&branch_id)
        .bind(&new_qty_str)
        .bind(&now)
        .execute(&state.db)
        .await;

        if let Err(e) = upsert {
            errors.push(format!("Product {}: {e}", entry.product_id));
            continue;
        }

        // Record stock movement
        let _ = sqlx::query(
            "INSERT INTO stock_movements
               (movement_id, product_id, branch_id, device_id, movement_type, quantity_delta,
                quantity_after, reference_type, notes, created_by_user_id, created_at)
             VALUES (?, ?, ?, ?, 'adjustment', ?, ?, 'stock_take', ?, ?, ?)"
        )
        .bind(Ulid::new().to_string())
        .bind(&entry.product_id)
        .bind(&branch_id)
        .bind(&device_id)
        .bind(&delta_str)
        .bind(&new_qty_str)
        .bind(&entry.notes)
        .bind(&actor_user_id)
        .bind(&now)
        .execute(&state.db)
        .await;

        updated += 1;
    }

    Ok(BulkStockTakeResult { updated, errors })
}
