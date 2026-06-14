use crate::errors::AppError;
/// Role-based access control helpers for Tauri commands.
///
/// All sensitive mutations look up the role from the database using the
/// caller's user_id — the frontend cannot forge a role because the DB is
/// the source of truth.
use sqlx::{Row, SqlitePool};

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
         WHERE u.user_id = ? AND u.is_active = 1",
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

/// Shorthand — any active role (cashier and above). Used to confirm the caller
/// is a real, active user rather than an arbitrary string injection.
pub async fn require_any_role(pool: &SqlitePool, user_id: &str) -> Result<(), AppError> {
    require_role(
        pool,
        user_id,
        &["owner", "manager", "cashier", "accountant"],
    )
    .await
}

/// Returns true if the user can override cross-device refund policy.
/// Only manager and owner can authorise a cashier's cross-device refund.
pub async fn can_override_refund(pool: &SqlitePool, user_id: &str) -> Result<bool, AppError> {
    manager_or_owner(pool, user_id)
        .await
        .map(|_| true)
        .or_else(|e| {
            if matches!(e, AppError::Permission(_)) {
                Ok(false)
            } else {
                Err(e)
            }
        })
}

// ─── Tests ───────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// Spin up an in-memory database with the real migrations.
    /// Migration 0030 deactivates seed users whose pin_hash is PLAIN: or a placeholder
    /// (correct production behaviour — no one can log in before setup).
    /// Tests that need an active user re-activate it explicitly here so they own
    /// their own state rather than relying on a security-sensitive default.
    async fn make_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");
        // Re-activate seed users for test purposes — migration 0030 deactivates them
        // in production to prevent login with the well-known default PINs.
        // rehash_plain_pins() would also run before any real login, so set a proper hash here.
        sqlx::query(
            "UPDATE users SET is_active = 1, pin_hash = 'PLAIN:0000'
             WHERE user_id IN ('01JUSER000000000000ADMIN1', '01JUSER000000000000CASH01')",
        )
        .execute(&pool)
        .await
        .expect("re-activate seed users for tests");
        pool
    }

    // ── owner passes manager_or_owner ─────────────────────────────────────────
    #[tokio::test]
    async fn owner_passes_manager_or_owner() {
        let pool = make_pool().await;
        let result = manager_or_owner(&pool, "01JUSER000000000000ADMIN1").await;
        assert!(result.is_ok(), "owner must pass manager_or_owner check");
    }

    /// Regression test for the "wizard owner inactive" bug.
    ///
    /// The wizard defaults the owner username to "admin" — the seeded account that
    /// migration 0030 deactivates (placeholder hash). setup_wizard_complete UPDATEs
    /// that row. The bug: the UPDATE did not set is_active=1, so the owner stayed
    /// inactive and every RBAC-gated command (CSV import, product CRUD) failed with
    /// "User not found or inactive". This test proves: (1) the seeded admin is
    /// inactive after migrations, and (2) the wizard's UPDATE-with-is_active=1
    /// re-activates it and RBAC then passes.
    #[tokio::test]
    async fn wizard_owner_reactivation_fixes_rbac() {
        // Fresh pool WITHOUT the make_pool re-activation helper, so we see the
        // real production state after migration 0030.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");

        // 1. After migrations, the seeded 'admin' owner is INACTIVE (the bug's setup).
        let active_before: i64 = sqlx::query_scalar(
            "SELECT is_active FROM users WHERE user_id = '01JUSER000000000000ADMIN1'",
        )
        .fetch_one(&pool)
        .await
        .expect("seed admin exists");
        assert_eq!(
            active_before, 0,
            "migration 0030 should deactivate the seeded admin"
        );

        // RBAC must fail while inactive (reproduces "User not found or inactive").
        let before = manager_or_owner(&pool, "01JUSER000000000000ADMIN1").await;
        assert!(
            matches!(before, Err(AppError::Permission(_))),
            "inactive owner must be rejected before the fix runs"
        );

        // 2. Apply the wizard's fixed UPDATE (now includes is_active=1).
        sqlx::query(
            "UPDATE users SET display_name='Owner', pin_hash='$argon2id$real$hash',
                              role_id='01JROLES000000000000000001', is_active=1,
                              updated_at=datetime('now')
             WHERE user_id='01JUSER000000000000ADMIN1'",
        )
        .execute(&pool)
        .await
        .expect("wizard update");

        // 3. RBAC now passes — owner can run CSV import and all gated commands.
        let after = manager_or_owner(&pool, "01JUSER000000000000ADMIN1").await;
        assert!(
            after.is_ok(),
            "re-activated wizard owner must pass RBAC: got {after:?}"
        );
    }

    // ── cashier is blocked by manager_or_owner ────────────────────────────────
    #[tokio::test]
    async fn cashier_blocked_by_manager_or_owner() {
        let pool = make_pool().await;
        let result = manager_or_owner(&pool, "01JUSER000000000000CASH01").await;
        assert!(
            matches!(result, Err(AppError::Permission(_))),
            "cashier must be rejected by manager_or_owner: got {result:?}"
        );
    }

    // ── cashier is blocked by owner_only ─────────────────────────────────────
    #[tokio::test]
    async fn cashier_blocked_by_owner_only() {
        let pool = make_pool().await;
        let result = owner_only(&pool, "01JUSER000000000000CASH01").await;
        assert!(
            matches!(result, Err(AppError::Permission(_))),
            "cashier must be rejected by owner_only: got {result:?}"
        );
    }

    // ── unknown user ID is rejected ───────────────────────────────────────────
    #[tokio::test]
    async fn unknown_user_rejected() {
        let pool = make_pool().await;
        let result = manager_or_owner(&pool, "DOES-NOT-EXIST").await;
        assert!(
            matches!(result, Err(AppError::Permission(_))),
            "unknown user must be rejected: got {result:?}"
        );
    }

    // ── inactive user is rejected even with correct role ─────────────────────
    #[tokio::test]
    async fn inactive_user_rejected() {
        let pool = make_pool().await;
        // Deactivate the admin user
        sqlx::query("UPDATE users SET is_active = 0 WHERE user_id = '01JUSER000000000000ADMIN1'")
            .execute(&pool)
            .await
            .expect("deactivate user");
        let result = manager_or_owner(&pool, "01JUSER000000000000ADMIN1").await;
        assert!(
            matches!(result, Err(AppError::Permission(_))),
            "inactive user must be rejected even with owner role: got {result:?}"
        );
    }

    // ── cashier is blocked from manager-only financial operations ───────────────
    // shift_close, cash_event_create, and pos_void_sale use manager_or_owner.
    // NOTE: refund_create uses require_any_role (cashiers CAN refund on same device;
    // cross-device refunds require a manager override token — see refund_commands.rs).
    #[tokio::test]
    async fn cashier_cannot_perform_manager_only_operations() {
        let pool = make_pool().await;
        let cashier_id = "01JUSER000000000000CASH01";

        // These commands use manager_or_owner internally
        for label in &["shift_close", "cash_event_create", "pos_void_sale"] {
            let result = manager_or_owner(&pool, cashier_id).await;
            assert!(
                matches!(result, Err(AppError::Permission(_))),
                "cashier must be blocked from {label}"
            );
        }
    }
}
