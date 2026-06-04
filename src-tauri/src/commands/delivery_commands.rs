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

    let active_branch: String = sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .unwrap_or_default();

    delivery_repo::list_deliveries(&state.db, &active_branch, &filter).await
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
    let result = delivery_repo::update_delivery_status(&state.db, &input).await?;

    // Trigger WhatsApp notification on status transitions
    let exp = super::setup_commands::currency_exponent(&result.currency);
    match input.delivery_status.as_str() {
        "dispatched" => {
            let _ = super::whatsapp_commands::whatsapp_send_delivery_impl(
                &state,
                &result.contact_number,
                &result.receipt_number,
                result.amount_minor,
                exp,
                &result.address_text,
                result.house_number.as_deref(),
                result.area.as_deref(),
                None,
            )
            .await;
        }
        "delivered" => {
            let _ = super::whatsapp_commands::whatsapp_notify_arrival_impl(
                &state,
                &result.contact_number,
                &result.receipt_number,
            )
            .await;
        }
        _ => {}
    }

    Ok(result)
}

#[tauri::command]
pub async fn delivery_confirm_payment(
    input: ConfirmPaymentInput,
    state: State<'_, AppState>,
) -> Result<DeliveryRow, AppError> {
    rbac::manager_or_owner(&state.db, &input.confirmed_by_user_id).await?;
    let result = delivery_repo::confirm_payment(&state.db, &input).await?;

    // Trigger WhatsApp payment reminder
    let exp = super::setup_commands::currency_exponent(&result.currency);
    let _ = super::whatsapp_commands::whatsapp_payment_reminder_impl(
        &state,
        &result.contact_number,
        &result.receipt_number,
        result.amount_minor,
        exp,
        &result.currency,
    )
    .await;

    Ok(result)
}

#[tauri::command]
pub async fn delivery_cancel(
    input: CancelDeliveryInput,
    state: State<'_, AppState>,
) -> Result<DeliveryRow, AppError> {
    rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;
    let result = delivery_repo::cancel_delivery(&state.db, &input).await?;
    Ok(result)
}

#[tauri::command]
pub async fn delivery_revert_payment(
    input: RevertPaymentInput,
    state: State<'_, AppState>,
) -> Result<DeliveryRow, AppError> {
    rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;
    let result = delivery_repo::revert_payment(&state.db, &input).await?;
    Ok(result)
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
