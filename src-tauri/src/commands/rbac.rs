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

// ─── Session-derived authorization ───────────────────────────────────────────
//
// The functions above answer "does this user id hold an allowed role". That is
// not the same question as "is the caller allowed to do this", and the gap
// between them is the whole defect: every one of them is handed an id that
// arrived in the command payload, so a caller who knows an owner's user id is
// authorised as that owner. `auth_list_users` hands out those ids, with roles
// attached, before anyone has logged in.
//
// The functions below close that gap by taking the session token instead. The
// actor comes back from `SessionStore::resolve`, which reads the user id out of
// a session this process issued at login and re-reads branch and role from the
// database. A payload cannot reach any of it.
//
// Prefer these for anything privileged. A payload may still name the *target*
// of an operation — which user to deactivate, whose loyalty to adjust — but it
// must never be the source of who is asking.

/// Roles accepted where any signed-in operator may act. Mirrors [`require_any_role`].
pub const ANY_ROLE: &[&str] = &["owner", "manager", "cashier", "accountant"];
/// Roles that work the till. Mirrors the `["owner", "manager", "cashier"]`
/// literal the delivery and till commands were passing to `require_role`;
/// unlike [`ANY_ROLE`] it excludes accountants, who do not serve customers.
pub const POS_ROLES: &[&str] = &["owner", "manager", "cashier"];
/// Roles accepted for supervisory actions. Mirrors [`manager_or_owner`].
pub const MANAGER_OR_OWNER: &[&str] = &["owner", "manager"];
/// Roles accepted for ownership actions. Mirrors [`owner_only`].
#[allow(dead_code)]
pub const OWNER_ONLY: &[&str] = &["owner"];

/// Authenticate the caller from their session token and require one of
/// `allowed_roles`.
///
/// Returns the trusted actor so the command can use `actor.user_id` and
/// `actor.branch_id` for attribution instead of trusting the payload.
pub async fn session_actor(
    sessions: &crate::auth_session::SessionStore,
    pool: &SqlitePool,
    session_token: &str,
    allowed_roles: &[&str],
) -> Result<crate::auth_session::AuthenticatedActor, AppError> {
    let actor = sessions.resolve(pool, session_token).await?;
    if allowed_roles.contains(&actor.role_name.as_str()) {
        Ok(actor)
    } else {
        Err(AppError::Permission(format!(
            "Role '{}' is not permitted for this action. Required: {:?}",
            actor.role_name, allowed_roles
        )))
    }
}

