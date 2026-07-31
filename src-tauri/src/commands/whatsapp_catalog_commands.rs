//! WhatsApp Business Catalog + Commerce.
//!
//! Three capability groups, all bridged through the Baileys sidecar:
//!   • Catalog READ  — list own products, peek another business, collections.
//!   • Catalog WRITE — create/update/delete/sync products (higher ban risk;
//!                     writes are rate-limited and manager/owner gated).
//!   • Commerce      — resolve incoming WhatsApp orders and send product cards.
//!
//! The linked WhatsApp number MUST be a WhatsApp Business account with a catalog
//! for any of this to return data. Read routes are safe; bulk writes over the
//! unofficial Baileys transport carry a real risk of the number being flagged —
//! callers should prefer Meta's official Catalog API for large-scale writes.

use crate::commands::rbac;
use crate::commands::whatsapp_commands::{read_sidecar_token, SIDECAR_URL};
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::State;

// ── Feature flag ──────────────────────────────────────────────────────────────
// The entire WhatsApp Commerce suite is OFF by default and must be explicitly
// enabled in Settings. Every command below refuses to run while disabled, so
// turning it off fully stops all catalog/order traffic (no sidecar calls, no
// order capture, no AI awareness).

pub(crate) async fn commerce_enabled(pool: &sqlx::SqlitePool) -> bool {
    sqlx::query_scalar::<_, String>(
        "SELECT value FROM app_config WHERE key = 'whatsapp_commerce_enabled'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(|v| v == "1" || v == "true")
    .unwrap_or(false)
}

async fn require_commerce_enabled(pool: &sqlx::SqlitePool) -> AppResult<()> {
    if commerce_enabled(pool).await {
        Ok(())
    } else {
        Err(AppError::Validation(
            "WhatsApp Commerce is turned off. Enable it in Settings → WhatsApp Commerce.".into(),
        ))
    }
}

/// Orders can originate from the WhatsApp catalog OR the public storefront.
/// The order screens must work when EITHER feature is on — gating them on the
/// catalog toggle alone hid storefront orders whenever commerce was off.
pub(crate) async fn orders_enabled(pool: &sqlx::SqlitePool) -> bool {
    if commerce_enabled(pool).await {
        return true;
    }
    sqlx::query_scalar::<_, String>("SELECT value FROM app_config WHERE key = 'storefront_enabled'")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false)
}

async fn require_orders_enabled(pool: &sqlx::SqlitePool) -> AppResult<()> {
    if orders_enabled(pool).await {
        Ok(())
    } else {
        Err(AppError::Validation(
            "Orders are turned off. Enable WhatsApp Commerce or the public Storefront in Settings."
                .into(),
        ))
    }
}

#[tauri::command]
pub async fn whatsapp_commerce_get_enabled(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<bool> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    Ok(commerce_enabled(&state.db).await)
}

/// True when the Orders surfaces should be visible: WhatsApp Commerce OR the
/// public Storefront is enabled. Used by the POS sidebar button.
#[tauri::command]
pub async fn whatsapp_orders_get_enabled(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<bool> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    Ok(orders_enabled(&state.db).await)
}

#[tauri::command]
pub async fn whatsapp_commerce_set_enabled(
    actor_user_id: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO app_config (key, value, updated_at) VALUES ('whatsapp_commerce_enabled', ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(if enabled { "1" } else { "0" })
    .bind(&now)
    .execute(&state.db)
    .await?;
    crate::commands::sync_commands::schedule_immediate_sync(&state);
    Ok(())
}

// ── HTTP client factory (matches whatsapp_commands convention) ────────────────
fn sidecar_client(timeout_secs: u64) -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .unwrap_or_default()
}

// ── Types ─────────────────────────────────────────────────────────────────────

// Order line item resolved from a WhatsApp order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaOrderProduct {
    pub id: String,
    pub name: String,
    pub quantity: f64,
    pub price: Option<i64>,
    pub currency: Option<String>,
    pub image_url: Option<String>,
}

