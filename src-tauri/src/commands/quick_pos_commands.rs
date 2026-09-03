//! Quick POS slots — the fixed row of one-tap products on the till.
//!
//! Ten ordered slots, each either empty or holding a product. Stored as a JSON
//! array in `app_config.quick_pos_products` rather than a table: it is a single
//! short list owned by the shop, not per-row data, and living in app_config
//! means it rides the existing config sync so every till shows the same row.
//!
//! Slots hold a `product_id` only. Name, price and image are resolved on read,
//! so a price change or a new photo reaches the till without anyone re-picking
//! the item — and a product that is deleted or deactivated simply reads back as
//! an empty slot instead of a broken tile.

use crate::commands::rbac;
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tauri::State;

/// The till renders exactly this many, so the picker offers exactly this many.
pub const SLOT_COUNT: usize = 10;

const CONFIG_KEY: &str = "quick_pos_products";

#[derive(Debug, Serialize)]
pub struct QuickPosSlot {
    /// 0-based position in the row. Always present, even when empty, so the
    /// caller can render the gaps without inferring them from the index.
    pub slot: usize,
    pub product_id: Option<String>,
    pub name: Option<String>,
    pub price_minor: Option<i64>,
    pub image_path: Option<String>,
}

impl QuickPosSlot {
    fn empty(slot: usize) -> Self {
        Self {
            slot,
            product_id: None,
            name: None,
            price_minor: None,
            image_path: None,
        }
    }
}

/// Read the raw slot assignment. Missing or malformed config reads as all-empty
/// rather than failing — a bad row of shortcuts must never stop the till.
async fn read_slots(pool: &SqlitePool) -> Vec<Option<String>> {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM app_config WHERE key = ?")
        .bind(CONFIG_KEY)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

    let mut slots: Vec<Option<String>> = raw
        .and_then(|json| serde_json::from_str::<Vec<Option<String>>>(&json).ok())
        .unwrap_or_default();
    slots.truncate(SLOT_COUNT);
    slots.resize(SLOT_COUNT, None);
    slots
}

#[tauri::command]
pub async fn quick_pos_load(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<QuickPosSlot>, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    load_inner(&state.db).await
}

pub(crate) async fn load_inner(pool: &SqlitePool) -> AppResult<Vec<QuickPosSlot>> {
    let slots = read_slots(pool).await;
    let mut out = Vec::with_capacity(SLOT_COUNT);

    for (index, product_id) in slots.into_iter().enumerate() {
        let Some(product_id) = product_id.filter(|id| !id.is_empty()) else {
            out.push(QuickPosSlot::empty(index));
            continue;
        };

        // The one price predicate, shared with the catalogue and with checkout.
        // This was a copy of it that had lost the tie-break, so a product with
        // two open price rows showed one price on the tile and was charged
        // another at the till.
        let price_in_force = crate::db::repositories::pricing::PRICE_IN_FORCE;
        let row = sqlx::query(&format!(
            "SELECT p.product_id, p.name, p.image_path,
                    COALESCE(pp.price_minor, 0) AS price_minor
               FROM products p
               LEFT JOIN product_prices pp ON pp.product_id = p.product_id
                    AND {price_in_force}
              WHERE p.product_id = ? AND p.is_active = 1 AND p.deleted_at IS NULL"
        ))
        .bind(&product_id)
        .fetch_optional(pool)
        .await?;

        match row {
            // A retired product leaves a gap, not a tile that cannot be sold.
            None => out.push(QuickPosSlot::empty(index)),
            Some(row) => out.push(QuickPosSlot {
                slot: index,
                product_id: Some(row.get("product_id")),
                name: Some(row.get("name")),
                price_minor: Some(row.get("price_minor")),
                image_path: row.try_get("image_path").ok(),
            }),
        }
    }

    Ok(out)
}

#[derive(Debug, Deserialize)]
pub struct QuickPosSaveInput {
    pub actor_user_id: String,
    /// Exactly `SLOT_COUNT` entries; `null` for an empty slot. Order is the
    /// order the till renders.
    pub product_ids: Vec<Option<String>>,
}

#[tauri::command]
pub async fn quick_pos_save(
    input: QuickPosSaveInput,
    state: State<'_, AppState>,
) -> Result<Vec<QuickPosSlot>, AppError> {
    rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;

    if input.product_ids.len() > SLOT_COUNT {
        return Err(AppError::Validation(format!(
            "The till shows {SLOT_COUNT} quick items; {} were sent.",
            input.product_ids.len()
        )));
    }

    let mut slots = input.product_ids;
    slots.resize(SLOT_COUNT, None);

    // Reject a product the till could not sell, rather than storing an id that
    // silently reads back as a gap and looks like the save was lost.
    for product_id in slots.iter().flatten() {
        if product_id.is_empty() {
            continue;
        }
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM products
              WHERE product_id = ? AND is_active = 1 AND deleted_at IS NULL",
        )
        .bind(product_id)
        .fetch_optional(&state.db)
        .await?;
        if exists.is_none() {
            return Err(AppError::Validation(
                "That product is no longer active and cannot be a quick item.".into(),
            ));
        }
    }

    let json = serde_json::to_string(&slots)
        .map_err(|e| AppError::Internal(format!("Encode quick POS slots: {e}")))?;

    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(CONFIG_KEY)
    .bind(&json)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&state.db)
    .await?;

    load_inner(&state.db).await
}

#[cfg(test)]
mod tests;
