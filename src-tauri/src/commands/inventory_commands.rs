use tauri::State;
use crate::errors::AppError;
use crate::inventory::stock_repo::{self, StockLevel, StockMovementRow};
use crate::AppState;

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
