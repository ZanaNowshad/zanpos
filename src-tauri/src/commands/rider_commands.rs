//! Delivery rider roster — CRUD.
//!
//! Riders are not `users`: they never sign in, hold no role and have no PIN.
//! Keeping them in their own table is what keeps them off the cashier-selection
//! screen at login while still giving the shop somewhere to record the WhatsApp
//! number a drop gets sent to. See `migrations/0050_riders.sql`.

use crate::commands::{rbac, sync_commands};
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tauri::State;
use ulid::Ulid;

#[derive(Debug, Serialize)]
pub struct RiderRow {
    pub rider_id: String,
    pub branch_id: String,
    pub name: String,
    pub phone: String,
    pub notes: Option<String>,
    pub is_active: bool,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct RiderInput {
    pub name: String,
    pub phone: String,
    pub notes: Option<String>,
    pub actor_user_id: String,
}

#[derive(Deserialize)]
pub struct RiderUpdateInput {
    pub rider_id: String,
    pub name: String,
    pub phone: String,
    pub notes: Option<String>,
    pub is_active: bool,
    pub actor_user_id: String,
}

fn map_row(r: &sqlx::sqlite::SqliteRow) -> RiderRow {
    RiderRow {
        rider_id: r.get("rider_id"),
        branch_id: r.get("branch_id"),
        name: r.get("name"),
        phone: r.get("phone"),
        notes: r.get("notes"),
        is_active: r.get::<i64, _>("is_active") == 1,
        created_at: r.get("created_at"),
    }
}

async fn actor_branch_id(pool: &SqlitePool, actor_user_id: &str) -> AppResult<String> {
    let row = sqlx::query("SELECT branch_id FROM users WHERE user_id = ? AND is_active = 1")
        .bind(actor_user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::Permission("User not found or inactive".into()))?;
    Ok(row.get("branch_id"))
}

/// Bahrain mobile numbers in E.164. The rider's number is the delivery route —
/// a malformed one fails silently at send time, hours after it was typed, so it
/// is rejected at entry instead.
fn normalize_phone(raw: &str) -> AppResult<String> {
    let mut value: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && !"-()".contains(*c))
        .collect();
    if let Some(rest) = value.strip_prefix("+973") {
        value = rest.to_string();
    } else if let Some(rest) = value.strip_prefix("973") {
        value = rest.to_string();
    }
    let value = value.trim_start_matches('0');
    if value.len() != 8 || !value.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::Validation(
            "Enter an 8-digit Bahrain mobile number for the rider".into(),
        ));
    }
    Ok(format!("+973{value}"))
}

fn clean_notes(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

fn duplicate_phone(e: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(ref db_err) = e {
        let msg = db_err.message();
        if msg.contains("UNIQUE constraint failed") && msg.contains("riders.phone") {
            return AppError::Conflict("Another rider already uses that number".into());
        }
    }
    AppError::Database(e)
}

const SELECT_COLUMNS: &str = "rider_id, branch_id, name, phone, notes, is_active, created_at";

/// List the roster. `active_only` is what the payment modal asks for — a rider
/// who has left should not be offered a new drop, but must stay visible in the
/// admin list so their record can be edited or restored.
#[tauri::command]
pub async fn rider_list(
    actor_user_id: String,
    active_only: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<RiderRow>, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    let branch_id = actor_branch_id(&state.db, &actor_user_id).await?;

    let sql = if active_only.unwrap_or(false) {
        format!(
            "SELECT {SELECT_COLUMNS} FROM riders
             WHERE branch_id = ? AND deleted_at IS NULL AND is_active = 1
             ORDER BY name, rider_id"
        )
    } else {
        format!(
            "SELECT {SELECT_COLUMNS} FROM riders
             WHERE branch_id = ? AND deleted_at IS NULL
             ORDER BY is_active DESC, name, rider_id"
        )
    };

    let rows = sqlx::query(&sql)
        .bind(&branch_id)
        .fetch_all(&state.db)
        .await?;
    Ok(rows.iter().map(map_row).collect())
}

#[tauri::command]
pub async fn rider_create(
    input: RiderInput,
    state: State<'_, AppState>,
) -> Result<RiderRow, AppError> {
    rbac::require_role(&state.db, &input.actor_user_id, &["owner", "manager"]).await?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("Rider name is required".into()));
    }
    if name.len() > 255 {
        return Err(AppError::Validation(
            "Rider name must not exceed 255 characters".into(),
        ));
    }
    let phone = normalize_phone(&input.phone)?;
    let notes = clean_notes(input.notes.as_deref());

    let branch_id = actor_branch_id(&state.db, &input.actor_user_id).await?;
    let rider_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let device_id: String = sqlx::query_scalar(
        "SELECT device_id FROM devices WHERE is_active=1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .flatten()
    .unwrap_or_default();

    sqlx::query(
        "INSERT INTO riders
           (rider_id, branch_id, origin_device_id, name, phone, notes, is_active,
            created_at, updated_at, sync_status, sync_attempts)
         VALUES (?,?,?,?,?,?,1,?,?,'pending',0)",
    )
    .bind(&rider_id)
    .bind(&branch_id)
    .bind(&device_id)
    .bind(name)
    .bind(&phone)
    .bind(notes.as_deref())
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(duplicate_phone)?;

    let row = sqlx::query(&format!(
        "SELECT {SELECT_COLUMNS} FROM riders WHERE rider_id = ?"
    ))
    .bind(&rider_id)
    .fetch_one(&state.db)
    .await?;
    sync_commands::schedule_immediate_sync(&state);
    Ok(map_row(&row))
}