fn parse_stored_order_products(raw: &str) -> Vec<WaOrderProduct> {
    if let Ok(products) = serde_json::from_str::<Vec<WaOrderProduct>>(raw) {
        return products;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let currency = value
        .get("currency")
        .and_then(|item| item.as_str())
        .map(str::to_string);
    value
        .get("items")
        .and_then(|items| items.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let id = item.get("product_id")?.as_str()?.to_string();
            let name = item.get("name")?.as_str()?.to_string();
            let quantity = item
                .get("quantity")
                .and_then(|quantity| {
                    quantity
                        .as_f64()
                        .or_else(|| quantity.as_str().and_then(|text| text.parse().ok()))
                })
                .filter(|quantity| quantity.is_finite() && *quantity > 0.0)?;
            Some(WaOrderProduct {
                id,
                name,
                quantity,
                price: item
                    .get("unit_price_minor")
                    .and_then(|price| price.as_i64()),
                currency: currency.clone(),
                image_url: None,
            })
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct SidecarOrderResponse {
    ok: bool,
    #[serde(default)]
    order: Option<SidecarOrder>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SidecarOrder {
    currency: Option<String>,
    total: Option<i64>,
    #[serde(default)]
    products: Vec<WaOrderProduct>,
}

#[derive(Debug, Serialize)]
pub struct WaOrder {
    pub order_id: String,
    pub customer_jid: String,
    pub customer_name: Option<String>,
    pub status: String,
    pub total_minor: Option<i64>,
    pub currency: Option<String>,
    pub product_count: i64,
    pub created_at: String,
    pub linked_sale_id: Option<String>,
    pub products: Vec<WaOrderProduct>,
}

// ── Catalog READ commands ─────────────────────────────────────────────────────

/// Map a sidecar-origin failure to a Validation error so the ACTUAL reason
/// reaches the UI. (AppError::Internal is deliberately masked as a generic
/// "unexpected error" message, which hides connection/business-account issues.)
fn sidecar_err(context: &str, detail: impl std::fmt::Display) -> AppError {
    tracing::warn!("WhatsApp commerce — {context}: {detail}");
    AppError::Validation(format!("WhatsApp {context}: {detail}"))
}

// ── Commerce: orders ──────────────────────────────────────────────────────────

/// Resolve a WhatsApp order via the sidecar and upsert it into wa_orders.
/// Best-effort: called from the message poll; failures are logged, never fatal.
pub(crate) async fn resolve_and_store_order(
    state: &AppState,
    token: &str,
    order_id: &str,
    order_token: &str,
    message_id: &str,
    customer_jid: &str,
    customer_name: &str,
) {
    // Respect the master switch — no order capture while the feature is off.
    if !commerce_enabled(&state.db).await {
        return;
    }
    // Skip if we already have it.
    if let Ok(Some(_)) =
        sqlx::query_scalar::<_, String>("SELECT order_id FROM wa_orders WHERE order_id = ?")
            .bind(order_id)
            .fetch_optional(&state.db)
            .await
    {
        return;
    }

    let url = format!(
        "{SIDECAR_URL}/orders/{}?token={}",
        urlencoding_lite(order_id),
        urlencoding_lite(order_token)
    );
    let resp = match sidecar_client(20)
        .get(url)
        .header("X-Sidecar-Token", token)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("resolve_and_store_order: sidecar unreachable: {e}");
            return;
        }
    };
    let body: SidecarOrderResponse = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("resolve_and_store_order: bad response: {e}");
            return;
        }
    };
    if !body.ok {
        tracing::warn!(
            "resolve_and_store_order: {}",
            body.error.unwrap_or_else(|| "unknown".into())
        );
        return;
    }
    let Some(order) = body.order else { return };
    let raw = serde_json::to_string(&order.products).unwrap_or_else(|_| "[]".into());
    let now = chrono::Utc::now().to_rfc3339();
    let inserted = sqlx::query(
        "INSERT OR IGNORE INTO wa_orders
         (order_id, customer_jid, customer_name, message_id, status, raw_json, total_minor, currency, product_count, created_at)
         VALUES (?, ?, ?, ?, 'new', ?, ?, ?, ?, ?)",
    )
    .bind(order_id)
    .bind(customer_jid)
    .bind(customer_name)
    .bind(message_id)
    .bind(&raw)
    .bind(order.total)
    .bind(&order.currency)
    .bind(order.products.len() as i64)
    .bind(&now)
    .execute(&state.db)
    .await;
    // INSERT OR IGNORE, so only count an order the first time we store it —
    // the message poll can hand us the same order id more than once.
    if matches!(inserted, Ok(ref done) if done.rows_affected() > 0) {
        crate::diagnostics::record_event(
            &state.db,
            "wa_order_received",
            Some(serde_json::json!({ "lines": order.products.len() })),
        )
        .await;
    }
}

