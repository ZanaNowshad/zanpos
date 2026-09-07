use crate::commands::customer_scope::actor_branch_id;
#[cfg(test)]
use crate::commands::customer_scope::customer_in_branch;
use crate::commands::customer_search::{customer_search_patterns, customer_search_where};
use crate::commands::rbac;
use crate::errors::AppError;
use crate::AppState;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
/// Customer management commands — CRUD + loyalty points.
use tauri::State;

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CustomerRow {
    pub customer_id: String,
    pub branch_id: String,
    /// What this shop calls them — the name on the receipt.
    pub name: String,
    /// The name they use on WhatsApp, when it differs. Carried so a cashier who
    /// remembers only that one can still find them.
    pub whatsapp_name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub loyalty_points: i64,
    pub created_at: String,
    pub notes: Option<String>,
}

/// One page of customers plus the branch-wide match count, so the UI can show
/// how many rows exist without loading them.
#[derive(Debug, Serialize)]
pub struct CustomerPage {
    pub items: Vec<CustomerRow>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

// ─── Input types ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CustomerInput {
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub notes: Option<String>,
    /// Set from the caller's session before use. Anything sent here is
    /// overwritten; it stays on the struct because the row records it.
    #[serde(default)]
    pub actor_user_id: String,
}

#[derive(Deserialize)]
pub struct CustomerUpdateInput {
    pub customer_id: String,
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub notes: Option<String>,
    /// Set from the caller's session before use. Anything sent here is
    /// overwritten; it stays on the struct because the row records it.
    #[serde(default)]
    pub actor_user_id: String,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Paging bounds for `customer_list`. A supermarket's customer table is
/// high-cardinality; an unbounded SELECT loaded every row on every keystroke.
const CUSTOMER_LIST_DEFAULT_LIMIT: i64 = 50;
pub(crate) const CUSTOMER_LIST_MAX_LIMIT: i64 = 200;

pub(crate) fn map_row(r: &sqlx::sqlite::SqliteRow) -> CustomerRow {
    CustomerRow {
        customer_id: r.get("customer_id"),
        branch_id: r.get("branch_id"),
        name: r.get("name"),
        whatsapp_name: r.try_get("whatsapp_name").unwrap_or(None),
        phone: r.get("phone"),
        email: r.get("email"),
        loyalty_points: r.get("loyalty_points"),
        created_at: r.get("created_at"),
        notes: r.get("notes"),
    }
}

pub(crate) fn clean_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

// ─── Commands ─────────────────────────────────────────────────────────────────

/// List customers, optionally filtered by name/phone search query.
#[tauri::command]
pub async fn customer_list(
    session_token: String,
    search: String,
    offset: Option<i64>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<CustomerPage, AppError> {
    let actor =
        rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE).await?;
    customer_list_inner(&state.db, &actor.user_id, &search, offset, limit).await
}

async fn customer_list_inner(
    pool: &SqlitePool,
    actor_user_id: &str,
    search: &str,
    offset: Option<i64>,
    limit: Option<i64>,
) -> Result<CustomerPage, AppError> {
    // F-HIGH-04: customer records contain PII (phone, email) — require an active role.
    rbac::require_any_role(pool, actor_user_id).await?;
    let branch_id = actor_branch_id(pool, actor_user_id).await?;

    let limit = limit
        .unwrap_or(CUSTOMER_LIST_DEFAULT_LIMIT)
        .clamp(1, CUSTOMER_LIST_MAX_LIMIT);
    let offset = offset.unwrap_or(0).max(0);

    // `name` is not unique, so ordering by it alone lets rows swap between
    // pages. customer_id is the primary key and breaks every tie, which is
    // what makes paging free of duplicates and gaps.
    let rows = if search.trim().is_empty() {
        sqlx::query(
            "SELECT customer_id, branch_id, name, whatsapp_name, phone, email, loyalty_points, created_at, notes
             FROM customers
             WHERE branch_id = ?
             ORDER BY name, customer_id
             LIMIT ? OFFSET ?",
        )
        .bind(&branch_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else {
        // Search runs in SQL over the whole authorised branch, not over the
        // page already loaded, so a match on page 40 is still reachable.
        let (pattern, digit_pattern) = customer_search_patterns(search);
        let sql = format!(
            "SELECT customer_id, branch_id, name, whatsapp_name, phone, email, loyalty_points, created_at, notes
             FROM customers
             WHERE branch_id = ?
               AND {}
             ORDER BY name, customer_id
             LIMIT ? OFFSET ?",
            customer_search_where(digit_pattern.is_some()),
        );
        let mut q = sqlx::query(&sql)
            .bind(&branch_id)
            // One per text placeholder in customer_search_where: name,
            // whatsapp_name, phone, email. Counted, not guessed — a bind short
            // here shifts every later value left and the branch filter silently
            // becomes a LIKE pattern.
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern);
        if let Some(digits) = &digit_pattern {
            q = q.bind(digits);
        }
        q.bind(limit).bind(offset).fetch_all(pool).await?
    };

    let total: i64 = if search.trim().is_empty() {
        sqlx::query_scalar("SELECT COUNT(*) FROM customers WHERE branch_id = ?")
            .bind(&branch_id)
            .fetch_one(pool)
            .await?
    } else {
        let (pattern, digit_pattern) = customer_search_patterns(search);
        let sql = format!(
            "SELECT COUNT(*) FROM customers WHERE branch_id = ? AND {}",
            customer_search_where(digit_pattern.is_some()),
        );
        let mut q = sqlx::query_scalar(&sql)
            .bind(&branch_id)
            // One per text placeholder in customer_search_where: name,
            // whatsapp_name, phone, email. Counted, not guessed — a bind short
            // here shifts every later value left and the branch filter silently
            // becomes a LIKE pattern.
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern);
        if let Some(digits) = &digit_pattern {
            q = q.bind(digits);
        }
        q.fetch_one(pool).await?
    };

    Ok(CustomerPage {
        items: rows.iter().map(map_row).collect(),
        total,
        offset,
        limit,
    })
}

/// Get a single customer by ID.
#[tauri::command]
pub async fn customer_get(
    session_token: String,
    customer_id: String,
    state: State<'_, AppState>,
) -> Result<CustomerRow, AppError> {
    let actor =
        rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE).await?;
    let branch_id = actor.branch_id.clone();
    // Scoped in the WHERE clause, not checked after fetching: a foreign
    // customer must never be loaded into memory in the first place.
    let row = sqlx::query(
        "SELECT customer_id, branch_id, name, whatsapp_name, phone, email, loyalty_points, created_at, notes
         FROM customers WHERE customer_id = ? AND branch_id = ?",
    )
    .bind(&customer_id)
    .bind(&branch_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Customer {} not found", customer_id)))?;

    Ok(map_row(&row))
}

/// Add loyalty points to a customer. Returns the new total.
#[cfg(test)]
mod name_search_tests;
#[cfg(test)]
mod tests;
