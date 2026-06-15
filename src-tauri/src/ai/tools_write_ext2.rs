//! Phase-3 extension mutation tools: delete, bulk-ops, suppliers, POs, and misc.

pub use crate::ai::tools::MutationResult;
use crate::domain::ai_admin::{ToolPreview, ToolPreviewField};
use crate::errors::{AppError, AppResult};
use sqlx::Row;
use sqlx::SqlitePool;

fn prev(name: &str, desc: &str, fields: Vec<(&str, String)>) -> ToolPreview {
    ToolPreview {
        tool_name: name.into(),
        description: desc.into(),
        fields: fields
            .into_iter()
            .map(|(l, v)| ToolPreviewField {
                label: l.into(),
                value: v,
            })
            .collect(),
    }
}

fn ok_mut(desc: &str, entity_type: &str, entity_id: &str) -> AppResult<MutationResult> {
    Ok(MutationResult {
        description: desc.into(),
        undo_snapshot_json: "{}".into(),
        rollback_tool: String::new(),
        rollback_input_json: "{}".into(),
        entity_type: entity_type.into(),
        entity_id: entity_id.into(),
    })
}

fn str_v(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}
fn req_v(v: &serde_json::Value, key: &str) -> AppResult<String> {
    let s = str_v(v, key);
    if s.is_empty() {
        Err(AppError::Validation(format!("{key} is required")))
    } else {
        Ok(s)
    }
}

async fn audit2(pool: &SqlitePool, event: &str, entity_id: &str, after: &str) {
    let id = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let _ = sqlx::query(
        "INSERT INTO audit_logs (audit_log_id, event_type, entity_type, entity_id,
         actor_user_id, actor_type, after_json, created_at, hash)
         VALUES (?, ?, 'ai_mutation', ?, 'AI_ADMIN', 'ai_agent', ?, ?, 'ai')",
    )
    .bind(&id)
    .bind(event)
    .bind(entity_id)
    .bind(after)
    .bind(&now)
    .execute(pool)
    .await;
}

