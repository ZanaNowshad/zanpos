use tauri::State;
use crate::db::repositories::product_repo;
use crate::domain::product::ProductWithPrice;
use crate::errors::AppError;
use crate::AppState;

#[tauri::command]
pub async fn product_search(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<ProductWithPrice>, AppError> {
    let limit = 50;
    product_repo::search_products(&state.db, &query, limit).await
}

#[tauri::command]
pub async fn product_get_by_barcode(
    barcode: String,
    state: State<'_, AppState>,
) -> Result<Option<ProductWithPrice>, AppError> {
    product_repo::get_product_by_barcode(&state.db, &barcode).await
}

#[tauri::command]
pub async fn product_list_all(
    state: State<'_, AppState>,
) -> Result<Vec<ProductWithPrice>, AppError> {
    product_repo::list_all_active(&state.db).await
}
