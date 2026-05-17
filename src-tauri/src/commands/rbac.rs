/// Role-based access control helpers for Tauri commands.
///
/// All sensitive mutations look up the role from the database using the
/// caller's user_id — the frontend cannot forge a role because the DB is
/// the source of truth.
use sqlx::{SqlitePool, Row};
use crate::errors::AppError;

/// Verify that `actor_user_id` holds one of `allowed_roles`.
/// Returns `Ok(())` on success, `Err(AppError::Permission)` on failure.
pub async fn require_role(
    pool: &SqlitePool,
    actor_user_id: &str,
    allowed_roles: &[&str],
) -> Result<(), AppError> {
    let row = sqlx::query(
        "SELECT r.name as role_name
         FROM users u
         JOIN roles r ON r.role_id = u.role_id
         WHERE u.user_id = ? AND u.is_active = 1"
    )
    .bind(actor_user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Internal(format!("RBAC lookup failed: {e}")))?;

    let role_name: String = row
        .ok_or_else(|| AppError::Permission("User not found or inactive".into()))?
        .get("role_name");

    if allowed_roles.contains(&role_name.as_str()) {
        Ok(())
    } else {
        Err(AppError::Permission(format!(
            "Role '{}' is not permitted for this action. Required: {:?}",
            role_name, allowed_roles
        )))
    }
}

/// Shorthand — owner only.
pub async fn owner_only(pool: &SqlitePool, user_id: &str) -> Result<(), AppError> {
    require_role(pool, user_id, &["owner"]).await
}

/// Shorthand — owner or manager.
pub async fn manager_or_owner(pool: &SqlitePool, user_id: &str) -> Result<(), AppError> {
    require_role(pool, user_id, &["owner", "manager"]).await
}
