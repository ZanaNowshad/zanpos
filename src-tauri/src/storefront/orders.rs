use crate::errors::{AppError, AppResult};
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use std::{collections::HashSet, str::FromStr};

const START: &str = "[ZANPOS:v1]";
const END: &str = "[/ZANPOS]";
const MAX_BLOCK_BYTES: usize = 32 * 1024;
const MAX_ITEMS: usize = 100;
const MAX_QUANTITY: Decimal = Decimal::from_parts(100, 0, 0, false, 0);

struct IncomingOrder {
    order_id: String,
    currency: String,
    items: Vec<IncomingItem>,
}

struct IncomingItem {
    product_id: String,
    quantity: Decimal,
}

#[derive(Serialize)]
struct NormalizedOrder {
    schema_version: u32,
    external_ref: String,
    branch_id: String,
    items: Vec<NormalizedItem>,
    total_minor: i64,
    currency: String,
}

#[derive(Serialize)]
struct NormalizedItem {
    product_id: String,
    name: String,
    quantity: String,
    unit_price_minor: i64,
    line_total_minor: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestedStorefrontOrder {
    pub order_id: String,
    pub external_ref: String,
    pub total_minor: i64,
    pub currency: String,
    pub raw_json: String,
}

fn valid_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                *byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn parse_block(body: &str) -> AppResult<Option<IncomingOrder>> {
    if !body.contains(START) {
        return Ok(None);
    }
    let normalized = body.replace("\r\n", "\n");
    let start = normalized
        .find(START)
        .ok_or_else(|| AppError::Validation("Missing storefront order marker".into()))?;
    let after_start = start + START.len();
    let relative_end = normalized[after_start..]
        .find(END)
        .ok_or_else(|| AppError::Validation("Unclosed storefront order marker".into()))?;
    let end = after_start + relative_end;
    if normalized[after_start + relative_end + END.len()..].contains(START)
        || normalized[..start].contains(END)
    {
        return Err(AppError::Validation(
            "Storefront message must contain exactly one order block".into(),
        ));
    }
    let block = normalized[after_start..end].trim_matches('\n');
    if block.len() > MAX_BLOCK_BYTES {
        return Err(AppError::Validation(
            "Storefront order block exceeds 32 KiB".into(),
        ));
    }
    let lines: Vec<&str> = block.lines().collect();
    if lines.len() != 3 {
        return Err(AppError::Validation(
            "Storefront order block must contain order_id, currency, and items".into(),
        ));
    }
    let order_id = lines[0]
        .strip_prefix("order_id=")
        .ok_or_else(|| AppError::Validation("Invalid storefront order_id line".into()))?
        .trim()
        .to_ascii_lowercase();
    if !valid_uuid(&order_id) {
        return Err(AppError::Validation(
            "Storefront order_id must be a UUID".into(),
        ));
    }
    let currency = lines[1]
        .strip_prefix("currency=")
        .ok_or_else(|| AppError::Validation("Invalid storefront currency line".into()))?
        .trim()
        .to_ascii_uppercase();
    if currency.len() != 3 || !currency.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(AppError::Validation(
            "Storefront currency must be a three-letter ISO code".into(),
        ));
    }
    let item_text = lines[2]
        .strip_prefix("items=")
        .ok_or_else(|| AppError::Validation("Invalid storefront items line".into()))?;
    let parts: Vec<&str> = item_text.split(',').collect();
    if parts.is_empty() || parts.len() > MAX_ITEMS {
        return Err(AppError::Validation(
            "Storefront order must contain 1 to 100 items".into(),
        ));
    }
    let mut seen = HashSet::new();
    let mut items = Vec::with_capacity(parts.len());
    for part in parts {
        let (product_id, quantity) = part
            .split_once(':')
            .ok_or_else(|| AppError::Validation("Invalid storefront item".into()))?;
        if product_id.is_empty()
            || product_id.len() > 128
            || !product_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
            || !seen.insert(product_id)
        {
            return Err(AppError::Validation(
                "Invalid or duplicate storefront product ID".into(),
            ));
        }
        let quantity = Decimal::from_str(quantity)
            .map_err(|_| AppError::Validation("Invalid storefront quantity".into()))?;
        if quantity <= Decimal::ZERO || quantity > MAX_QUANTITY || quantity.scale() > 3 {
            return Err(AppError::Validation(
                "Storefront quantity must be > 0, <= 100, with at most 3 decimals".into(),
            ));
        }
        items.push(IncomingItem {
            product_id: product_id.into(),
            quantity,
        });
    }
    Ok(Some(IncomingOrder {
        order_id,
        currency,
        items,
    }))
}

