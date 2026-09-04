#![cfg(test)]
//! Who the caller is, and who they may claim to be.
//!
//! ZANPOS has two ways to decide whether an operation is allowed, and they
//! answer different questions.
//!
//! `commands::rbac::require_role` is handed a user id and asks *does this id
//! hold an allowed role*. It never asks whether the caller is that user,
//! because it has no way to. The id arrives in the command payload, so the
//! answer is whatever the caller decided to type. `auth_list_users` — which
//! needs no authentication at all — returns every active user's id together
//! with their role, so choosing an owner's id is a lookup, not an attack.
//!
//! `commands::rbac::session_actor` asks *who is the caller*. The token indexes
//! a session this process issued at login; the branch and role are read back
//! from the database at resolve time. No part of a request payload reaches any
//! of it.
//!
//! These tests pin that distinction so it cannot quietly regress, and so the
//! migration away from payload-derived actors has a definition of done.

use super::migrated_pool;
use crate::auth_session::SessionStore;
use crate::commands::rbac;
use sqlx::SqlitePool;

const OWNER: &str = "01JUSER000000000000ADMIN1";
const CASHIER: &str = "01JUSER000000000000CASH01";

/// Seed users the migrations ship, re-activated and given a real branch.
///
/// Migration 0030 deactivates the seed accounts on purpose so nobody can log in
/// with a well-known PIN; a test that needs an active user says so itself
/// rather than depending on that default.
async fn with_active_users(pool: &SqlitePool) {
    sqlx::query("UPDATE users SET is_active = 1 WHERE user_id IN (?, ?)")
        .bind(OWNER)
        .bind(CASHIER)
        .execute(pool)
        .await
        .expect("activate seed users");
}

