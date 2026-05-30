use crate::commands::rbac;
use crate::db::repositories::delivery_repo;
use crate::domain::delivery::{
    CancelDeliveryInput, ConfirmPaymentInput, DeliveryListFilter, DeliveryRow,
    RevertPaymentInput, UpdateDeliveryStatusInput,
};
use crate::errors::AppError;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn delivery_list(
    filter: DeliveryListFilter,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<DeliveryRow>, AppError> {
    rbac::require_role(&state.db, &actor_user_id, &["owner", "manager", "cashier"]).await?;
    delivery_repo::list_deliveries(&state.db, &filter).await
}

#[tauri::command]
pub async fn delivery_get(
    delivery_id: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<DeliveryRow, AppError> {
    rbac::require_role(&state.db, &actor_user_id, &["owner", "manager", "cashier"]).await?;
    delivery_repo::get_delivery(&state.db, &delivery_id).await
}

#[tauri::command]
pub async fn delivery_update_status(
    input: UpdateDeliveryStatusInput,
    state: State<'_, AppState>,
) -> Result<DeliveryRow, AppError> {
    rbac::require_role(
        &state.db,
        &input.actor_user_id,
        &["owner", "manager", "cashier"],
    )
    .await?;
    delivery_repo::update_delivery_status(&state.db, &input).await
}

#[tauri::command]
pub async fn delivery_confirm_payment(
    input: ConfirmPaymentInput,
    state: State<'_, AppState>,
) -> Result<DeliveryRow, AppError> {
    rbac::manager_or_owner(&state.db, &input.confirmed_by_user_id).await?;
    delivery_repo::confirm_payment(&state.db, &input).await
}

#[tauri::command]
pub async fn delivery_cancel(
    input: CancelDeliveryInput,
    state: State<'_, AppState>,
) -> Result<DeliveryRow, AppError> {
    rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;
    delivery_repo::cancel_delivery(&state.db, &input).await
}

#[tauri::command]
pub async fn delivery_revert_payment(
    input: RevertPaymentInput,
    state: State<'_, AppState>,
) -> Result<DeliveryRow, AppError> {
    rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;
    delivery_repo::revert_payment(&state.db, &input).await
}

#[tauri::command]
pub async fn delivery_rider_suggestions(
    branch_id: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, AppError> {
    rbac::require_role(&state.db, &actor_user_id, &["owner", "manager", "cashier"]).await?;
    delivery_repo::rider_suggestions(&state.db, &branch_id).await
}
