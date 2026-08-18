use crate::commands::rbac;
use crate::db::repositories::delivery_repo;
use crate::domain::delivery::{
    CancelDeliveryInput, ConfirmPaymentInput, DeliveryListFilter, DeliveryRow, RevertPaymentInput,
    UpdateDeliveryStatusInput,
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

    // Delivery rows carry customer contact numbers and receipt references, so the
    // branch scope comes from the actor's own record. `filter.branch_id` is
    // caller-supplied and was previously preferred over any lookup, which let an
    // authenticated user read another branch's deliveries by changing one field.
    //
    // The previous fallback also defaulted to an empty string when no active
    // branch existed, which matched no rows silently rather than failing.
    let branch_id = rbac::actor_branch_id(&state.db, &actor_user_id).await?;

    delivery_repo::list_deliveries(&state.db, &branch_id, &filter).await
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
    // Cancellation via status update requires manager/owner — same as delivery_cancel
    if input.delivery_status == "cancelled" {
        rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;
    } else {
        rbac::require_role(
            &state.db,
            &input.actor_user_id,
            &["owner", "manager", "cashier"],
        )
        .await?;
    }
    let result = delivery_repo::update_delivery_status(&state.db, &input).await?;
    // WhatsApp messages are NEVER sent automatically on status change.
    // They are only sent when the cashier explicitly presses the dedicated
    // "Call & Notify" or "Payment Reminder" buttons in the UI.
    // (Previous auto-send caused bulk messages on POS restart — removed.)
    Ok(result)
}

#[tauri::command]
pub async fn delivery_confirm_payment(
    input: ConfirmPaymentInput,
    state: State<'_, AppState>,
) -> Result<DeliveryRow, AppError> {
    rbac::manager_or_owner(&state.db, &input.confirmed_by_user_id).await?;
    let result = delivery_repo::confirm_payment(&state.db, &input).await?;
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
    // Caller-supplied `branch_id` is not trusted for branch-scoped data;
    // the scope comes from the actor's own record. The parameter remains
    // only to preserve the existing invoke contract.
    let _ = branch_id;
    let branch_id = rbac::actor_branch_id(&state.db, &actor_user_id).await?;
    delivery_repo::rider_suggestions(&state.db, &branch_id).await
}
