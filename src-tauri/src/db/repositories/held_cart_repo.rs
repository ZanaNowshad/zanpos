use sqlx::{SqlitePool, Row};
use ulid::Ulid;
use crate::domain::cart::Cart;
use crate::domain::refund::HeldCartSummary;
use crate::errors::{AppError, AppResult};

pub async fn save_held_cart(
    pool: &SqlitePool,
    cart: &Cart,
    note: Option<String>,
) -> AppResult<HeldCartSummary> {
    let held_cart_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let cart_json = serde_json::to_string(cart)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let line_count = cart.lines.iter().filter(|l| !l.voided).count() as i64;
    let estimated_total: i64 = cart.lines.iter()
        .filter(|l| !l.voided)
        .map(|l| l.line_total_minor)
        .sum();

    sqlx::query(
        "INSERT INTO held_carts (held_cart_id, branch_id, device_id, shift_id, cashier_user_id, cart_json, held_at, note)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&held_cart_id).bind(&cart.branch_id).bind(&cart.device_id)
    .bind(&cart.shift_id).bind(&cart.cashier_user_id)
    .bind(&cart_json).bind(&now).bind(&note)
    .execute(pool)
    .await?;

    Ok(HeldCartSummary {
        held_cart_id,
        note,
        held_at: now,
        line_count,
        estimated_total_minor: estimated_total,
    })
}

pub async fn list_held_carts(pool: &SqlitePool, device_id: &str) -> AppResult<Vec<HeldCartSummary>> {
    let rows = sqlx::query(
        "SELECT held_cart_id, note, held_at, cart_json FROM held_carts WHERE device_id = ? ORDER BY held_at DESC"
    )
    .bind(device_id)
    .fetch_all(pool)
    .await?;

    let mut result = Vec::new();
    for row in &rows {
        let cart_json: String = row.get("cart_json");
        let cart: Cart = serde_json::from_str(&cart_json)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let line_count = cart.lines.iter().filter(|l| !l.voided).count() as i64;
        let estimated_total: i64 = cart.lines.iter()
            .filter(|l| !l.voided)
            .map(|l| l.line_total_minor)
            .sum();
        result.push(HeldCartSummary {
            held_cart_id: row.get("held_cart_id"),
            note: row.get("note"),
            held_at: row.get("held_at"),
            line_count,
            estimated_total_minor: estimated_total,
        });
    }
    Ok(result)
}

pub async fn resume_held_cart(
    pool: &SqlitePool,
    held_cart_id: &str,
    shift_id: &str,
) -> AppResult<Cart> {
    let row = sqlx::query("SELECT cart_json FROM held_carts WHERE held_cart_id = ?")
        .bind(held_cart_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Held cart not found".into()))?;

    let cart_json: String = row.get("cart_json");
    let mut cart: Cart = serde_json::from_str(&cart_json)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    cart.shift_id = shift_id.to_string();
    cart.cart_id = Ulid::new().to_string();

    sqlx::query("DELETE FROM held_carts WHERE held_cart_id = ?")
        .bind(held_cart_id)
        .execute(pool)
        .await?;

    Ok(cart)
}

pub async fn delete_held_cart(pool: &SqlitePool, held_cart_id: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM held_carts WHERE held_cart_id = ?")
        .bind(held_cart_id)
        .execute(pool)
        .await?;
    Ok(())
}
