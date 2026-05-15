use sqlx::{SqlitePool, Row};
use ulid::Ulid;
use crate::domain::shift::Shift;
use crate::errors::{AppError, AppResult};
use crate::sync::outbox;

fn row_to_shift(row: &sqlx::sqlite::SqliteRow) -> Shift {
    Shift {
        shift_id: row.get("shift_id"),
        branch_id: row.get("branch_id"),
        device_id: row.get("device_id"),
        cashier_user_id: row.get("cashier_user_id"),
        cashier_name: row.get("cashier_name"),
        opened_at: row.get("opened_at"),
        closed_at: row.get("closed_at"),
        opening_cash_minor: row.get("opening_cash_minor"),
        status: row.get("status"),
    }
}

pub async fn get_active_shift(pool: &SqlitePool, device_id: &str) -> AppResult<Option<Shift>> {
    let row = sqlx::query(
        "SELECT s.shift_id, s.branch_id, s.device_id, s.cashier_user_id,
                s.opened_at, s.closed_at, s.opening_cash_minor, s.status,
                u.display_name as cashier_name
         FROM shifts s JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.device_id = ? AND s.status = 'open'
         ORDER BY s.opened_at DESC LIMIT 1"
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.as_ref().map(row_to_shift))
}

pub async fn open_shift(
    pool: &SqlitePool,
    branch_id: &str,
    device_id: &str,
    cashier_user_id: &str,
    opening_cash_minor: i64,
) -> AppResult<Shift> {
    let existing = get_active_shift(pool, device_id).await?;
    if existing.is_some() {
        return Err(AppError::Conflict("A shift is already open for this device".into()));
    }

    let shift_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO shifts (shift_id, branch_id, device_id, cashier_user_id, opened_at, opening_cash_minor, status, sync_status)
         VALUES (?, ?, ?, ?, ?, ?, 'open', 'pending')"
    )
    .bind(&shift_id).bind(branch_id).bind(device_id)
    .bind(cashier_user_id).bind(&now).bind(opening_cash_minor)
    .execute(pool)
    .await?;

    let row = sqlx::query(
        "SELECT s.shift_id, s.branch_id, s.device_id, s.cashier_user_id,
                s.opened_at, s.closed_at, s.opening_cash_minor, s.status,
                u.display_name as cashier_name
         FROM shifts s JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.shift_id = ?"
    )
    .bind(&shift_id)
    .fetch_one(pool)
    .await?;

    let shift = row_to_shift(&row);
    tracing::info!("Shift opened: {}", shift_id);
    let _ = outbox::enqueue_shift(
        pool, device_id, branch_id, &shift_id, cashier_user_id,
        &shift.opened_at, None, opening_cash_minor, None,
        "open", None, &shift.opened_at, "create",
    ).await;
    Ok(shift)
}

pub async fn close_shift(
    pool: &SqlitePool,
    shift_id: &str,
    counted_cash_minor: Option<i64>,
    notes: Option<String>,
) -> AppResult<Shift> {
    let now = chrono::Utc::now().to_rfc3339();

    let expected: Option<i64> = sqlx::query_scalar(
        "SELECT s.opening_cash_minor + COALESCE(
             (SELECT SUM(p.amount_minor) FROM payments p
              JOIN sales sa ON sa.sale_id = p.sale_id
              WHERE sa.shift_id = ? AND p.payment_method = 'cash'), 0)
         FROM shifts s WHERE s.shift_id = ?"
    )
    .bind(shift_id).bind(shift_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    let diff = match (counted_cash_minor, expected) {
        (Some(counted), Some(exp)) => Some(counted - exp),
        _ => None,
    };

    sqlx::query(
        "UPDATE shifts SET status = 'closed', closed_at = ?, counted_cash_minor = ?,
         expected_cash_minor = ?, cash_difference_minor = ?, close_notes = ?
         WHERE shift_id = ? AND status = 'open'"
    )
    .bind(&now).bind(counted_cash_minor).bind(expected).bind(diff)
    .bind(&notes).bind(shift_id)
    .execute(pool)
    .await?;

    let row = sqlx::query(
        "SELECT s.shift_id, s.branch_id, s.device_id, s.cashier_user_id,
                s.opened_at, s.closed_at, s.opening_cash_minor, s.status,
                u.display_name as cashier_name
         FROM shifts s JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.shift_id = ?"
    )
    .bind(shift_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Shift not found".into()))?;

    let shift = row_to_shift(&row);
    tracing::info!("Shift closed: {}", shift_id);
    let _ = outbox::enqueue_shift(
        pool, &shift.device_id, &shift.branch_id, shift_id, &shift.cashier_user_id,
        &shift.opened_at, shift.closed_at.as_deref(),
        shift.opening_cash_minor, counted_cash_minor,
        "closed", notes.as_deref(), &now, "update",
    ).await;
    Ok(shift)
}
