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
    // The PIN screen calls this before anyone has logged in, so an unknown actor
    // is allowed through — same as `auth_list_users`. A *known but deactivated*
    // account is not: that is a former employee's id still being used.
    //
    // This is what the comment here always claimed to do. The code said
    // `let _ = rbac::require_any_role(...)`, which throws the result away, so
    // nothing was ever blocked. `require_any_role` cannot be used directly for
    // the distinction either — its query filters on `is_active = 1`, so a
    // deactivated user and a nonexistent one produce the same error. Asking
    // whether the row exists at all is what separates them.
    if !actor_user_id.is_empty() {
        let is_deactivated: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM users WHERE user_id = ? AND is_active = 0)",
        )
        .bind(&actor_user_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(false);
        if is_deactivated {
            return Err(AppError::Permission(
                "This account has been deactivated.".into(),
            ));
        }
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
    //
    // This guard had never once fired. It filtered on `delivery_orders.shift_id`,
    // a column that has never existed on that table — a delivery reaches its
    // shift through the sale it belongs to. SQLite answered "no such column",
    // and the `.unwrap_or(0)` below read that error as "nothing pending". So the
    // one check standing between an open till and cash arriving after close was
    // reporting all-clear by failing.
    //
    // Both `delivery_orders.sale_id` and `sales.shift_id` are NOT NULL, so the
    // join is total: no delivery can escape the count by having no sale.
    let pending_deliveries: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM delivery_orders d
           JOIN sales s ON s.sale_id = d.sale_id
          WHERE s.shift_id = ?
            AND d.delivery_status IN ('pending', 'dispatched')
            AND d.payment_status = 'pending'",
    )
    .bind(&input.shift_id)
    .fetch_one(&state.db)
    .await
    // Still tolerant of a genuine database error — but the query it tolerates is
    // now one that can succeed.
    .unwrap_or(0);

    if pending_deliveries > 0 {
        return Err(AppError::Conflict(format!(
            "Cannot close shift: {} delivery order(s) are still pending payment. \
             Resolve or cancel them before closing.",
            pending_deliveries
        )));
    }

    let closed = shift_repo::close_shift(
        &state.db,
        &input.shift_id,
        input.counted_cash_minor,
        input.notes,
    )
    .await?;

    crate::diagnostics::record_event(&state.db, "eod_completed", None).await;

    // Evening digest: best-effort, deduped per business day inside the
    // digest module itself — never blocks or fails the shift close.
    crate::digest::maybe_send_evening_digest(&state, &input.shift_id).await;

    Ok(closed)
}
