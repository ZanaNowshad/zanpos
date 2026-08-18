//! Loyalty points — the branch-wide aggregate, the top balances, and awarding.
//!
//! Split from `customer_commands` for size. The seam is real rather than
//! arbitrary: these three answer "what does the store owe, and to whom?", while
//! the directory next door answers "who is this customer?". They share the
//! branch-scoping helpers, which stay with the directory as the owner of the
//! customer row.

use crate::commands::customer_commands::{map_row, CustomerRow, CUSTOMER_LIST_MAX_LIMIT};
use crate::commands::customer_scope::{actor_branch_id, customer_in_branch};
use crate::commands::{rbac, sync_commands};
use crate::db::repositories::audit_hash;
use crate::errors::AppError;
use crate::AppState;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use tauri::State;

/// Branch-wide loyalty aggregate. Every field is a SQL COUNT or SUM.
#[derive(Debug, Serialize)]
pub struct LoyaltySummary {
    pub outstanding_points: i64,
    pub holders: i64,
    pub total_customers: i64,
    pub contactable_holders: i64,
}

#[tauri::command]
pub async fn customer_loyalty_summary(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<LoyaltySummary, AppError> {
    customer_loyalty_summary_inner(&state.db, &actor_user_id).await
}

pub(crate) async fn customer_loyalty_summary_inner(
    pool: &SqlitePool,
    actor_user_id: &str,
) -> Result<LoyaltySummary, AppError> {
    rbac::require_any_role(pool, actor_user_id).await?;
    let branch_id = actor_branch_id(pool, actor_user_id).await?;

    let row = sqlx::query(
        "SELECT
           COALESCE(SUM(loyalty_points), 0)                                    AS outstanding_points,
           COALESCE(SUM(CASE WHEN loyalty_points > 0 THEN 1 ELSE 0 END), 0)     AS holders,
           COUNT(*)                                                            AS total_customers,
           COALESCE(SUM(CASE WHEN loyalty_points > 0
                              AND phone IS NOT NULL
                              AND TRIM(phone) <> '' THEN 1 ELSE 0 END), 0)      AS contactable_holders
         FROM customers WHERE branch_id = ?",
    )
    .bind(&branch_id)
    .fetch_one(pool)
    .await?;

    Ok(LoyaltySummary {
        outstanding_points: row.get("outstanding_points"),
        holders: row.get("holders"),
        total_customers: row.get("total_customers"),
        contactable_holders: row.get("contactable_holders"),
    })
}

/// Highest balances in the actor's branch, ranked in SQL.
#[tauri::command]
pub async fn customer_top_balances(
    actor_user_id: String,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<CustomerRow>, AppError> {
    customer_top_balances_inner(&state.db, &actor_user_id, limit).await
}

pub(crate) async fn customer_top_balances_inner(
    pool: &SqlitePool,
    actor_user_id: &str,
    limit: Option<i64>,
) -> Result<Vec<CustomerRow>, AppError> {
    rbac::require_any_role(pool, actor_user_id).await?;
    let branch_id = actor_branch_id(pool, actor_user_id).await?;
    let limit = limit.unwrap_or(25).clamp(1, CUSTOMER_LIST_MAX_LIMIT);

    let rows = sqlx::query(
        "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
         FROM customers
         WHERE branch_id = ? AND loyalty_points > 0
         ORDER BY loyalty_points DESC, name, customer_id
         LIMIT ?",
    )
    .bind(&branch_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(map_row).collect())
}

/// Create a new customer, returning the created row.

#[tauri::command]
pub async fn customer_add_loyalty(
    actor_user_id: String,
    customer_id: String,
    points: i64,
    state: State<'_, AppState>,
) -> Result<i64, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let actor_branch = actor_branch_id(&state.db, &actor_user_id).await?;
    customer_in_branch(&state.db, &customer_id, &actor_branch).await?;
    if points == 0 {
        return Err(AppError::Validation("Points delta must be non-zero".into()));
    }

    // Fetch current loyalty points for before-state and existence check
    let existing = sqlx::query(
        "SELECT loyalty_points, branch_id, origin_device_id
         FROM customers WHERE customer_id = ?",
    )
    .bind(&customer_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Customer {} not found", customer_id)))?;

    let before_pts: i64 = existing.get("loyalty_points");
    let existing_branch: String = existing.get("branch_id");
    let existing_device: String = existing.get("origin_device_id");

    // Guard: loyalty_points must not go negative
    if points < 0 && before_pts + points < 0 {
        return Err(AppError::Validation(
            "Insufficient loyalty points for this deduction".into(),
        ));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let affected = sqlx::query(
        "UPDATE customers SET loyalty_points = loyalty_points + ?, updated_at = ?, sync_status = 'pending' WHERE customer_id = ?")
        .bind(points)
        .bind(&now)
        .bind(&customer_id)
        .execute(&state.db)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "Customer {} not found",
            customer_id
        )));
    }

    let row = sqlx::query(
        "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
         FROM customers WHERE customer_id = ?",
    )
    .bind(&customer_id)
    .fetch_one(&state.db)
    .await?;

    let customer = map_row(&row);
    let new_total = customer.loyalty_points;

    // sync_status='pending' is set explicitly in UPDATE — sync worker picks it up

    // Audit trail
    let before = serde_json::json!({ "loyalty_points": before_pts }).to_string();
    let after = serde_json::json!({ "loyalty_points": new_total }).to_string();
    if let Err(e) = audit_hash::insert_audit_entry(
        &state.db,
        "CUSTOMER_LOYALTY_ADJUSTED",
        "customer",
        &customer_id,
        &actor_user_id,
        "user",
        &existing_device,
        &existing_branch,
        Some(&before),
        Some(&after),
        None,
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [CUSTOMER_LOYALTY_ADJUSTED]: {:?}", e);
    }

    sync_commands::schedule_immediate_sync(&state);
    Ok(new_total)
}
