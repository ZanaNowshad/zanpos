use serde_json::{json, Value};
use sqlx::SqlitePool;
use crate::ai::client::ToolDef;
use crate::domain::ai_admin::{ToolPreview, ToolPreviewField};
use crate::errors::{AppError, AppResult};
use crate::db::repositories::{product_repo, report_repo};
use crate::domain::money;
use crate::sync::outbox;

// ── Tool catalogue ─────────────────────────────────────────────────────────────

pub fn all_tool_definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "get_today_summary".into(),
            description: "Get today's sales summary including totals, transaction count, and payment breakdown.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "list_products".into(),
            description: "List all active products with their current prices.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "search_products".into(),
            description: "Search products by name, SKU, or barcode.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search term" }
                },
                "required": ["query"]
            }),
        },
        ToolDef {
            name: "get_product".into(),
            description: "Get full details of a specific product by ID.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" }
                },
                "required": ["product_id"]
            }),
        },
        ToolDef {
            name: "update_product_price".into(),
            description: "Update the selling price of a product. Requires admin confirmation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" },
                    "new_price_minor": { "type": "integer", "description": "New price in minor units (e.g. 1500 = BHD 1.500)" },
                    "reason": { "type": "string", "description": "Reason for price change" }
                },
                "required": ["product_id", "new_price_minor"]
            }),
        },
        ToolDef {
            name: "set_product_active".into(),
            description: "Enable or disable a product. Disabled products don't appear in POS. Requires admin confirmation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" },
                    "is_active": { "type": "boolean" }
                },
                "required": ["product_id", "is_active"]
            }),
        },
        ToolDef {
            name: "update_product_name".into(),
            description: "Rename a product. Requires admin confirmation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" },
                    "new_name": { "type": "string" }
                },
                "required": ["product_id", "new_name"]
            }),
        },
    ]
}

pub fn is_mutation_tool(name: &str) -> bool {
    matches!(name, "update_product_price" | "set_product_active" | "update_product_name")
}

// ── Read-only tool executor ────────────────────────────────────────────────────

pub async fn execute_read_tool(
    pool: &SqlitePool,
    tool_name: &str,
    input: &Value,
    branch_id: &str,
    currency_exp: u32,
) -> AppResult<String> {
    match tool_name {
        "get_today_summary" => {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            let s = report_repo::today_summary(pool, branch_id, &today).await?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            Ok(format!(
                "Today ({}):\n- Transactions: {}\n- Net Total: BHD {}\n- Tax: BHD {}\n- Discounts: BHD {}\n- Cash: BHD {}\n- Card: BHD {}\n- Refunds: {} (BHD {})",
                s.business_date, s.transaction_count,
                fmt(s.net_total_minor), fmt(s.tax_total_minor), fmt(s.discount_total_minor),
                fmt(s.cash_total_minor), fmt(s.card_total_minor),
                s.refund_count, fmt(s.refund_total_minor)
            ))
        }
        "list_products" => {
            let products = product_repo::list_all_active(pool).await?;
            if products.is_empty() {
                return Ok("No active products found.".into());
            }
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let lines: Vec<String> = products.iter().map(|p| {
                format!("- {} (ID: {}) — BHD {} — {}",
                    p.product.name, p.product.product_id,
                    fmt(p.price_minor), p.category_name)
            }).collect();
            Ok(format!("{} active products:\n{}", products.len(), lines.join("\n")))
        }
        "search_products" => {
            let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let products = product_repo::search_products(pool, query, 20).await?;
            if products.is_empty() {
                return Ok(format!("No products found matching '{}'.", query));
            }
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let lines: Vec<String> = products.iter().map(|p| {
                format!("- {} (ID: {}) — BHD {}", p.product.name, p.product.product_id, fmt(p.price_minor))
            }).collect();
            Ok(format!("{} results for '{}':\n{}", products.len(), query, lines.join("\n")))
        }
        "get_product" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            Ok(format!(
                "Product: {}\nID: {}\nSKU: {}\nBarcode: {}\nCategory: {}\nPrice: BHD {}\nActive: {}\nTrack Inventory: {}",
                p.product.name, p.product.product_id,
                p.product.sku.as_deref().unwrap_or("—"),
                p.product.barcode.as_deref().unwrap_or("—"),
                p.category_name,
                fmt(p.price_minor),
                p.product.is_active,
                p.product.track_inventory
            ))
        }
        _ => Err(AppError::Validation(format!("Unknown read tool: {}", tool_name))),
    }
}

// ── Mutation dry-run: build a human-readable preview ──────────────────────────