pub async fn ingest_storefront_message(
    pool: &SqlitePool,
    body: &str,
    message_id: &str,
    customer_jid: &str,
    customer_name: &str,
) -> AppResult<Option<IngestedStorefrontOrder>> {
    let Some(order) = parse_block(body)? else {
        return Ok(None);
    };
    if message_id.trim().is_empty() || customer_jid.trim().is_empty() {
        return Err(AppError::Validation(
            "Storefront message identity is missing".into(),
        ));
    }
    let mut tx = pool.begin().await?;
    let branch = sqlx::query(
        "SELECT branch_id,currency FROM branches
         WHERE is_active=1 AND deleted_at IS NULL ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Validation("No active storefront branch".into()))?;
    let branch_id: String = branch.get("branch_id");
    let currency: String = branch.get("currency");
    if order.currency != currency {
        return Err(AppError::Validation(
            "Storefront order currency does not match the branch".into(),
        ));
    }

    let mut total_minor = 0_i64;
    let mut normalized_items = Vec::with_capacity(order.items.len());
    for item in &order.items {
        let row = sqlx::query(
            "SELECT p.name,p.allow_decimal_quantity,pp.price_minor,pp.currency
             FROM products p
             JOIN storefront_products sp ON sp.product_id=p.product_id AND sp.is_visible=1
             JOIN product_prices pp ON pp.price_id=(
                 SELECT candidate.price_id FROM product_prices candidate
                 WHERE candidate.product_id=p.product_id AND candidate.price_type='selling'
                   AND (candidate.branch_id=? OR candidate.branch_id IS NULL)
                   AND datetime(candidate.effective_from) <= datetime('now')
                   AND (candidate.effective_to IS NULL OR
                        datetime(candidate.effective_to) > datetime('now'))
                 ORDER BY CASE WHEN candidate.branch_id=? THEN 0 ELSE 1 END,
                          datetime(candidate.effective_from) DESC,
                          candidate.price_id DESC LIMIT 1)
             WHERE p.product_id=? AND p.is_active=1 AND p.deleted_at IS NULL",
        )
        .bind(&branch_id)
        .bind(&branch_id)
        .bind(&item.product_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            AppError::Validation(format!(
                "Storefront product {} is unavailable",
                item.product_id
            ))
        })?;
        let allow_decimal: i64 = row.get("allow_decimal_quantity");
        if allow_decimal == 0 && item.quantity.scale() != 0 {
            return Err(AppError::Validation(format!(
                "Product {} requires a whole quantity",
                item.product_id
            )));
        }
        let item_currency: String = row.get("currency");
        if item_currency != currency {
            return Err(AppError::Validation(
                "Storefront product currency mismatch".into(),
            ));
        }
        let unit_price_minor: i64 = row.get("price_minor");
        let line_decimal = Decimal::from_i64(unit_price_minor)
            .ok_or_else(|| AppError::Validation("Invalid product price".into()))?
            * item.quantity;
        let line_total_minor = line_decimal
            .round()
            .to_i64()
            .ok_or_else(|| AppError::Validation("Storefront order total overflow".into()))?;
        total_minor = total_minor
            .checked_add(line_total_minor)
            .ok_or_else(|| AppError::Validation("Storefront order total overflow".into()))?;
        normalized_items.push(NormalizedItem {
            product_id: item.product_id.clone(),
            name: row.get("name"),
            quantity: item.quantity.normalize().to_string(),
            unit_price_minor,
            line_total_minor,
        });
    }

    let normalized = NormalizedOrder {
        schema_version: 1,
        external_ref: order.order_id.clone(),
        branch_id,
        items: normalized_items,
        total_minor,
        currency: currency.clone(),
    };
    let raw_json = serde_json::to_string(&normalized)
        .map_err(|e| AppError::Internal(format!("Normalize storefront order: {e}")))?;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR IGNORE INTO wa_orders
         (order_id,customer_jid,customer_name,message_id,status,raw_json,total_minor,
          currency,product_count,created_at,source,external_ref)
         VALUES (?,?,?,?,'new',?,?,?,?,?,'web_storefront',?)",
    )
    .bind(&order.order_id)
    .bind(customer_jid.trim())
    .bind(customer_name.trim())
    .bind(message_id.trim())
    .bind(&raw_json)
    .bind(total_minor)
    .bind(&currency)
    .bind(order.items.len() as i64)
    .bind(&now)
    .bind(&order.order_id)
    .execute(&mut *tx)
    .await?;
    let row = sqlx::query(
        "SELECT order_id,external_ref,total_minor,currency,raw_json
         FROM wa_orders WHERE source='web_storefront' AND external_ref=?",
    )
    .bind(&order.order_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(IngestedStorefrontOrder {
        order_id: row.get("order_id"),
        external_ref: row.get("external_ref"),
        total_minor: row.get("total_minor"),
        currency: row.get("currency"),
        raw_json: row.get("raw_json"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_storefront_text_is_ignored() {
        assert!(parse_block("normal customer message").unwrap().is_none());
    }

    #[test]
    fn uuid_shape_is_strict() {
        assert!(valid_uuid("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!valid_uuid("550e8400e29b41d4a716446655440000"));
    }
}