async fn product_name(pool: &SqlitePool, id: &str) -> String {
    sqlx::query_scalar::<_, Option<String>>("SELECT name FROM products WHERE product_id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten()
        .unwrap_or_else(|| id.to_string())
}

// ── Dry-run previews ──────────────────────────────────────────────────────────

pub async fn dry_run(
    pool: &SqlitePool,
    tool_name: &str,
    input: &serde_json::Value,
    currency_exp: u32,
) -> AppResult<ToolPreview> {
    match tool_name {
        "delete_product" => {
            let id = req_v(input, "product_id")?;
            let name = product_name(pool, &id).await;
            Ok(prev(tool_name, "Permanently delete a product", vec![
                ("Product", name),
                ("⚠ Warning", "Blocked if this product has any sales history".into()),
            ]))
        }
        "delete_category" => Ok(prev(tool_name, "Delete a category", vec![
            ("Category ID", req_v(input, "category_id")?),
            ("⚠ Warning", "Blocked if any products still belong to it".into()),
        ])),
        "delete_tax_rule" => Ok(prev(tool_name, "Delete a tax rule", vec![
            ("Tax Rule ID", req_v(input, "tax_rule_id")?),
            ("⚠ Warning", "Blocked if any products reference it".into()),
        ])),
        "delete_user" => Ok(prev(tool_name, "Delete a staff account", vec![
            ("User ID", req_v(input, "user_id")?),
            ("⚠ Warning", "Blocked if user has shifts or sales history".into()),
        ])),
        "update_delivery_details" => Ok(prev(tool_name, "Update delivery details", vec![
            ("Delivery ID", req_v(input, "delivery_id")?),
            ("Rider", str_v(input, "delivery_staff_name")),
            ("Contact", str_v(input, "contact_number")),
        ])),
        "reassign_delivery_rider" => Ok(prev(tool_name, "Reassign delivery rider", vec![
            ("Delivery ID", req_v(input, "delivery_id")?),
            ("New Rider", req_v(input, "rider_name")?),
        ])),
        "batch_dispatch_deliveries" => {
            let ids = input.get("delivery_ids")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            Ok(prev(tool_name, "Dispatch multiple deliveries", vec![
                ("Count", format!("{ids} delivery order(s)")),
            ]))
        }
        "duplicate_product" => {
            let id = req_v(input, "product_id")?;
            let name = product_name(pool, &id).await;
            Ok(prev(tool_name, "Clone a product", vec![
                ("Source", name),
                ("New Name", req_v(input, "new_name")?),
            ]))
        }
        "send_whatsapp_to_customer" => Ok(prev(tool_name, "Send WhatsApp message", vec![
            ("Phone/Customer", str_v(input, "phone")),
            ("Message", req_v(input, "message")?),
        ])),
        "reset_user_pin" => Ok(prev(tool_name, "Reset staff PIN", vec![
            ("User ID", req_v(input, "user_id")?),
            ("New PIN", "●●●●".into()),
        ])),
        "lock_user" => Ok(prev(tool_name, "Lock staff account", vec![
            ("User ID", req_v(input, "user_id")?),
            ("Effect", "Account disabled — cannot log in".into()),
        ])),
        "unlock_user" => Ok(prev(tool_name, "Unlock staff account", vec![
            ("User ID", req_v(input, "user_id")?),
            ("Effect", "Account re-enabled".into()),
        ])),
        "bulk_deactivate_products" => Ok(prev(tool_name, "Deactivate all products in category", vec![
            ("Category ID", req_v(input, "category_id")?),
        ])),
        "bulk_activate_products" => Ok(prev(tool_name, "Activate all products in category", vec![
            ("Category ID", req_v(input, "category_id")?),
        ])),
        "bulk_set_category" => Ok(prev(tool_name, "Move products to new category", vec![
            ("From", req_v(input, "from_category_id")?),
            ("To", req_v(input, "to_category_id")?),
        ])),
        "bulk_set_tax_rule" => Ok(prev(tool_name, "Apply tax rule to category", vec![
            ("Category ID", req_v(input, "category_id")?),
            ("Tax Rule ID", req_v(input, "tax_rule_id")?),
        ])),
        "bulk_update_reorder_point" => Ok(prev(tool_name, "Set reorder point for category", vec![
            ("Category ID", req_v(input, "category_id")?),
            ("Reorder Point", format!("{}", input.get("reorder_point").and_then(|v| v.as_f64()).unwrap_or(0.0))),
        ])),
        "bulk_update_cost" => {
            let count = input.get("updates")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            Ok(prev(tool_name, "Update cost price for multiple products", vec![
                ("Products", format!("{count} product(s)")),
            ]))
        }
        "create_supplier" => Ok(prev(tool_name, "Add a new supplier", vec![
            ("Name", req_v(input, "name")?),
            ("Phone", str_v(input, "phone")),
        ])),
        "update_supplier" => Ok(prev(tool_name, "Update supplier details", vec![
            ("Supplier ID", req_v(input, "supplier_id")?),
            ("Name", str_v(input, "name")),
        ])),
        "create_purchase_order" => {
            let lines = input.get("lines")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            Ok(prev(tool_name, "Create purchase order", vec![
                ("Supplier", str_v(input, "supplier_id")),
                ("Lines", format!("{lines} item(s)")),
            ]))
        }
        "update_purchase_order" => Ok(prev(tool_name, "Update purchase order status", vec![
            ("PO ID", req_v(input, "po_id")?),
            ("Status", str_v(input, "status")),
        ])),
        "vacuum_database" => Ok(prev(tool_name, "VACUUM SQLite database", vec![
            ("Effect", "Reclaims disk space and defragments the DB file".into()),
            ("Duration", "May take a few seconds. Read-only during VACUUM.".into()),
        ])),
        "create_customer_note" => Ok(prev(tool_name, "Attach note to customer", vec![
            ("Customer ID", req_v(input, "customer_id")?),
            ("Note", req_v(input, "note")?),
        ])),
        other => {
            crate::ai::tools_write_ext3::dry_run(pool, other, input, currency_exp).await
        }
    }
}

// ── Mutation executors ────────────────────────────────────────────────────────

pub async fn execute(
    pool: &SqlitePool,
    tool_name: &str,
    input: &serde_json::Value,
    currency_exp: u32,
) -> AppResult<MutationResult> {
    match tool_name {
        "delete_product" => {
            let id = req_v(input, "product_id")?;
            let has_sales: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM sale_items WHERE product_id = ?")
                    .bind(&id)
                    .fetch_one(pool)
                    .await?;
            if has_sales > 0 {
                return Err(AppError::Validation(format!(
                    "Cannot delete product {id} — it has {has_sales} sales records. Use set_product_active instead."
                )));
            }
            let name = product_name(pool, &id).await;
            sqlx::query("DELETE FROM product_barcodes WHERE product_id = ?")
                .bind(&id).execute(pool).await?;
            sqlx::query("DELETE FROM products WHERE product_id = ?")
                .bind(&id).execute(pool).await?;
            audit2(pool, "product_deleted", &id, &format!("{{\"name\":\"{name}\"}}")).await;
            ok_mut(&format!("Product '{name}' deleted."), "product", &id)
        }

        "delete_category" => {
            let id = req_v(input, "category_id")?;
            let product_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE category_id = ?")
                    .bind(&id).fetch_one(pool).await?;
            if product_count > 0 {
                return Err(AppError::Validation(format!(
                    "Category has {product_count} product(s). Reassign them before deleting."
                )));
            }
            sqlx::query("DELETE FROM categories WHERE category_id = ?")
                .bind(&id).execute(pool).await?;
            audit2(pool, "category_deleted", &id, "{}").await;
            ok_mut(&format!("Category {id} deleted."), "category", &id)
        }

        "delete_tax_rule" => {
            let id = req_v(input, "tax_rule_id")?;
            let in_use: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE tax_rule_id = ?")
                    .bind(&id).fetch_one(pool).await?;
            if in_use > 0 {
                return Err(AppError::Validation(format!(
                    "Tax rule is assigned to {in_use} product(s). Reassign them first."
                )));
            }
            sqlx::query("DELETE FROM tax_rules WHERE tax_rule_id = ?")
                .bind(&id).execute(pool).await?;
            audit2(pool, "tax_rule_deleted", &id, "{}").await;
            ok_mut(&format!("Tax rule {id} deleted."), "tax_rule", &id)
        }

        "delete_user" => {
            let id = req_v(input, "user_id")?;
            let has_shifts: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM shifts WHERE cashier_user_id = ?")
                    .bind(&id).fetch_one(pool).await?;
            let has_sales: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM sales WHERE cashier_user_id = ?")
                    .bind(&id).fetch_one(pool).await?;
            if has_shifts > 0 || has_sales > 0 {
                return Err(AppError::Validation(
                    "Cannot delete user with shift or sales history. Set is_active=false instead.".into(),
                ));
            }
            sqlx::query("DELETE FROM users WHERE user_id = ?")
                .bind(&id).execute(pool).await?;
            audit2(pool, "user_deleted", &id, "{}").await;
            ok_mut(&format!("User {id} deleted."), "user", &id)
        }

        "update_delivery_details" => {
            let id = req_v(input, "delivery_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            let mut parts: Vec<String> = Vec::new();
            macro_rules! set_if {
                ($field:expr, $col:expr) => {
                    if let Some(v) = input.get($field).and_then(|x| x.as_str()).filter(|s| !s.is_empty()) {
                        sqlx::query(&format!("UPDATE delivery_orders SET {} = ?, updated_at = ? WHERE delivery_id = ?", $col))
                            .bind(v).bind(&now).bind(&id).execute(pool).await?;
                        parts.push(format!("{}: {v}", $col));
                    }
                };
            }
            set_if!("delivery_staff_name", "delivery_staff_name");
            set_if!("contact_number", "contact_number");
            set_if!("address_text", "address_text");
            set_if!("house_number", "house_number");
            set_if!("area", "area");
            set_if!("delivery_note", "delivery_note");
            if parts.is_empty() {
                return Err(AppError::Validation("No fields to update.".into()));
            }
            audit2(pool, "delivery_updated", &id, &format!("{{\"changes\":\"{}\"}}", parts.join(", "))).await;
            ok_mut(&format!("Delivery {id} updated: {}.", parts.join(", ")), "delivery", &id)
        }

        "reassign_delivery_rider" => {
            let id = req_v(input, "delivery_id")?;
            let rider = req_v(input, "rider_name")?;
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE delivery_orders SET delivery_staff_name = ?, updated_at = ? WHERE delivery_id = ?")
                .bind(&rider).bind(&now).bind(&id).execute(pool).await?;
            audit2(pool, "delivery_rider_reassigned", &id, &format!("{{\"rider\":\"{rider}\"}}")).await;
            ok_mut(&format!("Delivery {id} reassigned to {rider}."), "delivery", &id)
        }

        "batch_dispatch_deliveries" => {
            let ids = input
                .get("delivery_ids")
                .and_then(|v| v.as_array())
                .ok_or_else(|| AppError::Validation("delivery_ids required".into()))?
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>();
            if ids.is_empty() {
                return Err(AppError::Validation("delivery_ids must not be empty".into()));
            }
            let now = chrono::Utc::now().to_rfc3339();
            let mut dispatched = 0usize;
            for id in &ids {
                let rows = sqlx::query(
                    "UPDATE delivery_orders SET delivery_status = 'dispatched', updated_at = ? WHERE delivery_id = ? AND delivery_status = 'pending'",
                )
                .bind(&now).bind(id).execute(pool).await?.rows_affected();
                if rows > 0 { dispatched += 1; }
            }
            ok_mut(&format!("{dispatched} delivery/deliveries dispatched."), "delivery_batch", "batch")
        }

        "duplicate_product" => {
            let src_id = req_v(input, "product_id")?;
            let new_name = req_v(input, "new_name")?;
            let row = sqlx::query(
                "SELECT price_minor, cost_minor, category_id, tax_rule_id,
                         track_stock, reorder_point, is_active
                 FROM products WHERE product_id = ?",
            )
            .bind(&src_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Product {src_id} not found")))?;
            let new_id = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT INTO products (product_id, name, price_minor, cost_minor, category_id,
                 tax_rule_id, track_stock, reorder_point, is_active, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&new_id)
            .bind(&new_name)
            .bind(row.try_get::<Option<i64>, _>("price_minor").ok().flatten().unwrap_or(0))
            .bind(row.try_get::<Option<i64>, _>("cost_minor").ok().flatten())
            .bind(row.try_get::<Option<String>, _>("category_id").ok().flatten())
            .bind(row.try_get::<Option<String>, _>("tax_rule_id").ok().flatten())
            .bind(row.try_get::<Option<i64>, _>("track_stock").ok().flatten().unwrap_or(0))
            .bind(row.try_get::<Option<f64>, _>("reorder_point").ok().flatten())
            .bind(1i64)
            .bind(&now)
            .bind(&now)
            .execute(pool).await?;
            audit2(pool, "product_duplicated", &new_id, &format!("{{\"source\":\"{src_id}\",\"name\":\"{new_name}\"}}")).await;
            ok_mut(&format!("Product '{new_name}' created as clone of {src_id}."), "product", &new_id)
        }

        "send_whatsapp_to_customer" => {
            let message = req_v(input, "message")?;
            let phone = if let Some(p) = input.get("phone").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                p.to_string()
            } else {
                let cid = req_v(input, "customer_id")?;
                sqlx::query_scalar::<_, Option<String>>("SELECT phone FROM customers WHERE customer_id = ?")
                    .bind(&cid)
                    .fetch_optional(pool).await?
                    .flatten()
                    .ok_or_else(|| AppError::Validation("Customer has no phone number on file.".into()))?
            };
            let token_path = std::env::var("APPDATA").unwrap_or_default();
            let token = std::fs::read_to_string(
                std::path::Path::new(&token_path)
                    .join("com.super.zanpos")
                    .join("wa-session")
                    .join(".sidecar_token"),
            )
            .unwrap_or_default();
            let client = reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .unwrap_or_default();
            let resp = client
                .post("http://127.0.0.1:3131/send")
                .header("X-Sidecar-Token", token.trim())
                .json(&serde_json::json!({ "to": phone, "message": message }))
                .send()
                .await
                .map_err(|e| AppError::Internal(format!("WhatsApp sidecar unreachable: {e}")))?;
            if !resp.status().is_success() {
                return Err(AppError::Internal(format!("Sidecar error: {}", resp.status())));
            }
            ok_mut(&format!("WhatsApp message sent to {phone}."), "whatsapp", &phone)
        }

        "reset_user_pin" => {
            let uid = req_v(input, "user_id")?;
            let pin = req_v(input, "new_pin")?;
            if pin.len() < 4 || pin.len() > 6 || !pin.chars().all(|c| c.is_ascii_digit()) {
                return Err(AppError::Validation("PIN must be 4–6 digits.".into()));
            }
            let hash = crate::db::repositories::auth_repo::hash_pin(&pin)
                .map_err(|e| AppError::Internal(format!("PIN hash error: {e}")))?;
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE users SET pin_hash = ?, updated_at = ? WHERE user_id = ?")
                .bind(&hash).bind(&now).bind(&uid).execute(pool).await?;
            audit2(pool, "user_pin_reset", &uid, "{}").await;
            ok_mut(&format!("PIN reset for user {uid}."), "user", &uid)
        }

        "lock_user" => {
            let uid = req_v(input, "user_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE users SET is_active = 0, updated_at = ? WHERE user_id = ?")
                .bind(&now).bind(&uid).execute(pool).await?;
            audit2(pool, "user_locked", &uid, "{}").await;
            ok_mut(&format!("User {uid} locked."), "user", &uid)
        }

        "unlock_user" => {
            let uid = req_v(input, "user_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE users SET is_active = 1, updated_at = ? WHERE user_id = ?")
                .bind(&now).bind(&uid).execute(pool).await?;
            audit2(pool, "user_unlocked", &uid, "{}").await;
            ok_mut(&format!("User {uid} unlocked."), "user", &uid)
        }

        "bulk_deactivate_products" => {
            let cid = req_v(input, "category_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            let count = sqlx::query(
                "UPDATE products SET is_active = 0, updated_at = ? WHERE category_id = ? AND is_active = 1",
            )
            .bind(&now).bind(&cid).execute(pool).await?.rows_affected();
            audit2(pool, "bulk_deactivate", &cid, &format!("{{\"count\":{count}}}")).await;
            ok_mut(&format!("{count} product(s) deactivated in category {cid}."), "category", &cid)
        }

        "bulk_activate_products" => {
            let cid = req_v(input, "category_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            let count = sqlx::query(
                "UPDATE products SET is_active = 1, updated_at = ? WHERE category_id = ? AND is_active = 0",
            )
            .bind(&now).bind(&cid).execute(pool).await?.rows_affected();
            audit2(pool, "bulk_activate", &cid, &format!("{{\"count\":{count}}}")).await;
            ok_mut(&format!("{count} product(s) activated in category {cid}."), "category", &cid)
        }

        "bulk_set_category" => {
            let from_cid = req_v(input, "from_category_id")?;
            let to_cid = req_v(input, "to_category_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            let count = sqlx::query(
                "UPDATE products SET category_id = ?, updated_at = ? WHERE category_id = ?",
            )
            .bind(&to_cid).bind(&now).bind(&from_cid)
            .execute(pool).await?.rows_affected();
            audit2(pool, "bulk_category_move", &from_cid, &format!("{{\"to\":\"{to_cid}\",\"count\":{count}}}")).await;
            ok_mut(&format!("{count} product(s) moved from {from_cid} to {to_cid}."), "category", &from_cid)
        }

        "bulk_set_tax_rule" => {
            let cid = req_v(input, "category_id")?;
            let trid = req_v(input, "tax_rule_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            let count = sqlx::query(
                "UPDATE products SET tax_rule_id = ?, updated_at = ? WHERE category_id = ?",
            )
            .bind(&trid).bind(&now).bind(&cid)
            .execute(pool).await?.rows_affected();
            audit2(pool, "bulk_tax_rule", &cid, &format!("{{\"tax_rule_id\":\"{trid}\",\"count\":{count}}}")).await;
            ok_mut(&format!("Tax rule {trid} applied to {count} product(s) in category {cid}."), "category", &cid)
        }

        "bulk_update_reorder_point" => {
            let cid = req_v(input, "category_id")?;
            let rp = input
                .get("reorder_point")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("reorder_point required".into()))?;
            let now = chrono::Utc::now().to_rfc3339();
            let count = sqlx::query(
                "UPDATE products SET reorder_point = ?, updated_at = ? WHERE category_id = ?",
            )
            .bind(rp).bind(&now).bind(&cid)
            .execute(pool).await?.rows_affected();
            audit2(pool, "bulk_reorder_point", &cid, &format!("{{\"reorder_point\":{rp},\"count\":{count}}}")).await;
            ok_mut(&format!("Reorder point set to {rp} for {count} product(s) in {cid}."), "category", &cid)
        }

        "bulk_update_cost" => {
            let updates = input
                .get("updates")
                .and_then(|v| v.as_array())
                .ok_or_else(|| AppError::Validation("updates array required".into()))?;
            if updates.is_empty() {
                return Err(AppError::Validation("updates array must not be empty".into()));
            }
            let now = chrono::Utc::now().to_rfc3339();
            let mut count = 0u64;
            for upd in updates {
                let pid = str_v(upd, "product_id");
                let cost = upd.get("cost_minor").and_then(|v| v.as_i64()).unwrap_or(0);
                if pid.is_empty() { continue; }
                sqlx::query("UPDATE products SET cost_minor = ?, updated_at = ? WHERE product_id = ?")
                    .bind(cost).bind(&now).bind(&pid).execute(pool).await?;
                count += 1;
            }
            ok_mut(&format!("{count} product cost(s) updated."), "product_cost_bulk", "batch")
        }

        "create_supplier" => {
            let name = req_v(input, "name")?;
            let id = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT INTO suppliers (supplier_id, name, phone, email, contact_name, address, notes, is_active, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?)",
            )
            .bind(&id)
            .bind(&name)
            .bind(str_v(input, "phone"))
            .bind(str_v(input, "email"))
            .bind(str_v(input, "contact_name"))
            .bind(str_v(input, "address"))
            .bind(str_v(input, "notes"))
            .bind(&now).bind(&now)
            .execute(pool).await?;
            audit2(pool, "supplier_created", &id, &format!("{{\"name\":\"{name}\"}}")).await;
            ok_mut(&format!("Supplier '{name}' created ({id})."), "supplier", &id)
        }

        "update_supplier" => {
            let id = req_v(input, "supplier_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            macro_rules! sup_set {
                ($field:expr, $col:expr) => {
                    if let Some(v) = input.get($field).and_then(|x| x.as_str()).filter(|s| !s.is_empty()) {
                        sqlx::query(&format!("UPDATE suppliers SET {} = ?, updated_at = ? WHERE supplier_id = ?", $col))
                            .bind(v).bind(&now).bind(&id).execute(pool).await?;
                    }
                };
            }
            sup_set!("name", "name");
            sup_set!("phone", "phone");
            sup_set!("email", "email");
            sup_set!("contact_name", "contact_name");
            sup_set!("address", "address");
            sup_set!("notes", "notes");
            if let Some(active) = input.get("is_active").and_then(|v| v.as_bool()) {
                sqlx::query("UPDATE suppliers SET is_active = ?, updated_at = ? WHERE supplier_id = ?")
                    .bind(if active { 1i64 } else { 0i64 }).bind(&now).bind(&id).execute(pool).await?;
            }
            audit2(pool, "supplier_updated", &id, "{}").await;
            ok_mut(&format!("Supplier {id} updated."), "supplier", &id)
        }

        "create_purchase_order" => {
            let lines = input
                .get("lines")
                .and_then(|v| v.as_array())
                .ok_or_else(|| AppError::Validation("lines array required".into()))?;
            if lines.is_empty() {
                return Err(AppError::Validation("PO must have at least one line".into()));
            }
            let po_id = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            let supplier_id = str_v(input, "supplier_id");
            let expected_date = str_v(input, "expected_date");
            let notes = str_v(input, "notes");
            sqlx::query(
                "INSERT INTO purchase_orders (po_id, supplier_id, status, expected_date, notes, created_by, created_at, updated_at)
                 VALUES (?, ?, 'draft', ?, ?, 'AI_ADMIN', ?, ?)",
            )
            .bind(&po_id)
            .bind(if supplier_id.is_empty() { None } else { Some(&supplier_id) })
            .bind(if expected_date.is_empty() { None } else { Some(&expected_date) })
            .bind(if notes.is_empty() { None } else { Some(&notes) })
            .bind(&now).bind(&now)
            .execute(pool).await?;
            for line in lines {
                let line_id = ulid::Ulid::new().to_string();
                let product_id = str_v(line, "product_id");
                let product_name = str_v(line, "product_name");
                let ordered_qty = line.get("ordered_qty").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let unit_cost = line.get("unit_cost_minor").and_then(|v| v.as_i64()).unwrap_or(0);
                let pname = if product_name.is_empty() && !product_id.is_empty() {
                    sqlx::query_scalar::<_, Option<String>>("SELECT name FROM products WHERE product_id = ?")
                        .bind(&product_id).fetch_optional(pool).await?.flatten().unwrap_or_default()
                } else { product_name };
                sqlx::query(
                    "INSERT INTO purchase_order_lines (po_line_id, po_id, product_id, product_name, ordered_qty, unit_cost_minor, created_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&line_id).bind(&po_id)
                .bind(if product_id.is_empty() { None } else { Some(&product_id) })
                .bind(&pname)
                .bind(ordered_qty).bind(unit_cost).bind(&now)
                .execute(pool).await?;
            }
            audit2(pool, "purchase_order_created", &po_id, &format!("{{\"lines\":{}}}", lines.len())).await;
            ok_mut(&format!("Purchase order {po_id} created ({} line(s)).", lines.len()), "purchase_order", &po_id)
        }

        "update_purchase_order" => {
            let po_id = req_v(input, "po_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            if let Some(status) = input.get("status").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                sqlx::query("UPDATE purchase_orders SET status = ?, updated_at = ? WHERE po_id = ?")
                    .bind(status).bind(&now).bind(&po_id).execute(pool).await?;
            }
            if let Some(rd) = input.get("received_date").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                sqlx::query("UPDATE purchase_orders SET received_date = ?, updated_at = ? WHERE po_id = ?")
                    .bind(rd).bind(&now).bind(&po_id).execute(pool).await?;
            }
            if let Some(notes) = input.get("notes").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                sqlx::query("UPDATE purchase_orders SET notes = ?, updated_at = ? WHERE po_id = ?")
                    .bind(notes).bind(&now).bind(&po_id).execute(pool).await?;
            }
            audit2(pool, "purchase_order_updated", &po_id, "{}").await;
            ok_mut(&format!("Purchase order {po_id} updated."), "purchase_order", &po_id)
        }

        "vacuum_database" => {
            sqlx::query("VACUUM").execute(pool).await?;
            ok_mut("Database VACUUM complete — disk space reclaimed.", "database", "global")
        }

        "create_customer_note" => {
            let cid = req_v(input, "customer_id")?;
            let note = req_v(input, "note")?;
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE customers SET notes = ?, updated_at = ? WHERE customer_id = ?")
                .bind(&note).bind(&now).bind(&cid).execute(pool).await?;
            audit2(pool, "customer_note_updated", &cid, "{}").await;
            ok_mut(&format!("Note saved for customer {cid}."), "customer", &cid)
        }

        other => {
            crate::ai::tools_write_ext3::execute(pool, other, input, currency_exp).await
        }
    }
}