pub async fn dry_run_mutation(
    pool: &SqlitePool,
    tool_name: &str,
    input: &Value,
    currency_exp: u32,
) -> AppResult<ToolPreview> {
    let fmt = |n: i64| money::format_minor(n, currency_exp);

    match tool_name {
        "update_product_price" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_price = input.get("new_price_minor").and_then(|v| v.as_i64())
                .ok_or_else(|| AppError::Validation("Missing new_price_minor".into()))?;
            let reason = input.get("reason").and_then(|v| v.as_str()).unwrap_or("—");

            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Update selling price of '{}'", p.product.name),
                fields: vec![
                    ToolPreviewField { label: "Product".into(), value: p.product.name.clone() },
                    ToolPreviewField { label: "Current Price".into(), value: format!("BHD {}", fmt(p.price_minor)) },
                    ToolPreviewField { label: "New Price".into(), value: format!("BHD {}", fmt(new_price)) },
                    ToolPreviewField { label: "Reason".into(), value: reason.into() },
                ],
            })
        }
        "set_product_active" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let is_active = input.get("is_active").and_then(|v| v.as_bool())
                .ok_or_else(|| AppError::Validation("Missing is_active".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("{} product '{}'", if is_active { "Enable" } else { "Disable" }, p.product.name),
                fields: vec![
                    ToolPreviewField { label: "Product".into(), value: p.product.name.clone() },
                    ToolPreviewField {
                        label: "Action".into(),
                        value: if is_active { "Enable (show in POS)".into() } else { "Disable (hide from POS)".into() },
                    },
                ],
            })
        }
        "update_product_name" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_name = input.get("new_name").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing new_name".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Rename product"),
                fields: vec![
                    ToolPreviewField { label: "Current Name".into(), value: p.product.name.clone() },
                    ToolPreviewField { label: "New Name".into(), value: new_name.into() },
                ],
            })
        }
        _ => Err(AppError::Validation(format!("Unknown mutation tool: {}", tool_name))),
    }
}

// ── Mutation executor ─────────────────────────────────────────────────────────

pub struct MutationResult {
    pub description: String,
    pub undo_snapshot_json: String,
    pub rollback_tool: String,
    pub rollback_input_json: String,
    pub entity_type: String,
    pub entity_id: String,
}

