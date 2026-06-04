use crate::commands::rbac;
use crate::db::repositories::delivery_repo;
use crate::domain::delivery::{
    CancelDeliveryInput, ConfirmPaymentInput, DeliveryListFilter, DeliveryRow,
    RevertPaymentInput, UpdateDeliveryStatusInput,
};
use crate::errors::AppError;
use crate::sync::outbox;
use crate::AppState;
use tauri::State;

/// After any delivery mutation, re-enqueue the full delivery row so other
/// terminals receive the updated status via the sync worker's push cycle.
async fn sync_delivery_after_update(state: &AppState, delivery_id: &str) {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT d.*, s.receipt_number
         FROM delivery_orders d
         JOIN sales s ON s.sale_id = d.sale_id
         WHERE d.delivery_id = ?",
    )
    .bind(delivery_id)
    .fetch_optional(&state.db)
    .await;

    let active_device: Option<String> = sqlx::query_scalar(
        "SELECT device_id FROM devices WHERE is_active=1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let active_branch: Option<String> = sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active=1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    if let (Ok(Some(r)), Some(device_id), Some(branch_id)) = (row, active_device, active_branch) {
        let _ = outbox::enqueue_delivery_order(
            &state.db,
            &device_id, &branch_id,
            &r.get::<String, _>("delivery_id"),
            &r.get::<String, _>("sale_id"),
            &r.get::<String, _>("receipt_number"),
            r.get::<Option<String>, _>("customer_id").as_deref(),
            r.get::<Option<String>, _>("customer_name").as_deref(),
            &r.get::<String, _>("contact_number"),
            &r.get::<String, _>("address_text"),
            r.get::<Option<String>, _>("house_number").as_deref(),
            r.get::<Option<String>, _>("area").as_deref(),
            r.get::<Option<String>, _>("delivery_note").as_deref(),
            r.get::<Option<String>, _>("delivery_staff_name").as_deref(),
            &r.get::<String, _>("expected_payment_method"),
            &r.get::<String, _>("payment_status"),
            &r.get::<String, _>("delivery_status"),
            r.get::<i64, _>("amount_minor"),
            &r.get::<String, _>("currency"),
            r.get::<Option<String>, _>("paid_confirmed_at").as_deref(),
            &r.get::<String, _>("created_by_user_id"),
            &r.get::<String, _>("created_at"),
            &r.get::<String, _>("updated_at"),
        )
        .await;
    }
}

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
    sync_delivery_after_update(&state, &input.delivery_id).await;

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
    sync_delivery_after_update(&state, &input.delivery_id).await;

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
    sync_delivery_after_update(&state, &input.delivery_id).await;
    Ok(result)
}

#[tauri::command]
pub async fn delivery_revert_payment(
    input: RevertPaymentInput,
    state: State<'_, AppState>,
) -> Result<DeliveryRow, AppError> {
    rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;
    let result = delivery_repo::revert_payment(&state.db, &input).await?;
    sync_delivery_after_update(&state, &input.delivery_id).await;
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
