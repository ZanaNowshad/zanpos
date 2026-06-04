use crate::errors::{AppError, AppResult};
use sqlx::Row;
use sqlx::SqlitePool;

/// Resolve the active branch_id. Returns NotFound error if no active branch.
pub async fn active_branch_id(pool: &SqlitePool) -> AppResult<String> {
    let row = sqlx::query(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("No active branch configured".into()))?;
    Ok(row.get("branch_id"))
}

/// Resolve the active device_id. Returns NotFound error if no active device.
pub async fn active_device_id(pool: &SqlitePool) -> AppResult<String> {
    let row = sqlx::query(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("No active device configured".into()))?;
    Ok(row.get("device_id"))
}

/// Resolve both active branch_id and device_id in one call.
pub async fn active_branch_and_device(pool: &SqlitePool) -> AppResult<(String, String)> {
    Ok((active_branch_id(pool).await?, active_device_id(pool).await?))
}