async fn role_of(pool: &SqlitePool, user_id: &str) -> String {
    sqlx::query_scalar(
        "SELECT r.name FROM users u JOIN roles r ON r.role_id = u.role_id WHERE u.user_id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("role")
}

/// The two mechanisms disagree about a spoofed id, and that disagreement is
/// the defect.
///
/// A cashier who names the owner passes every payload-derived check, because
/// the check reads the row for the id it was handed. The same cashier holding
/// a real cashier session cannot pass the session-derived check no matter what
/// they put in the payload, because the payload is not consulted.
#[tokio::test]
async fn a_cashier_cannot_become_an_owner_by_naming_one() {
    let pool = migrated_pool().await;
    with_active_users(&pool).await;
    assert_eq!(role_of(&pool, OWNER).await, "owner");
    assert_eq!(role_of(&pool, CASHIER).await, "cashier");

    // The payload-derived path: the caller supplies the owner's id and the
    // owner-only gate opens. This is RBAC-1, stated as an executable fact.
    assert!(
        rbac::owner_only(&pool, OWNER).await.is_ok(),
        "naming the owner passes the payload-derived owner gate — this is the \
         defect being migrated away from, not a property to preserve"
    );

    // The session-derived path: a cashier's own token resolves to a cashier,
    // and there is nowhere to put a different id.
    let sessions = SessionStore::default();
    let cashier_token = sessions.issue(CASHIER).await.token;

    let escalated = rbac::session_actor(&sessions, &pool, &cashier_token, rbac::OWNER_ONLY).await;
    assert!(
        escalated.is_err(),
        "a cashier's session resolved to an owner-permitted actor — the token \
         no longer binds identity"
    );

    let actor = rbac::session_actor(&sessions, &pool, &cashier_token, rbac::ANY_ROLE)
        .await
        .expect("a cashier is still allowed to act as a cashier");
    assert_eq!(
        actor.user_id, CASHIER,
        "the resolved actor must be the session's user, never a supplied one"
    );
    assert_eq!(actor.role_name, "cashier");
}

/// An owner's session is an owner's session — the positive case, so the test
/// above cannot pass by rejecting everything.
#[tokio::test]
async fn an_owner_session_still_opens_owner_only_work() {
    let pool = migrated_pool().await;
    with_active_users(&pool).await;
    let sessions = SessionStore::default();
    let token = sessions.issue(OWNER).await.token;

    let actor = rbac::session_actor(&sessions, &pool, &token, rbac::OWNER_ONLY)
        .await
        .expect("an owner must pass the owner gate");
    assert_eq!(actor.user_id, OWNER);
    assert_eq!(actor.role_name, "owner");
    assert!(
        !actor.branch_id.is_empty(),
        "the branch must come back from the database, not from the caller"
    );
}

/// A branch in a payload names what is being acted on, not where the caller is.
#[tokio::test]
async fn a_cashier_cannot_reach_into_another_branch() {
    let pool = migrated_pool().await;
    with_active_users(&pool).await;
    let sessions = SessionStore::default();

    let cashier = rbac::session_actor(
        &sessions,
        &pool,
        &sessions.issue(CASHIER).await.token,
        rbac::ANY_ROLE,
    )
    .await
    .expect("cashier session");

    rbac::require_branch(&cashier, &cashier.branch_id)
        .expect("a cashier may act on their own branch");
    assert!(
        rbac::require_branch(&cashier, "01JBRANCH0000000000OTHER").is_err(),
        "a cashier reached another branch by naming it"
    );

    // An owner is the deliberate exception — multi-branch administration is the
    // reason the role exists.
    let owner = rbac::session_actor(
        &sessions,
        &pool,
        &sessions.issue(OWNER).await.token,
        rbac::ANY_ROLE,
    )
    .await
    .expect("owner session");
    rbac::require_branch(&owner, "01JBRANCH0000000000OTHER")
        .expect("an owner administers every branch");
}

/// No token, a malformed token, and a well-formed token nobody issued are all
/// refused — an unauthenticated caller cannot reach a privileged actor.
#[tokio::test]
async fn an_unauthenticated_caller_resolves_to_nothing() {
    let pool = migrated_pool().await;
    with_active_users(&pool).await;
    let sessions = SessionStore::default();

    for (label, token) in [
        ("empty", String::new()),
        ("not base64", "!!!!".to_string()),
        ("wrong length", "c2hvcnQ".to_string()),
        // Correctly shaped and correctly sized, simply never issued.
        ("never issued", "A".repeat(43)),
    ] {
        assert!(
            rbac::session_actor(&sessions, &pool, &token, rbac::ANY_ROLE)
                .await
                .is_err(),
            "a {label} token authenticated somebody"
        );
    }
}

/// Revoking a session ends its authority immediately.
#[tokio::test]
async fn a_revoked_session_stops_authorising() {
    let pool = migrated_pool().await;
    with_active_users(&pool).await;
    let sessions = SessionStore::default();
    let token = sessions.issue(OWNER).await.token;

    rbac::session_actor(&sessions, &pool, &token, rbac::OWNER_ONLY)
        .await
        .expect("the session works before revocation");
    sessions.revoke(&token).await.expect("revoke");
    assert!(
        rbac::session_actor(&sessions, &pool, &token, rbac::OWNER_ONLY)
            .await
            .is_err(),
        "a revoked token still authorised — logout does not end authority"
    );
}

/// Deactivating an account ends its authority even while a token is live.
///
/// The session is in memory and the account is in the database, so this only
/// holds because the role and branch are re-read on every resolve rather than
/// captured at login.
#[tokio::test]
async fn deactivating_an_account_ends_a_live_session() {
    let pool = migrated_pool().await;
    with_active_users(&pool).await;
    let sessions = SessionStore::default();
    let token = sessions.issue(CASHIER).await.token;

    rbac::session_actor(&sessions, &pool, &token, rbac::ANY_ROLE)
        .await
        .expect("active cashier resolves");

    sqlx::query("UPDATE users SET is_active = 0 WHERE user_id = ?")
        .bind(CASHIER)
        .execute(&pool)
        .await
        .expect("deactivate");

    assert!(
        rbac::session_actor(&sessions, &pool, &token, rbac::ANY_ROLE)
            .await
            .is_err(),
        "a deactivated employee kept working until their token expired"
    );
}

/// A role change takes effect on the next call, not at the next login.
#[tokio::test]
async fn a_demotion_takes_effect_without_a_new_login() {
    let pool = migrated_pool().await;
    with_active_users(&pool).await;
    let sessions = SessionStore::default();
    let token = sessions.issue(OWNER).await.token;

    rbac::session_actor(&sessions, &pool, &token, rbac::OWNER_ONLY)
        .await
        .expect("owner before demotion");

    let cashier_role: String =
        sqlx::query_scalar("SELECT role_id FROM users WHERE user_id = ?")
            .bind(CASHIER)
            .fetch_one(&pool)
            .await
            .expect("cashier role id");
    sqlx::query("UPDATE users SET role_id = ? WHERE user_id = ?")
        .bind(&cashier_role)
        .bind(OWNER)
        .execute(&pool)
        .await
        .expect("demote");

    assert!(
        rbac::session_actor(&sessions, &pool, &token, rbac::OWNER_ONLY)
            .await
            .is_err(),
        "a demoted owner kept owner authority on an existing session"
    );
}

/// Naming a target is legitimate; naming a caller is not.
///
/// Administrative work has to be able to say which user it acts on. The
/// invariant is only that the target never becomes the principal — so an
/// owner acting on a cashier is authorised as the owner, and a cashier naming
/// the owner as a target is still only a cashier.
#[tokio::test]
async fn a_target_id_in_the_payload_is_not_the_caller() {
    let pool = migrated_pool().await;
    with_active_users(&pool).await;
    let sessions = SessionStore::default();

    let owner = rbac::session_actor(
        &sessions,
        &pool,
        &sessions.issue(OWNER).await.token,
        rbac::MANAGER_OR_OWNER,
    )
    .await
    .expect("owner may administer users");
    assert_eq!(
        owner.user_id, OWNER,
        "the actor is the session's user even when the payload names another"
    );

    // The same call shape from a cashier, naming the owner as the target, is
    // refused on the caller's role — the target is irrelevant to that decision.
    assert!(
        rbac::session_actor(
            &sessions,
            &pool,
            &sessions.issue(CASHIER).await.token,
            rbac::MANAGER_OR_OWNER,
        )
        .await
        .is_err(),
        "a cashier administering users was allowed"
    );
}

/// Pre-authentication login discovery gives out no account id.
///
/// This listing has to exist — a shared till shows who is on shift so they can
/// pick themselves — and it runs before anyone has authenticated. It used to
/// return each account's id alongside their role, which is what turned "guess
/// an owner's id" into "read one" while a payload id could still establish the
/// caller.
///
/// Two things changed. Knowing an id stopped being worth anything on migrated
/// commands, and the id stopped being handed out here at all: login is by
/// username, so nothing needed it. The names and roles that remain are already
/// on the screen in front of whoever is standing at the till.
#[tokio::test]
async fn login_discovery_exposes_no_account_ids() {
    let pool = migrated_pool().await;
    with_active_users(&pool).await;

    let users = crate::db::repositories::auth_repo::list_active_users(&pool)
        .await
        .expect("the PIN screen lists users before login");
    assert!(!users.is_empty(), "the PIN screen has nobody to show");

    // The type itself is the guarantee: if a `user_id` field is ever added back
    // to `UserSummary`, this stops compiling rather than silently leaking.
    let serialised = serde_json::to_string(&users).expect("serialise the listing");
    assert!(
        !serialised.contains(OWNER) && !serialised.contains(CASHIER),
        "the pre-auth listing carries an account id: {serialised}"
    );
    assert!(
        !serialised.contains("user_id"),
        "the pre-auth listing has a user_id field again: {serialised}"
    );

    // Login still works from what remains.
    let owner = users
        .iter()
        .find(|u| u.role_name == "owner")
        .expect("the owner is still listed by name");
    assert!(
        !owner.username.is_empty(),
        "login is by username, so the username must survive"
    );
}