#[tauri::command]
pub async fn whatsapp_order_list(
    actor_user_id: String,
    status: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<Vec<WaOrder>> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    require_orders_enabled(&state.db).await?;
    let filter = status.filter(|s| !s.is_empty());
    let rows = if let Some(s) = &filter {
        sqlx::query_as::<_, (String, String, Option<String>, String, Option<i64>, Option<String>, i64, String, Option<String>, String)>(
            "SELECT order_id, customer_jid, customer_name, status, total_minor, currency, product_count, created_at, linked_sale_id, raw_json
             FROM wa_orders WHERE status = ? ORDER BY created_at DESC LIMIT 200",
        )
        .bind(s)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, (String, String, Option<String>, String, Option<i64>, Option<String>, i64, String, Option<String>, String)>(
            "SELECT order_id, customer_jid, customer_name, status, total_minor, currency, product_count, created_at, linked_sale_id, raw_json
             FROM wa_orders ORDER BY created_at DESC LIMIT 200",
        )
        .fetch_all(&state.db)
        .await?
    };
    Ok(rows
        .into_iter()
        .map(
            |(
                order_id,
                customer_jid,
                customer_name,
                status,
                total_minor,
                currency,
                product_count,
                created_at,
                linked_sale_id,
                raw_json,
            )| {
                let products = parse_stored_order_products(&raw_json);
                WaOrder {
                    order_id,
                    customer_jid,
                    customer_name,
                    status,
                    total_minor,
                    currency,
                    product_count,
                    created_at,
                    linked_sale_id,
                    products,
                }
            },
        )
        .collect())
}

/// Mark an order reviewed/fulfilled/cancelled. When linking to a POS sale,
/// pass `linked_sale_id`; the fulfilment itself (creating the sale) is done in
/// the POS UI where the cart lives — this records the outcome atomically.
#[tauri::command]
pub async fn whatsapp_order_update_status(
    actor_user_id: String,
    order_id: String,
    status: String,
    linked_sale_id: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    require_orders_enabled(&state.db).await?;
    let allowed = ["new", "reviewed", "fulfilled", "cancelled"];
    if !allowed.contains(&status.as_str()) {
        return Err(AppError::Validation(format!(
            "Invalid order status: {status}"
        )));
    }
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE wa_orders
         SET status = ?, reviewed_at = ?, reviewed_by = ?, linked_sale_id = COALESCE(?, linked_sale_id), version = version + 1
         WHERE order_id = ?",
    )
    .bind(&status)
    .bind(&now)
    .bind(&actor_user_id)
    .bind(&linked_sale_id)
    .bind(&order_id)
    .execute(&state.db)
    .await?;
    // Only fulfilment closes the order loop that `wa_order_received` opens —
    // the pair is what makes "orders die when unseen" measurable rather than
    // anecdotal. Reviewed/cancelled are deliberately not counted as fulfilled.
    if status == "fulfilled" {
        crate::diagnostics::record_event(
            &state.db,
            "wa_order_fulfilled",
            Some(serde_json::json!({ "linked_to_sale": linked_sale_id.is_some() })),
        )
        .await;
    }
    Ok(())
}

/// Match a WhatsApp order's line items to POS products (by retailer_id/barcode,
/// then exact name) so the POS can open a pre-filled cart. Returns unresolved
/// items so the cashier can map them manually.
#[derive(Debug, Serialize)]
pub struct WaOrderMatch {
    pub matched: Vec<WaMatchedLine>,
    pub unmatched: Vec<WaOrderProduct>,
}

#[derive(Debug, Serialize)]
pub struct WaMatchedLine {
    pub product_id: String,
    pub name: String,
    pub barcode: Option<String>,
    pub quantity: f64,
}

