use tauri::State;
use crate::domain::cart::Cart;
use crate::domain::refund::HeldCartSummary;
use crate::db::repositories::held_cart_repo;
use crate::errors::AppError;
use crate::AppState;

#[derive(serde::Deserialize)]
pub struct HoldCartInput {
    pub cart: Cart,
    pub note: Option<String>,
}

#[tauri::command]
pub async fn held_cart_save(input: HoldCartInput, state: State<'_, AppState>) -> Result<HeldCartSummary, AppError> {
    held_cart_repo::save_held_cart(&state.db, &input.cart, input.note).await
}

#[tauri::command]
pub async fn held_cart_list(device_id: String, state: State<'_, AppState>) -> Result<Vec<HeldCartSummary>, AppError> {
    held_cart_repo::list_held_carts(&state.db, &device_id).await
}

#[derive(serde::Deserialize)]
pub struct ResumeCartInput {
    pub held_cart_id: String,
    pub shift_id: String,
}

#[tauri::command]
pub async fn held_cart_resume(input: ResumeCartInput, state: State<'_, AppState>) -> Result<Cart, AppError> {
    held_cart_repo::resume_held_cart(&state.db, &input.held_cart_id, &input.shift_id).await
}

#[tauri::command]
pub async fn held_cart_delete(held_cart_id: String, state: State<'_, AppState>) -> Result<(), AppError> {
    held_cart_repo::delete_held_cart(&state.db, &held_cart_id).await
}
