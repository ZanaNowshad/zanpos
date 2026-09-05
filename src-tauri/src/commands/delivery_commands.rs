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
    session_token: String,
    state: State<'_, AppState>,
) -> Result<Vec<DeliveryRow>, AppError> {
    let actor =
        rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::POS_ROLES).await?;

    // Delivery rows carry customer contact numbers and receipt references, so the
    // branch scope comes from the actor's own record. `filter.branch_id` is
    // caller-supplied and was previously preferred over any lookup, which let an
    // authenticated user read another branch's deliveries by changing one field.
    //
    // The previous fallback also defaulted to an empty string when no active
    // branch existed, which matched no rows silently rather than failing.
    // The branch now comes back with the authenticated actor, so this is no
    // longer a lookup keyed on an id the caller chose — and it is one query
    // fewer.
    let branch_id = actor.branch_id;

    delivery_repo::list_deliveries(&state.db, &branch_id, &filter).await
}

#[tauri::command]
pub async fn delivery_get(
    delivery_id: String,
    session_token: String,
    state: State<'_, AppState>,
) -> Result<DeliveryRow, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::POS_ROLES).await?;
    delivery_repo::get_delivery(&state.db, &delivery_id).await
}

#[tauri::command]
pub async fn delivery_update_status(
    input: UpdateDeliveryStatusInput,
    session_token: String,
    state: State<'_, AppState>,
) -> Result<DeliveryRow, AppError> {
    // Cancellation via status update requires manager/owner — same as delivery_cancel
    let roles = if input.delivery_status == "cancelled" {
        // Cancelling through a status change is still a cancellation.
        rbac::MANAGER_OR_OWNER
    } else {
        rbac::POS_ROLES
    };
    let actor =
        rbac::session_actor(&state.sessions, &state.db, &session_token, roles).await?;
    let result = delivery_repo::update_delivery_status(&state.db, &actor.user_id, &input).await?;
    // WhatsApp messages are NEVER sent automatically on status change.
    // They are only sent when the cashier explicitly presses the dedicated
    // "Call & Notify" or "Payment Reminder" buttons in the UI.
    // (Previous auto-send caused bulk messages on POS restart — removed.)
    Ok(result)
}

#[tauri::command]
pub async fn delivery_confirm_payment(
    input: ConfirmPaymentInput,
    session_token: String,
    state: State<'_, AppState>,
) -> Result<DeliveryRow, AppError> {
    // Who confirmed the cash is written to `paid_confirmed_by_user_id`, so it
    // has to be the authenticated caller. It used to be whatever id the payload
    // named, which made the record of who took the money forgeable.
    let actor = rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    let result = delivery_repo::confirm_payment(&state.db, &actor.user_id, &input).await?;
    Ok(result)
}

#[tauri::command]
pub async fn delivery_cancel(
    input: CancelDeliveryInput,
    session_token: String,
    state: State<'_, AppState>,
) -> Result<DeliveryRow, AppError> {
    let actor = rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    let result = delivery_repo::cancel_delivery(&state.db, &actor.user_id, &input).await?;
    Ok(result)
}

#[tauri::command]
pub async fn delivery_revert_payment(
    input: RevertPaymentInput,
    session_token: String,
    state: State<'_, AppState>,
) -> Result<DeliveryRow, AppError> {
    let actor = rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    let result = delivery_repo::revert_payment(&state.db, &actor.user_id, &input).await?;
    Ok(result)
}

#[tauri::command]
pub async fn delivery_rider_suggestions(
    branch_id: String,
    session_token: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, AppError> {
    let actor =
        rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::POS_ROLES).await?;
    // Caller-supplied `branch_id` is not trusted for branch-scoped data;
    // the scope comes from the actor's own record. The parameter remains
    // only to preserve the existing invoke contract.
    let _ = branch_id;
    let branch_id = actor.branch_id;
    delivery_repo::rider_suggestions(&state.db, &branch_id).await
}