#[tauri::command]
pub async fn whatsapp_order_match(
    actor_user_id: String,
    order_id: String,
    state: State<'_, AppState>,
) -> AppResult<WaOrderMatch> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    require_orders_enabled(&state.db).await?;
    let raw: Option<String> =
        sqlx::query_scalar("SELECT raw_json FROM wa_orders WHERE order_id = ?")
            .bind(&order_id)
            .fetch_optional(&state.db)
            .await?
            .flatten();
    let items = raw
        .as_deref()
        .map(parse_stored_order_products)
        .unwrap_or_default();

    let mut matched = Vec::new();
    let mut unmatched = Vec::new();
    for item in items {
        // Try WhatsApp mapping (wa_product_id → POS product), then barcode, then name.
        let pos: Option<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT p.product_id, p.name, p.barcode FROM products p
             LEFT JOIN wa_catalog_products wcp ON wcp.product_id = p.product_id
             WHERE wcp.wa_product_id = ?1 OR p.barcode = ?1 OR p.name = ?2
             LIMIT 1",
        )
        .bind(&item.id)
        .bind(&item.name)
        .fetch_optional(&state.db)
        .await?;
        match pos {
            Some((product_id, name, barcode)) => matched.push(WaMatchedLine {
                product_id,
                name,
                barcode,
                quantity: item.quantity.max(1.0),
            }),
            None => unmatched.push(item),
        }
    }
    Ok(WaOrderMatch { matched, unmatched })
}

/// Send a free-text WhatsApp message to the customer who placed an order.
/// Used by the POS Orders page ("Message customer"). Resolves the phone from the
/// stored order's customer_jid and posts to the sidecar /send endpoint.
#[tauri::command]
pub async fn whatsapp_order_message(
    actor_user_id: String,
    order_id: String,
    message: String,
    state: State<'_, AppState>,
) -> AppResult<bool> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    require_orders_enabled(&state.db).await?;
    if message.trim().is_empty() {
        return Err(AppError::Validation("Message text is required".into()));
    }
    let jid: Option<String> =
        sqlx::query_scalar("SELECT customer_jid FROM wa_orders WHERE order_id = ?")
            .bind(&order_id)
            .fetch_optional(&state.db)
            .await?
            .flatten();
    let jid = jid.ok_or_else(|| AppError::NotFound("Order not found".into()))?;
    // customer_jid looks like "97333050666@s.whatsapp.net" — strip to bare phone.
    let phone = jid.split('@').next().unwrap_or("").to_string();
    if phone.is_empty() {
        return Err(AppError::Validation(
            "Order has no valid customer number".into(),
        ));
    }
    let token = read_sidecar_token(&state);
    let resp = sidecar_client(20)
        .post(format!("{SIDECAR_URL}/send"))
        .header("X-Sidecar-Token", &token)
        .json(&serde_json::json!({ "to": phone, "message": message }))
        .send()
        .await
        .map_err(|e| sidecar_err("service is unreachable (is WhatsApp running?)", e))?;
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    Ok(body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false))
}

#[cfg(test)]
mod storefront_order_tests {
    use super::*;

    #[test]
    fn parses_normalized_web_storefront_order_products() {
        let raw = r#"{
          "currency":"BHD",
          "items":[{
            "product_id":"P-1",
            "name":"Coffee",
            "quantity":"1.5",
            "unit_price_minor":1250,
            "line_total_minor":1875
          }]
        }"#;

        let products = parse_stored_order_products(raw);
        assert_eq!(products.len(), 1);
        assert_eq!(products[0].id, "P-1");
        assert_eq!(products[0].quantity, 1.5);
        assert_eq!(products[0].price, Some(1250));
        assert_eq!(products[0].currency.as_deref(), Some("BHD"));
    }
}

#[tauri::command]
pub async fn whatsapp_send_product(
    actor_user_id: String,
    to: String,
    wa_product_id: String,
    state: State<'_, AppState>,
) -> AppResult<bool> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    require_commerce_enabled(&state.db).await?;
    if to.trim().is_empty() || wa_product_id.trim().is_empty() {
        return Err(AppError::Validation(
            "Recipient and product ID are required".into(),
        ));
    }
    let token = read_sidecar_token(&state);
    let resp = sidecar_client(120)
        .post(format!("{SIDECAR_URL}/send-product"))
        .header("X-Sidecar-Token", &token)
        .json(&serde_json::json!({ "to": to, "product_id": wa_product_id }))
        .send()
        .await
        .map_err(|e| sidecar_err("service is unreachable (is WhatsApp running?)", e))?;
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    Ok(body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Minimal percent-encoding for path/query segments (avoids a new dependency).
/// Encodes everything that isn't an unreserved URL char.
fn urlencoding_lite(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencoding_handles_jid_and_tokens() {
        assert_eq!(
            urlencoding_lite("97333050666@s.whatsapp.net"),
            "97333050666%40s.whatsapp.net"
        );
        // base64 tokens contain +, /, = which must be encoded for a query string.
        assert_eq!(urlencoding_lite("aB+/9=="), "aB%2B%2F9%3D%3D");
    }
}