pub async fn execute_mutation(
    pool: &SqlitePool,
    tool_name: &str,
    input: &Value,
    currency_exp: u32,
) -> AppResult<MutationResult> {
    let fmt = |n: i64| money::format_minor(n, currency_exp);

    match tool_name {
        "update_product_price" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_price = input.get("new_price_minor").and_then(|v| v.as_i64())
                .ok_or_else(|| AppError::Validation("Missing new_price_minor".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let old_price = p.price_minor;

            // Expire current active price and insert new one
            let now = chrono::Utc::now().to_rfc3339();
            let new_price_id = ulid::Ulid::new().to_string();

            sqlx::query(
                "UPDATE product_prices SET effective_to = ?
                 WHERE product_id = ? AND branch_id IS NULL AND price_type = 'selling'
                   AND effective_to IS NULL"
            )
            .bind(&now)
            .bind(product_id)
            .execute(pool)
            .await?;

            sqlx::query(
                "INSERT INTO product_prices (price_id, product_id, branch_id, price_type, price_minor,
                 currency, effective_from, effective_to, created_by_user_id)
                 VALUES (?, ?, NULL, 'selling', ?, 'BHD', ?, NULL, 'AI_ADMIN')"
            )
            .bind(&new_price_id)
            .bind(product_id)
            .bind(new_price)
            .bind(&now)
            .execute(pool)
            .await?;

            // Write audit log
            write_audit(pool, "AI_ADMIN", "product_price_update", product_id,
                &json!({"from": old_price, "to": new_price})).await?;

            // Enqueue price for sync
            let _ = outbox::enqueue_product_price(
                pool, "01JDEVICE0000000000000001", "01JBRANCH0000000000000001",
                &new_price_id, product_id, new_price, "BHD", &now,
                "AI_ADMIN", None, &now,
            ).await;
            // Enqueue updated product row (effective_to changed on old price, but product itself didn't change — just the price)
            // The product entity sync is handled by the price entry above.

            Ok(MutationResult {
                description: format!("Price of '{}' changed from BHD {} to BHD {}",
                    p.product.name, fmt(old_price), fmt(new_price)),
                undo_snapshot_json: json!({ "price_minor": old_price }).to_string(),
                rollback_tool: "update_product_price".into(),
                rollback_input_json: json!({
                    "product_id": product_id, "new_price_minor": old_price
                }).to_string(),
                entity_type: "product_price".into(),
                entity_id: product_id.into(),
            })
        }
        "set_product_active" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let is_active = input.get("is_active").and_then(|v| v.as_bool())
                .ok_or_else(|| AppError::Validation("Missing is_active".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let old_active = p.product.is_active;

            sqlx::query("UPDATE products SET is_active = ?, version = version + 1 WHERE product_id = ?")
                .bind(is_active as i64)
                .bind(product_id)
                .execute(pool)
                .await?;

            write_audit(pool, "AI_ADMIN", "product_status_change", product_id,
                &json!({"from": old_active, "to": is_active})).await?;

            // Enqueue updated product for sync
            {
                let now_ts = chrono::Utc::now().to_rfc3339();
                let updated = product_repo::get_product_by_id(pool, product_id).await;
                if let Ok(Some(up)) = updated {
                    let ca = if up.product.created_at.is_empty() { now_ts.clone() } else { up.product.created_at.clone() };
                    let ua = if up.product.updated_at.is_empty() { now_ts.clone() } else { up.product.updated_at.clone() };
                    let _ = outbox::enqueue_product(
                        pool, "01JDEVICE0000000000000001", "01JBRANCH0000000000000001",
                        product_id, &up.product.category_id, &up.product.name,
                        up.product.sku.as_deref(), up.product.barcode.as_deref(),
                        up.product.description.as_deref(),
                        up.product.track_inventory, up.product.allow_decimal_quantity,
                        up.product.is_active, up.product.tax_rule_id.as_deref(),
                        up.product.cost_minor, &up.product.currency,
                        &ca, &ua, up.product.version,
                    ).await;
                }
            }

            Ok(MutationResult {
                description: format!("Product '{}' {}",
                    p.product.name, if is_active { "enabled" } else { "disabled" }),
                undo_snapshot_json: json!({ "is_active": old_active }).to_string(),
                rollback_tool: "set_product_active".into(),
                rollback_input_json: json!({
                    "product_id": product_id, "is_active": old_active
                }).to_string(),
                entity_type: "product".into(),
                entity_id: product_id.into(),
            })
        }
        "update_product_name" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_name = input.get("new_name").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing new_name".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let old_name = p.product.name.clone();

            sqlx::query("UPDATE products SET name = ?, version = version + 1 WHERE product_id = ?")
                .bind(new_name)
                .bind(product_id)
                .execute(pool)
                .await?;

            write_audit(pool, "AI_ADMIN", "product_rename", product_id,
                &json!({"from": &old_name, "to": new_name})).await?;

            // Enqueue renamed product for sync
            {
                let now_ts = chrono::Utc::now().to_rfc3339();
                let updated = product_repo::get_product_by_id(pool, product_id).await;
                if let Ok(Some(up)) = updated {
                    let ca = if up.product.created_at.is_empty() { now_ts.clone() } else { up.product.created_at.clone() };
                    let ua = if up.product.updated_at.is_empty() { now_ts.clone() } else { up.product.updated_at.clone() };
                    let _ = outbox::enqueue_product(
                        pool, "01JDEVICE0000000000000001", "01JBRANCH0000000000000001",
                        product_id, &up.product.category_id, &up.product.name,
                        up.product.sku.as_deref(), up.product.barcode.as_deref(),
                        up.product.description.as_deref(),
                        up.product.track_inventory, up.product.allow_decimal_quantity,
                        up.product.is_active, up.product.tax_rule_id.as_deref(),
                        up.product.cost_minor, &up.product.currency,
                        &ca, &ua, up.product.version,
                    ).await;
                }
            }

            Ok(MutationResult {
                description: format!("Product renamed from '{}' to '{}'", old_name, new_name),
                undo_snapshot_json: json!({ "name": &old_name }).to_string(),
                rollback_tool: "update_product_name".into(),
                rollback_input_json: json!({
                    "product_id": product_id, "new_name": &old_name
                }).to_string(),
                entity_type: "product".into(),
                entity_id: product_id.into(),
            })
        }
        _ => Err(AppError::Validation(format!("Unknown mutation tool: {}", tool_name))),
    }
}

// ── Undo executor ─────────────────────────────────────────────────────────────

pub async fn execute_undo(
    pool: &SqlitePool,
    rollback_tool: &str,
    rollback_input_json: &str,
    currency_exp: u32,
) -> AppResult<String> {
    let input: Value = serde_json::from_str(rollback_input_json)
        .map_err(|e| AppError::Validation(format!("Invalid rollback input: {}", e)))?;
    let result = execute_mutation(pool, rollback_tool, &input, currency_exp).await?;
    Ok(result.description)
}

// ── Audit helper ──────────────────────────────────────────────────────────────

async fn write_audit(
    pool: &SqlitePool,
    actor_user_id: &str,
    event_type: &str,
    entity_id: &str,
    after: &serde_json::Value,
) -> AppResult<()> {
    let id = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let hash = format!("{:x}", md5_simple(&format!("{}{}{}", id, event_type, now)));
    sqlx::query(
        "INSERT INTO audit_logs
         (audit_log_id, event_type, entity_type, entity_id, actor_user_id,
          actor_type, after_json, created_at, hash)
         VALUES (?, ?, 'product', ?, ?, 'ai_agent', ?, ?, ?)"
    )
    .bind(&id)
    .bind(event_type)
    .bind(entity_id)
    .bind(actor_user_id)
    .bind(after.to_string())
    .bind(&now)
    .bind(&hash)
    .execute(pool)
    .await?;
    Ok(())
}

fn md5_simple(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}
