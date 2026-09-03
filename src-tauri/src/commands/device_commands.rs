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
    /// When the hub last accepted a heartbeat from this terminal, if ever.
    pub last_seen_at: Option<String>,
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
        last_seen_at: r.try_get("last_seen_at").ok().flatten(),
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
        "SELECT device_id, device_code, name, is_active, created_at, last_seen_at
         FROM devices WHERE branch_id = ? AND deleted_at IS NULL
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
        "SELECT device_id, device_code, name, is_active, created_at, last_seen_at
         FROM devices WHERE device_id = ?",
    )
    .bind(&device_id)
    .fetch_one(&state.db)
    .await?;

    Ok(map_row(&row))
}

/// Remove a device from the register list.
///
/// A soft delete, and deliberately so: `device_id` is stamped on every sale,
/// shift, refund and delivery this terminal ever recorded. Removing the row
/// outright would leave that history pointing at a device that no longer
/// exists, and receipt numbering is per-device — a reused code would collide
/// with numbers already issued.
///
/// Two things are refused rather than warned about. A device with an open shift
/// still has takings to account for, and removing it would strand the count.
/// And a device cannot remove itself: the terminal doing the asking would keep
/// running on a record that says it is gone.
#[tauri::command]
pub async fn device_delete(
    device_id: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    let this_device: Option<String> = sqlx::query_scalar(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .flatten();
    if this_device.as_deref() == Some(device_id.as_str()) {
        return Err(AppError::Validation(
            "This is the terminal you are using — remove it from another device.".into(),
        ));
    }

    let open_shifts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM shifts WHERE device_id = ? AND status = 'open'")
            .bind(&device_id)
            .fetch_one(&state.db)
            .await?;
    if open_shifts > 0 {
        return Err(AppError::Validation(
            "That device still has an open shift. Close the shift first so the cash is accounted for.".into(),
        ));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let affected = sqlx::query(
        "UPDATE devices
            SET deleted_at = ?, is_active = 0, updated_at = ?,
                version = version + 1, sync_status = 'pending'
          WHERE device_id = ? AND deleted_at IS NULL",
    )
    .bind(&now)
    .bind(&now)
    .bind(&device_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("Device {device_id} not found")));
    }
    Ok(())
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

#[derive(Debug, Serialize)]
pub struct DeviceRekeyResult {
    pub old_device_id: String,
    pub device_id: String,
}

/// Re-issue this terminal's device identity with a fresh ULID.
///
/// The recovery path for a database cloned onto a second PC (backup restore):
/// both machines then share one `device_id`, so heartbeats collide, receipt
/// counters mint the same numbers, and each side's rows are invisible to the
/// other. This is irreversible and audited — it rewrites this terminal's
/// origin across its whole history.
#[tauri::command]
pub async fn device_rekey(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<DeviceRekeyResult, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    let (old_device_id, device_id) = crate::device_identity::rekey_to_fresh(&state.db).await?;

    // Every other holder of "which terminal is this" reads it from the database
    // and so picks the new value up immediately. The diagnostics module caches it
    // in process memory instead — for good reason, since it is stamped onto every
    // event — so it is the one place that has to be told.
    crate::diagnostics::set_device_id(device_id.clone());

    let branch_id = active_branch_id(&state).await.unwrap_or_default();
    if let Err(error) = crate::db::repositories::audit_hash::insert_audit_entry(
        &state.db,
        "DEVICE_REKEYED",
        "device",
        "devices",
        &actor_user_id,
        "user",
        &device_id,
        &branch_id,
        None,
        Some(&format!("old_device_id={old_device_id}")),
        Some("Terminal identity re-issued after being shared with another install"),
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [DEVICE_REKEYED]: {error:?}");
    }

    Ok(DeviceRekeyResult {
        old_device_id,
        device_id,
    })
}
