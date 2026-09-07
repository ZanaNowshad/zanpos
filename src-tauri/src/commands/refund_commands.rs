use crate::commands::override_token;
use crate::commands::rbac;
use crate::db::repositories::{audit_hash, refund_repo};
use crate::domain::refund::{RefundItemInput, RefundResult, SaleForRefund};
use crate::domain::sale::SaleResult;
use crate::errors::AppError;
use crate::AppState;
use tauri::State;

/// Reason code validation mirroring refund_repo::create_refund guard.
/// `pub` so unit tests can reach it; only called at test-time.
#[cfg_attr(not(test), allow(dead_code))]
pub fn sanitise_reason_code(code: &str) -> &str {
    match code {
        "customer_return" | "defective" | "wrong_item" | "exchange" | "other" => code,
        _ => "other",
    }
}

#[tauri::command]
pub async fn refund_get_sale(
    receipt_number: String,
    session_token: String,
    state: State<'_, AppState>,
) -> Result<SaleForRefund, AppError> {
    // Any active user may look up a sale by receipt number for refund purposes.
    // The cross-device refund policy is enforced in refund_create, not here.
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE).await?;
    refund_repo::get_sale_by_receipt(&state.db, &receipt_number).await
}

#[tauri::command]
pub async fn receipt_reprint(
    receipt_number: String,
    session_token: String,
    state: State<'_, AppState>,
) -> Result<SaleResult, AppError> {
    // Any till user may reprint a receipt, but the audit entry below names who
    // did — so that name comes from the session rather than the payload.
    let actor =
        rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::POS_ROLES).await?;
    let requesting_user_id = actor.user_id;
    let sale = refund_repo::get_sale_result_by_receipt(&state.db, &receipt_number).await
        .map_err(|e| {
            if matches!(e, AppError::NotFound(_)) {
                AppError::NotFound(
                    "Receipt not found. Sales older than 90 days may have been pruned from this device.".into()
                )
            } else {
                e
            }
        })?;

    // A reprint is a read, but it is not nothing.
    //
    // Duplicate receipts are how a returned item gets refunded twice, and the
    // reprint itself left no trace at all — the till could not answer "who
    // printed this, and how many times". The row records the reprint; it does
    // not touch the sale, mint a receipt number or take a payment, which is what
    // makes a reprint safe to repeat after a paper jam.
    //
    // Best-effort on purpose. A cashier standing at the counter with a jammed
    // printer must not be refused their receipt because the audit write failed;
    // the failure is logged rather than raised.
    let (device_id, branch_id) = sale_origin(&state.db, &sale.sale_id).await;
    if let Err(error) = audit_hash::insert_audit_entry_override(
        &state.db,
        "sale.receipt_reprinted",
        "sale",
        &sale.sale_id,
        &requesting_user_id,
        "user",
        &device_id,
        &branch_id,
        None,
        Some(&format!("{{\"receipt_number\":\"{receipt_number}\"}}")),
        None,
        false,
    )
    .await
    {
        tracing::warn!("receipt {receipt_number} reprinted but not audited: {error}");
    }

    Ok(sale)
}

