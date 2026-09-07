use crate::commands::rbac;
use crate::db::repositories::product_repo;
use crate::domain::product::ProductWithPrice;
use crate::errors::AppError;
use crate::AppState;
use tauri::State;

// F-HIGH-04: All product read commands now require any authenticated role.
// Product data (catalog, prices) is legitimately read by cashier/manager/owner.

/// Search products by name/SKU/barcode with cursor pagination.
/// `after_id`: exclusive lower bound on product_id (pass None for first page).
/// `page_size`: max results, capped at 100, defaults to 50.
#[tauri::command]
pub async fn product_search(
    session_token: String,
    query: String,
    after_id: Option<String>,
    page_size: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<ProductWithPrice>, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE)
        .await?;
    let page_size = page_size.unwrap_or(50).min(100);
    product_repo::search_products_paginated(&state.db, &query, after_id.as_deref(), page_size).await
}

#[tauri::command]
pub async fn product_get_by_barcode(
    session_token: String,
    barcode: String,
    state: State<'_, AppState>,
) -> Result<Option<ProductWithPrice>, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE)
        .await?;
    product_repo::get_product_by_barcode(&state.db, &barcode).await
}

// Cursor: the last product_id seen. Pass None/null for the first page.
// Page size — capped at 200, defaults to 50.
#[tauri::command]
pub async fn product_list_all(
    session_token: String,
    after_id: Option<String>,
    page_size: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<ProductWithPrice>, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE)
        .await?;
    let page_size = page_size.unwrap_or(50).min(200);
    product_repo::list_all_active(&state.db, after_id.as_deref(), page_size).await
}
