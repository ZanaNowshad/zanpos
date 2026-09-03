use crate::domain::cart::Cart;
use crate::domain::refund::HeldCartSummary;
use crate::errors::{AppError, AppResult};
use sqlx::{Row, SqlitePool};
use ulid::Ulid;

pub async fn save_held_cart(
    pool: &SqlitePool,
    cart: &Cart,
    note: Option<String>,
) -> AppResult<HeldCartSummary> {
    let held_cart_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let cart_json = serde_json::to_string(cart).map_err(|e| AppError::Internal(e.to_string()))?;

    let line_count = cart.lines.iter().filter(|l| !l.voided).count() as i64;
    let estimated_total: i64 = cart
        .lines
        .iter()
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

pub async fn list_held_carts(
    pool: &SqlitePool,
    device_id: &str,
) -> AppResult<Vec<HeldCartSummary>> {
    let rows = sqlx::query(
        "SELECT held_cart_id, note, held_at, cart_json FROM held_carts WHERE device_id = ? ORDER BY held_at DESC"
    )
    .bind(device_id)
    .fetch_all(pool)
    .await?;

    let mut result = Vec::new();
    for row in &rows {
        let cart_json: String = row.get("cart_json");
        let cart: Cart =
            serde_json::from_str(&cart_json).map_err(|e| AppError::Internal(e.to_string()))?;
        let line_count = cart.lines.iter().filter(|l| !l.voided).count() as i64;
        let estimated_total: i64 = cart
            .lines
            .iter()
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
    let mut cart: Cart =
        serde_json::from_str(&cart_json).map_err(|e| AppError::Internal(e.to_string()))?;

    let held_cart_key = std::mem::replace(&mut cart.cart_id, Ulid::new().to_string());
    cart.shift_id = shift_id.to_string();

    // P2-06: validate that all products in the held cart still exist
    for line in &cart.lines {
        if let Some(pid) = &line.product_id {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM products WHERE product_id=? AND deleted_at IS NULL)",
            )
            .bind(pid)
            .fetch_one(pool)
            .await
            .unwrap_or(false);
            if !exists {
                return Err(AppError::Validation(format!(
                    "A product in this held order no longer exists (id: {}). \
                     Please remove it before resuming.",
                    pid
                )));
            }
        }
    }

    // Carry the manager's approvals onto the new cart id.
    //
    // A resumed cart is a new cart as far as the database is concerned, and both
    // `pos_price_overrides` and `pos_discount_authorizations` are keyed by cart
    // id — that is what makes them proof rather than a claim from the frontend.
    // Left behind, the approvals go missing exactly when the cashier presses
    // Charge: an overridden price silently reverts to the catalogue and fails the
    // payment check, and a discount is refused outright. The manager approved
    // this basket; parking it at the till and picking it up again does not
    // withdraw that. Re-keying, deleting the held row and doing both in one
    // transaction means a crash mid-resume cannot strand the approvals against a
    // cart nobody holds.
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE pos_price_overrides SET cart_id = ? WHERE cart_id = ?")
        .bind(&cart.cart_id)
        .bind(&held_cart_key)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE pos_discount_authorizations SET cart_id = ? WHERE cart_id = ?")
        .bind(&cart.cart_id)
        .bind(&held_cart_key)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM held_carts WHERE held_cart_id = ?")
        .bind(held_cart_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(cart)
}

pub async fn delete_held_cart(pool: &SqlitePool, held_cart_id: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM held_carts WHERE held_cart_id = ?")
        .bind(held_cart_id)
        .execute(pool)
        .await?;
    Ok(())
}