/// Where a sale was rung up, for the audit entry's device and branch columns.
async fn sale_origin(pool: &sqlx::SqlitePool, sale_id: &str) -> (String, String) {
    sqlx::query_as::<_, (String, String)>(
        "SELECT device_id, branch_id FROM sales WHERE sale_id = ?",
    )
    .bind(sale_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_default()
}

#[derive(serde::Deserialize)]
pub struct CreateRefundInput {
    pub original_sale_id: String,
    pub items: Vec<RefundItemInput>,
    pub reason: String,
    /// Structured reason code: customer_return | defective | wrong_item | exchange | other
    pub return_reason_code: Option<String>,
    /// Proves who is issuing the refund. The refunding user and their role are
    /// both derived from this; nothing in the payload names either.
    pub session_token: String,
    /// Short-lived token from auth_validate_manager_pin. Required when a cashier
    /// attempts a cross-device refund. Manager/owner cross-device refunds do not
    /// need this token (their role is checked directly).
    pub manager_override_token: Option<String>,
    /// Stable for one refund attempt, so a retry after a timeout is answered
    /// with the reversal already made instead of paying the customer twice.
    /// The refund screen mints it once and keeps it across retries, the same way
    /// the cart id serves a sale.
    pub idempotency_key: Option<String>,
}

/// Resolve the current device id from app_config. Returns empty string if not configured.
async fn current_device_id(pool: &sqlx::SqlitePool) -> String {
    sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'device_id'")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten()
        .unwrap_or_default()
}

#[tauri::command]
pub async fn refund_create(
    input: CreateRefundInput,
    state: State<'_, AppState>,
) -> Result<RefundResult, AppError> {
    // Step 1 — Authenticate the caller from their session.
    // RBAC DECISION: cashiers ARE permitted to initiate refunds on the same device
    // (same-device refund is a standard POS workflow — cashier returns a just-sold item).
    // Cross-device refunds require a manager override token (enforced in Step 4 below).
    // Manager/owner can refund cross-device without a token (their role is checked
    // directly). Every active role is accepted here; the policy that matters is in
    // Step 4.
    //
    // The identity and the role must both come from the session. When the payload
    // named the refunding user, Step 2 asked whether *that* id was a manager — so a
    // cashier who put a manager's user id in the field was granted the manager path
    // and could issue a cross-device refund with no override token, against an
    // account that was not theirs.
    let actor = rbac::session_actor(
        &state.sessions,
        &state.db,
        &input.session_token,
        rbac::ANY_ROLE,
    )
    .await?;
    let created_by_user_id = actor.user_id.clone();

    // Step 2 — Check if the *authenticated* user is manager/owner.
    let user_is_manager = matches!(actor.role_name.as_str(), "owner" | "manager");
    let reason_code = input.return_reason_code.as_deref().unwrap_or("other");

    // Step 3 — Resolve the current device id and compare with the sale's origin.
    let device_id = current_device_id(&state.db).await;
    let sale_device: Option<String> =
        sqlx::query_scalar("SELECT origin_device_id FROM sales WHERE sale_id = ?")
            .bind(&input.original_sale_id)
            .fetch_optional(&state.db)
            .await?
            .flatten();

    let is_cross_device = sale_device.as_deref().is_some_and(|sd| sd != device_id);

    // Step 4 — Enforce cross-device refund policy.
    let override_used = if is_cross_device {
        if user_is_manager {
            // Manager/owner refunding cross-device — allowed, logged with override.
            true
        } else {
            // Cashier refunding cross-device — manager PIN override required.
            let token = input.manager_override_token.as_deref().ok_or_else(|| {
                AppError::Permission(
                    "Cross-device refund requires manager override. Enter manager PIN.".into(),
                )
            })?;
            let _manager_id = override_token::consume_override_token(&state.db, token)
                .await
                .ok_or_else(|| {
                    AppError::Permission(
                        "Invalid or expired manager override token. Please re-enter manager PIN."
                            .into(),
                    )
                })?;
            true
        }
    } else {
        false
    };

    refund_repo::create_refund(
        &state.db,
        &input.original_sale_id,
        input.items,
        &input.reason,
        reason_code,
        &created_by_user_id,
        override_used,
        input.idempotency_key,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_reason_codes_pass_through() {
        assert_eq!(sanitise_reason_code("customer_return"), "customer_return");
        assert_eq!(sanitise_reason_code("defective"), "defective");
        assert_eq!(sanitise_reason_code("wrong_item"), "wrong_item");
        assert_eq!(sanitise_reason_code("exchange"), "exchange");
        assert_eq!(sanitise_reason_code("other"), "other");
    }

    #[test]
    fn unknown_reason_code_falls_back_to_other() {
        assert_eq!(sanitise_reason_code("SCAM"), "other");
        assert_eq!(sanitise_reason_code(""), "other");
        assert_eq!(sanitise_reason_code("undefined"), "other");
    }
}