#[tauri::command]
pub async fn rider_update(
    input: RiderUpdateInput,
    state: State<'_, AppState>,
) -> Result<RiderRow, AppError> {
    rbac::require_role(&state.db, &input.actor_user_id, &["owner", "manager"]).await?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("Rider name is required".into()));
    }
    let phone = normalize_phone(&input.phone)?;
    let notes = clean_notes(input.notes.as_deref());
    let branch_id = actor_branch_id(&state.db, &input.actor_user_id).await?;
    let now = chrono::Utc::now().to_rfc3339();

    let changed = sqlx::query(
        "UPDATE riders
            SET name = ?, phone = ?, notes = ?, is_active = ?, updated_at = ?,
                version = version + 1, sync_status = 'pending'
          WHERE rider_id = ? AND branch_id = ? AND deleted_at IS NULL",
    )
    .bind(name)
    .bind(&phone)
    .bind(notes.as_deref())
    .bind(i64::from(input.is_active))
    .bind(&now)
    .bind(&input.rider_id)
    .bind(&branch_id)
    .execute(&state.db)
    .await
    .map_err(duplicate_phone)?
    .rows_affected();

    if changed == 0 {
        return Err(AppError::NotFound(format!(
            "Rider {} not found",
            input.rider_id
        )));
    }

    let row = sqlx::query(&format!(
        "SELECT {SELECT_COLUMNS} FROM riders WHERE rider_id = ?"
    ))
    .bind(&input.rider_id)
    .fetch_one(&state.db)
    .await?;
    sync_commands::schedule_immediate_sync(&state);
    Ok(map_row(&row))
}

/// Soft delete. Past deliveries keep the rider's name in
/// `delivery_orders.delivery_staff_name`, which is history and must not change
/// because someone left the roster.
#[tauri::command]
pub async fn rider_delete(
    rider_id: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::require_role(&state.db, &actor_user_id, &["owner", "manager"]).await?;
    let branch_id = actor_branch_id(&state.db, &actor_user_id).await?;
    let now = chrono::Utc::now().to_rfc3339();

    let changed = sqlx::query(
        "UPDATE riders
            SET deleted_at = ?, is_active = 0, updated_at = ?,
                version = version + 1, sync_status = 'pending'
          WHERE rider_id = ? AND branch_id = ? AND deleted_at IS NULL",
    )
    .bind(&now)
    .bind(&now)
    .bind(&rider_id)
    .bind(&branch_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if changed == 0 {
        return Err(AppError::NotFound(format!("Rider {rider_id} not found")));
    }
    sync_commands::schedule_immediate_sync(&state);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_shapes_a_cashier_actually_types() {
        for raw in ["33050666", "+973 3305 0666", "973-33050666", "033050666"] {
            assert_eq!(normalize_phone(raw).unwrap(), "+97333050666", "for {raw:?}");
        }
    }

    #[test]
    fn rejects_anything_that_would_fail_silently_at_send_time() {
        for raw in ["", "3305066", "330506667", "notaphone", "+44 7700 900000"] {
            assert!(
                normalize_phone(raw).is_err(),
                "expected {raw:?} to be rejected"
            );
        }
    }
}
