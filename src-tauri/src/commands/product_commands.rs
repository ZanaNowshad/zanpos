use crate::commands::rbac;
use crate::db::repositories::product_repo;
use crate::domain::product::ProductWithPrice;
use crate::errors::AppError;
use crate::AppState;
use tauri::State;

// F-HIGH-04: All product read commands now require any authenticated role.
// Product data (catalog, prices) is legitimately read by cashier/manager/owner.

#[tauri::command]
pub async fn product_search(
    actor_user_id: String,
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<ProductWithPrice>, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    let limit = 50;
    product_repo::search_products(&state.db, &query, limit).await
}

#[tauri::command]
pub async fn product_get_by_barcode(
    actor_user_id: String,
    barcode: String,
    state: State<'_, AppState>,
) -> Result<Option<ProductWithPrice>, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    product_repo::get_product_by_barcode(&state.db, &barcode).await
}

#[tauri::command]
pub async fn product_list_all(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ProductWithPrice>, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    product_repo::list_all_active(&state.db).await
}
