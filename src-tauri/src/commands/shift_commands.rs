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
    pub opening_cash_minor: i64,
    /// Whose shift this is. Derived here, never named by the caller.
    pub session_token: String,
}

#[tauri::command]
pub async fn shift_open(
    input: OpenShiftInput,
    state: State<'_, AppState>,
) -> Result<Shift, AppError> {
    // Any authenticated till user may open a shift, but only for themselves.
    // The old code said as much in a comment and did not enforce it: the cashier
    // was named in the payload, so a shift — and the cash float it makes someone
    // answerable for — could be opened against any active colleague.
    let actor = rbac::session_actor(
        &state.sessions,
        &state.db,
        &input.session_token,
        rbac::POS_ROLES,
    )
    .await?;
    rbac::require_branch(&actor, &input.branch_id)?;
    if input.opening_cash_minor < 0 {
        return Err(AppError::Validation(
            "Opening cash float cannot be negative".into(),
        ));
    }
    shift_repo::open_shift(
        &state.db,
        &input.branch_id,
        &input.device_id,
        &actor.user_id,
        input.opening_cash_minor,
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct CloseShiftInput {
    pub shift_id: String,
    /// Who is closing the shift. Both "is this my own shift" and "am I senior
    /// enough to close someone else's" are answered from this.
    pub session_token: String,
    pub counted_cash_minor: Option<i64>,
    pub notes: Option<String>,
}

/// Who may close a given shift.
///
/// A cashier may close only their own shift, and only on the device it was
/// opened on. A manager may close anyone's shift on this device. An owner may
/// close any shift anywhere.
///
/// This is a pure decision so it can be tested directly. Every argument is
/// server-derived: the first two come from the resolved session, the last three
/// from the database.
fn may_close_shift(
    actor_user_id: &str,
    actor_role: &str,
    shift_owner_id: &str,
    shift_device_id: &str,
    active_device_id: &str,
) -> Result<(), AppError> {
    let is_own_shift = shift_owner_id == actor_user_id;
    let is_supervisor = matches!(actor_role, "owner" | "manager");

    if !is_own_shift && !is_supervisor {
        return Err(AppError::Permission(
            "Only a manager or owner can close another cashier's shift.".into(),
        ));
    }
    if shift_device_id != active_device_id && actor_role != "owner" {
        return Err(AppError::Permission(
            "Only an owner can close a shift opened on another device.".into(),
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn shift_close(
    input: CloseShiftInput,
    state: State<'_, AppState>,
) -> Result<Shift, AppError> {
    // A cashier may only close their own shift on their own device.
    // A manager may close any shift on the current device.
    // An owner may close any shift on any device.
    //
    // All three tests below compare against the caller, so the caller has to be
    // established before any of them run. While it arrived in the payload the
    // ladder inverted: naming the shift's own cashier made the first test false
    // and skipped the manager check, and naming any owner satisfied the
    // cross-device check — so any till could close any shift, on any device,
    // and the cash count would be filed under someone else's name.
    let actor = rbac::session_actor(
        &state.sessions,
        &state.db,
        &input.session_token,
        rbac::ANY_ROLE,
    )
    .await?;

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

    may_close_shift(
        &actor.user_id,
        &actor.role_name,
        &owner_id,
        &shift_device_id,
        &active_device,
    )?;

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

#[cfg(test)]
mod tests {
    use super::may_close_shift;

    const TILL_1: &str = "device-till-1";
    const TILL_2: &str = "device-till-2";
    const CASHIER: &str = "user-cashier";
    const OTHER_CASHIER: &str = "user-other-cashier";

    #[test]
    fn a_cashier_closes_their_own_shift_on_their_own_till() {
        assert!(may_close_shift(CASHIER, "cashier", CASHIER, TILL_1, TILL_1).is_ok());
    }

    /// The defect this replaced. The old ladder compared the shift's owner
    /// against an id supplied in the payload, so sending the shift's *own*
    /// cashier id made "is this mine" true and skipped the manager check
    /// entirely — any till could close any cashier's shift. The caller is now
    /// the resolved session, which cannot be set to someone else.
    #[test]
    fn a_cashier_cannot_close_another_cashiers_shift() {
        assert!(may_close_shift(CASHIER, "cashier", OTHER_CASHIER, TILL_1, TILL_1).is_err());
    }

    #[test]
    fn a_manager_closes_another_cashiers_shift_on_this_till() {
        assert!(may_close_shift("user-manager", "manager", OTHER_CASHIER, TILL_1, TILL_1).is_ok());
    }

    /// The second half of the same inversion: naming any owner satisfied the
    /// cross-device check, so a shift opened on another till could be closed
    /// from this one and its cash count filed here.
    #[test]
    fn only_an_owner_reaches_a_shift_opened_on_another_till() {
        assert!(may_close_shift("user-manager", "manager", OTHER_CASHIER, TILL_2, TILL_1).is_err());
        assert!(may_close_shift(CASHIER, "cashier", CASHIER, TILL_2, TILL_1).is_err());
        assert!(may_close_shift("user-owner", "owner", OTHER_CASHIER, TILL_2, TILL_1).is_ok());
    }

    /// An accountant is an active account with no till duties. Being signed in
    /// is not the same as being allowed to close someone's cash drawer.
    #[test]
    fn an_active_account_without_till_duties_closes_nothing_of_anyone_elses() {
        assert!(may_close_shift("user-accountant", "accountant", OTHER_CASHIER, TILL_1, TILL_1)
            .is_err());
    }
}
