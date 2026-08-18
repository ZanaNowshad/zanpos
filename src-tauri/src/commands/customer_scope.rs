//! Branch scoping for customer records.
//!
//! Shared by the directory and the loyalty commands, which is why it lives on
//! its own rather than inside either. Both need the same two guarantees and
//! neither should be free to reinvent them.

use crate::errors::{AppError, AppResult};
use sqlx::{Row, SqlitePool};

/// The branch the *actor* belongs to, resolved from the database.
///
/// Customer rows carry PII, so their scope must come from server-held identity
/// rather than anything the caller sends. `users.branch_id` is NOT NULL and is
/// populated from `active_branch_id()` at user creation — the same source
/// `customer_create` uses — so on any existing install every user and every
/// customer already share a branch id and this scoping hides nothing.
///
/// `users.branch_scope` is written as '[]' and never read; multi-branch
/// assignment is not implemented, so it is deliberately not consulted here.
pub(crate) async fn actor_branch_id(pool: &SqlitePool, actor_user_id: &str) -> AppResult<String> {
    let row = sqlx::query("SELECT branch_id FROM users WHERE user_id = ? AND is_active = 1")
        .bind(actor_user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::Permission("User not found or inactive".into()))?;
    Ok(row.get("branch_id"))
}

/// Confirm a customer exists *and* belongs to the actor's branch.
///
/// Returns `NotFound` rather than `Permission` for a foreign customer: telling
/// a caller "that exists but is not yours" confirms the existence of another
/// branch's customer record, which is itself a disclosure.
pub(crate) async fn customer_in_branch(
    pool: &SqlitePool,
    customer_id: &str,
    branch_id: &str,
) -> AppResult<()> {
    let found: Option<String> = sqlx::query_scalar(
        "SELECT customer_id FROM customers WHERE customer_id = ? AND branch_id = ?",
    )
    .bind(customer_id)
    .bind(branch_id)
    .fetch_optional(pool)
    .await?;
    found
        .map(|_| ())
        .ok_or_else(|| AppError::NotFound(format!("Customer {} not found", customer_id)))
}
