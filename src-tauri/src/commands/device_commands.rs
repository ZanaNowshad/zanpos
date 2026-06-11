use crate::commands::rbac;
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::{Deserialize, Serialize};
use sqlx::Row;
/// Device registration management commands.
use tauri::State;
use ulid::Ulid;

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct DeviceRow {
    pub device_id: String,
    pub device_code: String,
    pub device_name: String,
    pub is_active: bool,
    pub created_at: String,
}

// ─── Input types ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DeviceInput {
    pub device_code: String,
    pub device_name: String,
}

// ─── Helper ───────────────────────────────────────────────────────────────────

async fn active_branch_id(state: &AppState) -> AppResult<String> {
    let row = sqlx::query(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No active branch configured".into()))?;
    Ok(row.get("branch_id"))
}

fn map_row(r: &sqlx::sqlite::SqliteRow) -> DeviceRow {
    let active: i64 = r.get("is_active");
    DeviceRow {
        device_id: r.get("device_id"),
        device_code: r.get("device_code"),
        device_name: r.get("name"),
        is_active: active != 0,
        created_at: r.try_get("created_at").unwrap_or_default(),
    }
}

// ─── Commands ─────────────────────────────────────────────────────────────────

/// List all devices for the active branch.
#[tauri::command]
pub async fn device_list(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<DeviceRow>, AppError> {
    // BUG-PRODUCTS-2: this endpoint was unauthenticated — device info is sensitive
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let branch_id = active_branch_id(&state).await?;
    let rows = sqlx::query(
        "SELECT device_id, device_code, name, is_active,
                COALESCE(last_seen_at, '') AS created_at
         FROM devices WHERE branch_id = ?
         ORDER BY device_code",
    )
    .bind(&branch_id)
    .fetch_all(&state.db)
    .await?;

    Ok(rows.iter().map(map_row).collect())
}

/// Register a new POS terminal device.
#[tauri::command]
pub async fn device_create(
    input: DeviceInput,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<DeviceRow, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    if input.device_code.trim().is_empty() {
        return Err(AppError::Validation("Device code is required".into()));
    }
    if input.device_name.trim().is_empty() {
        return Err(AppError::Validation("Device name is required".into()));
    }

    let branch_id = active_branch_id(&state).await?;
    let device_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO devices
           (device_id, branch_id, device_code, name, status, is_active, created_at, updated_at)
         VALUES (?,?,?,?,'offline',1,?,?)",
    )
    .bind(&device_id)
    .bind(&branch_id)
    .bind(input.device_code.trim())
    .bind(input.device_name.trim())
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            AppError::Validation("Device code already exists for this branch".into())
        } else {
            e.into()
        }
    })?;

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    let row = sqlx::query(
        "SELECT device_id, device_code, name, is_active,
                COALESCE(last_seen_at, '') AS created_at
         FROM devices WHERE device_id = ?",
    )
    .bind(&device_id)
    .fetch_one(&state.db)
    .await?;

    let _ = now; // suppress unused warning
    Ok(map_row(&row))
}

/// Activate or deactivate a device.
#[tauri::command]
pub async fn device_toggle_active(
    device_id: String,
    is_active: bool,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let now = chrono::Utc::now().to_rfc3339();
    let affected = sqlx::query("UPDATE devices SET is_active = ?, updated_at = ?, sync_status = 'pending' WHERE device_id = ?")
        .bind(is_active as i64)
        .bind(&now)
        .bind(&device_id)
        .execute(&state.db)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "Device {} not found",
            device_id
        )));
    }

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    Ok(())
}
