use crate::commands::rbac;
use crate::db::repositories::held_cart_repo;
use crate::domain::cart::Cart;
use crate::domain::refund::HeldCartSummary;
use crate::errors::AppError;
use crate::AppState;
use tauri::State;

#[derive(serde::Deserialize)]
pub struct HoldCartInput {
    pub cart: Cart,
    pub note: Option<String>,
    pub actor_user_id: String,
}

#[tauri::command]
pub async fn held_cart_save(
    input: HoldCartInput,
    state: State<'_, AppState>,
) -> Result<HeldCartSummary, AppError> {
    rbac::require_any_role(&state.db, &input.actor_user_id).await?;
    held_cart_repo::save_held_cart(&state.db, &input.cart, input.note).await
}

#[tauri::command]
pub async fn held_cart_list(
    actor_user_id: String,
    device_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<HeldCartSummary>, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    held_cart_repo::list_held_carts(&state.db, &device_id).await
}

#[derive(serde::Deserialize)]
pub struct ResumeCartInput {
    pub held_cart_id: String,
    pub shift_id: String,
    pub actor_user_id: String,
}

#[tauri::command]
pub async fn held_cart_resume(
    input: ResumeCartInput,
    state: State<'_, AppState>,
) -> Result<Cart, AppError> {
    rbac::require_any_role(&state.db, &input.actor_user_id).await?;
    held_cart_repo::resume_held_cart(&state.db, &input.held_cart_id, &input.shift_id).await
}

#[tauri::command]
pub async fn held_cart_delete(
    actor_user_id: String,
    held_cart_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    held_cart_repo::delete_held_cart(&state.db, &held_cart_id).await
}
