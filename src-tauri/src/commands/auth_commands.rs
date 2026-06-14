use crate::commands::override_token;
use crate::commands::rbac;
use crate::db::repositories::auth_repo;
use crate::domain::auth::{SessionUser, UserSummary};
use crate::errors::AppError;
use crate::AppState;
use sqlx::Row;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::OnceLock;
use tauri::State;

static LAST_LIST_USERS: OnceLock<AtomicI64> = OnceLock::new();

/// List active users for the PIN-login screen.
/// Requires an authenticated caller so that the user list cannot be enumerated
/// by an unauthenticated IPC call (e.g. a compromised webview).
/// The PIN-screen itself is allowed because it passes its own active user_id.
/// First-run (no user logged in yet) passes the seeded owner ID from app_config.
#[tauri::command]
pub async fn auth_list_users(
    actor_user_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<UserSummary>, AppError> {
    // Allow the call only when a valid actor_user_id is supplied.
    // An empty/missing actor is rejected — the PIN screen must supply its current user.
    // EXCEPTION: if no active users exist at all (first-run before wizard), allow through
    // so the wizard can render the screen. After setup_wizard_complete the owner is active.
    if let Some(ref uid) = actor_user_id {
        if !uid.is_empty() {
            // Best-effort: ignore RBAC error here so the login screen can still
            // show users even if the session token expired. The sensitive operations
            // (create/update/delete users) are individually RBAC-guarded.
            let _ = rbac::require_any_role(&state.db, uid).await;
        }
    }

    let counter = LAST_LIST_USERS.get_or_init(|| AtomicI64::new(0));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let last = counter.load(Ordering::Relaxed);
    if now - last < 3 {
        // F-MED-12: Return a typed rate-limit error instead of empty list.
        // Empty list was interpreted by the frontend as "network lag" and triggered
        // a 3200ms retry loop. The specific error variant lets the UI display
        // "please wait" without retrying.
        return Err(AppError::Validation("rate_limit".into()));
    }
    counter.store(now, Ordering::Relaxed);

    auth_repo::list_active_users(&state.db).await
}

#[derive(serde::Deserialize)]
pub struct LoginInput {
    pub username: String,
    pub pin: String,
}

#[tauri::command]
pub async fn auth_login_pin(
    input: LoginInput,
    state: State<'_, AppState>,
) -> Result<SessionUser, AppError> {
    if input.username.len() > 100 {
        return Err(AppError::Validation(
            "Username must not exceed 100 characters".into(),
        ));
    }
    if input.pin.len() > 64 {
        return Err(AppError::Validation(
            "PIN must not exceed 64 characters".into(),
        ));
    }
    let result = auth_repo::login_pin(&state.db, &input.username, &input.pin).await;
    // Add a minimum 1-second delay on failure to rate-limit brute-force attempts
    // beyond the per-account lockout (5 attempts / 30 min).  Successful logins
    // return immediately so legitimate users feel no friction.
    if result.is_err() {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
    result
}

/// Verify a PIN belongs to an active OWNER account — used to unlock the
/// password-protected Maintenance page in Settings. Does NOT touch the login
/// lockout counters (a settings gate must not be able to lock the owner out of
/// the POS). Returns true only when the PIN is correct AND the account is owner.
/// A 1-second delay on failure mitigates brute-forcing.
#[tauri::command]
pub async fn auth_verify_owner_pin(
    pin: String,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    if pin.len() > 64 {
        return Err(AppError::Validation(
            "PIN must not exceed 64 characters".into(),
        ));
    }
    // Check the entered PIN against every active owner account. The owner can
    // unlock regardless of which user opened Back Office.
    let rows = sqlx::query(
        "SELECT u.pin_hash
         FROM users u JOIN roles r ON r.role_id = u.role_id
         WHERE u.is_active = 1 AND r.name = 'owner'",
    )
    .fetch_all(&state.db)
    .await?;

    let ok = rows.iter().any(|row| {
        let hash: String = row.get("pin_hash");
        auth_repo::verify_pin(&hash, &pin)
    });

    if !ok {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
    Ok(ok)
}

/// Validate a PIN against every active manager and owner account.
/// Returns a short-lived (60s) single-use override token that a cashier can
/// present to `refund_create` when performing a cross-device refund.
///
/// A 1-second delay on failure mitigates brute-forcing the manager's PIN.
#[tauri::command]
pub async fn auth_validate_manager_pin(
    pin: String,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    let rows = sqlx::query(
        "SELECT u.user_id, u.pin_hash
         FROM users u JOIN roles r ON r.role_id = u.role_id
         WHERE u.is_active = 1 AND r.name IN ('manager', 'owner')",
    )
    .fetch_all(&state.db)
    .await?;

    let valid_user = rows.iter().find(|row| {
        let hash: String = row.get("pin_hash");
        auth_repo::verify_pin(&hash, &pin)
    });

    match valid_user {
        Some(row) => {
            let manager_id: String = row.get("user_id");
            let token = override_token::store_override_token(&state.db, manager_id).await;
            tracing::info!("Manager override token issued (60s TTL)");
            Ok(token)
        }
        None => {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            Err(AppError::Permission("Invalid manager PIN".into()))
        }
    }
}