/// Confirm the authenticated caller may act on `branch_id`.
///
/// A branch id in a payload names which branch is being acted on; it does not
/// establish which branch the caller belongs to. Only an owner may reach
/// outside their own branch.
#[allow(dead_code)] // Used by the authorization invariants; wired in as the branch-scoped families migrate.
pub fn require_branch(
    actor: &crate::auth_session::AuthenticatedActor,
    branch_id: &str,
) -> Result<(), AppError> {
    if actor.branch_id == branch_id || actor.role_name == "owner" {
        Ok(())
    } else {
        Err(AppError::Permission(
            "This account cannot act on another branch.".into(),
        ))
    }
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

    // ── Settings-reachable command policy ─────────────────────────────────────
    //
    // These assert the authorisation each Settings group depends on, exercised
    // through the RBAC helpers directly — i.e. the adversarial path where the
    // frontend is bypassed entirely and a Tauri command is invoked straight.
    // Frontend visibility (settingsConfig.tsx) is not a security control.

    const OWNER: &str = "01JUSER000000000000ADMIN1";
    const CASHIER: &str = "01JUSER000000000000CASH01";

    /// Settings → Store & legal: `settings_update_branch` is owner_only.
    #[tokio::test]
    async fn store_identity_mutation_is_owner_only() {
        let pool = make_pool().await;
        assert!(owner_only(&pool, OWNER).await.is_ok());
        assert!(
            matches!(
                owner_only(&pool, CASHIER).await,
                Err(AppError::Permission(_))
            ),
            "a cashier must not be able to rewrite store identity by calling the command directly"
        );
    }

    /// Settings → Data & sync: `db_backup` is owner_only.
    /// Settings → Advanced: `download_and_install_update` is owner_only.
    #[tokio::test]
    async fn backup_and_update_install_are_owner_only() {
        let pool = make_pool().await;
        assert!(owner_only(&pool, OWNER).await.is_ok());
        assert!(matches!(
            owner_only(&pool, CASHIER).await,
            Err(AppError::Permission(_))
        ));
    }

    /// Settings → Sales & receipts (tax rules, business flags) and
    /// Hardware & printing (printer config, test print) are manager_or_owner.
    #[tokio::test]
    async fn business_and_hardware_mutations_need_manager_or_owner() {
        let pool = make_pool().await;
        assert!(manager_or_owner(&pool, OWNER).await.is_ok());
        assert!(
            matches!(
                manager_or_owner(&pool, CASHIER).await,
                Err(AppError::Permission(_))
            ),
            "a cashier must not change tax rules, business flags or printer configuration"
        );
    }

    /// An unknown actor id is not merely unauthorised — it must not resolve at
    /// all, so a fabricated identifier cannot be used to reach any command.
    #[tokio::test]
    async fn fabricated_actor_id_is_rejected() {
        let pool = make_pool().await;
        for id in ["", "not-a-user", "01JUSER000000000000FAKE01", "' OR 1=1 --"] {
            assert!(
                matches!(
                    manager_or_owner(&pool, id).await,
                    Err(AppError::Permission(_))
                ),
                "fabricated actor id {id:?} must be rejected"
            );
            assert!(matches!(
                owner_only(&pool, id).await,
                Err(AppError::Permission(_))
            ));
        }
    }

    /// Deactivating a user revokes access immediately, without needing the
    /// frontend to notice.
    #[tokio::test]
    async fn deactivated_owner_loses_settings_access() {
        let pool = make_pool().await;
        assert!(owner_only(&pool, OWNER).await.is_ok());
        sqlx::query("UPDATE users SET is_active = 0 WHERE user_id = ?")
            .bind(OWNER)
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            matches!(owner_only(&pool, OWNER).await, Err(AppError::Permission(_))),
            "a deactivated owner must lose access on the next call"
        );
    }

    #[tokio::test]
    async fn actor_branch_id_resolves_from_the_database_only() {
        let pool = make_pool().await;

        // A real, active user resolves to the branch stored on their record.
        let branch = actor_branch_id(&pool, "01JUSER000000000000ADMIN1")
            .await
            .expect("seed admin has a branch");
        assert!(!branch.is_empty());

        // Nothing the caller can send stands in for a real identity. This is the
        // guarantee the report commands rely on when they discard their own
        // `branch_id` argument.
        for forged in ["", "' OR 1=1 --", "01JBRANCH0000000000000001", "unknown"] {
            assert!(
                actor_branch_id(&pool, forged).await.is_err(),
                "{forged:?} must not resolve to a branch"
            );
        }
    }

    #[tokio::test]
    async fn a_deactivated_user_loses_branch_scope() {
        let pool = make_pool().await;
        sqlx::query("UPDATE users SET is_active = 0 WHERE user_id = '01JUSER000000000000ADMIN1'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(actor_branch_id(&pool, "01JUSER000000000000ADMIN1")
            .await
            .is_err());
    }
}

/// The branch an actor belongs to, resolved from the database.
///
/// Commands that take a `branch_id` parameter must not scope their queries with
/// it: the frontend can send any value, so trusting it turns a role check into
/// no protection at all for branch-scoped data. Resolve the scope here instead
/// and ignore what arrived.
///
/// `users.branch_id` is NOT NULL and is populated from the active branch when a
/// user is created, so this returns the same value the client would have sent
/// on a single-branch install — it closes the hole without changing behaviour.
pub async fn actor_branch_id(pool: &SqlitePool, actor_user_id: &str) -> Result<String, AppError> {
    let row = sqlx::query("SELECT branch_id FROM users WHERE user_id = ? AND is_active = 1")
        .bind(actor_user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Internal(format!("Branch lookup failed: {e}")))?
        .ok_or_else(|| AppError::Permission("User not found or inactive".into()))?;
    Ok(row.get("branch_id"))
}
