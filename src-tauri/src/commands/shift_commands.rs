use crate::commands::rbac;
use crate::db::repositories::shift_repo;
use crate::domain::shift::Shift;
use crate::errors::AppError;
use crate::AppState;
use sqlx::Row;
use tauri::State;

#[tauri::command]
pub async fn shift_get_active(
    device_id: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Option<Shift>, AppError> {
    // Best-effort RBAC: the PIN screen calls this before login, so we allow it through
    // if the actor lookup fails (same pattern as auth_list_users). But if actor resolves
    // to a valid but inactive user, we block — prevents active enumeration by bad actor.
    if !actor_user_id.is_empty() {
        let _ = rbac::require_any_role(&state.db, &actor_user_id).await;
    }
    shift_repo::get_active_shift(&state.db, &device_id).await
}

#[derive(serde::Deserialize)]
pub struct OpenShiftInput {
    pub branch_id: String,
    pub device_id: String,
    pub cashier_user_id: String,
    pub opening_cash_minor: i64,
}

#[tauri::command]
pub async fn shift_open(
    input: OpenShiftInput,
    state: State<'_, AppState>,
) -> Result<Shift, AppError> {
    // Any authenticated user (cashier and above) may open a shift for themselves.
    rbac::require_any_role(&state.db, &input.cashier_user_id).await?;
    if input.opening_cash_minor < 0 {
        return Err(AppError::Validation(
            "Opening cash float cannot be negative".into(),
        ));
    }
    shift_repo::open_shift(
        &state.db,
        &input.branch_id,
        &input.device_id,
        &input.cashier_user_id,
        input.opening_cash_minor,
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct CloseShiftInput {
    pub shift_id: String,
    pub actor_user_id: String,
    pub counted_cash_minor: Option<i64>,
    pub notes: Option<String>,
}

#[tauri::command]
pub async fn shift_close(
    input: CloseShiftInput,
    state: State<'_, AppState>,
) -> Result<Shift, AppError> {
    // A cashier may only close their own shift on their own device.
    // A manager may close any shift on the current device.
    // An owner may close any shift on any device.
    let row = sqlx::query(
        "SELECT cashier_user_id, device_id FROM shifts WHERE shift_id = ? AND status = 'open'",
    )
    .bind(&input.shift_id)
    .fetch_optional(&state.db)
    .await?;

    let row = row.ok_or_else(|| AppError::NotFound("Shift not found or already closed".into()))?;
    let owner_id: String = row.get("cashier_user_id");
    let shift_device_id: String = row.get("device_id");

    // Resolve the active device_id for this terminal.
    let active_device: String = sqlx::query_scalar(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .flatten()
    .unwrap_or_default();

    if owner_id != input.actor_user_id {
        // Not the owning cashier — must be manager or owner.
        rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;
    }

    if shift_device_id != active_device {
        // Cross-device shift close requires owner-only permission.
        rbac::owner_only(&state.db, &input.actor_user_id).await?;
    }

    // BUG-POS-5: Block shift close if there are pending/dispatched deliveries
    // with unpaid COD. Cash payment on delivery requires an open shift.
    let pending_deliveries: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM delivery_orders
         WHERE shift_id = ?
           AND delivery_status IN ('pending', 'dispatched')
           AND payment_status = 'pending'",
    )
    .bind(&input.shift_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    if pending_deliveries > 0 {
        return Err(AppError::Conflict(format!(
            "Cannot close shift: {} delivery order(s) are still pending payment. \
             Resolve or cancel them before closing.",
            pending_deliveries
        )));
    }

    shift_repo::close_shift(
        &state.db,
        &input.shift_id,
        input.counted_cash_minor,
        input.notes,
    )
    .await
}
