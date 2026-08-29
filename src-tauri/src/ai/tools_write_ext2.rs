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
    if let Err(error) = sqlx::query(
        "INSERT INTO audit_logs (audit_log_id, event_type, entity_type, entity_id,
         actor_user_id, actor_type, after_json, created_at, hash)
         VALUES (?, ?, 'ai_mutation', ?, ?, 'ai_agent', ?, ?, 'ai')",
    )
    .bind(&id)
    .bind(event)
    .bind(entity_id)
    .bind(crate::ai::tool_policy::current_actor_id().unwrap_or_else(|| "unknown".into()))
    .bind(after)
    .bind(&now)
    .execute(pool)
    .await
    {
        tracing::error!("AI audit write failed: {error}");
    }
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
            Ok(prev(
                tool_name,
                "Permanently delete a product",
                vec![
                    ("Product", name),
                    (
                        "⚠ Warning",
                        "Blocked if this product has any sales history".into(),
                    ),
                ],
            ))
        }
        "delete_category" => Ok(prev(
            tool_name,
            "Delete a category",
            vec![
                ("Category ID", req_v(input, "category_id")?),
                (
                    "⚠ Warning",
                    "Blocked if any products still belong to it".into(),
                ),
            ],
        )),
        "delete_tax_rule" => Ok(prev(
            tool_name,
            "Delete a tax rule",
            vec![
                ("Tax Rule ID", req_v(input, "tax_rule_id")?),
                ("⚠ Warning", "Blocked if any products reference it".into()),
            ],
        )),
        "delete_user" => Ok(prev(
            tool_name,
            "Delete a staff account",
            vec![
                ("User ID", req_v(input, "user_id")?),
                (
                    "⚠ Warning",
                    "Blocked if user has shifts or sales history".into(),
                ),
            ],
        )),
        "update_delivery_details" => Ok(prev(
            tool_name,
            "Update delivery details",
            vec![
                ("Delivery ID", req_v(input, "delivery_id")?),
                ("Rider", str_v(input, "delivery_staff_name")),
                ("Contact", str_v(input, "contact_number")),
            ],
        )),
        "reassign_delivery_rider" => Ok(prev(
            tool_name,
            "Reassign delivery rider",
            vec![
                ("Delivery ID", req_v(input, "delivery_id")?),
                ("New Rider", req_v(input, "rider_name")?),
            ],
        )),
        "batch_dispatch_deliveries" => {
            let ids = input
                .get("delivery_ids")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            Ok(prev(
                tool_name,
                "Dispatch multiple deliveries",
                vec![("Count", format!("{ids} delivery order(s)"))],
            ))
        }
        "duplicate_product" => {
            let id = req_v(input, "product_id")?;
            let name = product_name(pool, &id).await;
            Ok(prev(
                tool_name,
                "Clone a product",
                vec![("Source", name), ("New Name", req_v(input, "new_name")?)],
            ))
        }
        "send_whatsapp_to_customer" => Ok(prev(
            tool_name,
            "Send WhatsApp message",
            vec![
                ("Phone/Customer", str_v(input, "phone")),
                ("Message", req_v(input, "message")?),
            ],
        )),
        "reset_user_pin" => Ok(prev(
            tool_name,
            "Reset staff PIN",
            vec![
                ("User ID", req_v(input, "user_id")?),
                ("New PIN", "●●●●".into()),
            ],
        )),
        "lock_user" => Ok(prev(
            tool_name,
            "Lock staff account",
            vec![
                ("User ID", req_v(input, "user_id")?),
                ("Effect", "Account disabled — cannot log in".into()),
            ],
        )),
        "unlock_user" => Ok(prev(
            tool_name,
            "Unlock staff account",
            vec![
                ("User ID", req_v(input, "user_id")?),
                ("Effect", "Account re-enabled".into()),
            ],
        )),
        "bulk_deactivate_products" => Ok(prev(
            tool_name,
            "Deactivate all products in category",
            vec![("Category ID", req_v(input, "category_id")?)],
        )),
        "bulk_activate_products" => Ok(prev(
            tool_name,
            "Activate all products in category",
            vec![("Category ID", req_v(input, "category_id")?)],
        )),
        "bulk_set_category" => Ok(prev(
            tool_name,
            "Move products to new category",
            vec![
                ("From", req_v(input, "from_category_id")?),
                ("To", req_v(input, "to_category_id")?),
            ],
        )),
        "bulk_set_tax_rule" => Ok(prev(
            tool_name,
            "Apply tax rule to category",
            vec![
                ("Category ID", req_v(input, "category_id")?),
                ("Tax Rule ID", req_v(input, "tax_rule_id")?),
            ],
        )),
        "bulk_update_reorder_point" => Ok(prev(
            tool_name,
            "Set reorder point for category",
            vec![
                ("Category ID", req_v(input, "category_id")?),
                (
                    "Reorder Point",
                    format!(
                        "{}",
                        input
                            .get("reorder_point")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0)
                    ),
                ),
            ],
        )),
        "bulk_update_cost" => {
            let count = input
                .get("updates")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            Ok(prev(
                tool_name,
                "Update cost price for multiple products",
                vec![("Products", format!("{count} product(s)"))],
            ))
        }
        "create_supplier" => Ok(prev(
            tool_name,
            "Add a new supplier",
            vec![
                ("Name", req_v(input, "name")?),
                ("Phone", str_v(input, "phone")),
            ],
        )),
        "update_supplier" => Ok(prev(
            tool_name,
            "Update supplier details",
            vec![
                ("Supplier ID", req_v(input, "supplier_id")?),
                ("Name", str_v(input, "name")),
            ],
        )),
        "create_purchase_order" => {
            let lines = input
                .get("lines")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            Ok(prev(
                tool_name,
                "Create purchase order",
                vec![
                    ("Supplier", str_v(input, "supplier_id")),
                    ("Lines", format!("{lines} item(s)")),
                ],
            ))
        }
        "update_purchase_order" => Ok(prev(
            tool_name,
            "Update purchase order status",
            vec![
                ("PO ID", req_v(input, "po_id")?),
                ("Status", str_v(input, "status")),
            ],
        )),
        "vacuum_database" => Ok(prev(
            tool_name,
            "VACUUM SQLite database",
            vec![
                (
                    "Effect",
                    "Reclaims disk space and defragments the DB file".into(),
                ),
                (
                    "Duration",
                    "May take a few seconds. Read-only during VACUUM.".into(),
                ),
            ],
        )),
        "reindex_database" => Ok(prev(
            tool_name,
            "Rebuild all database indexes",
            vec![
                (
                    "Effect",
                    "Rebuilds fragmented indexes to restore query performance".into(),
                ),
                (
                    "Scope",
                    "All user tables (products, sales, inventory, etc.)".into(),
                ),
            ],
        )),
        "force_wal_checkpoint" => Ok(prev(
            tool_name,
            "Truncate write-ahead log (WAL)",
            vec![
                (
                    "Effect",
                    "Shrinks oversized WAL file to zero, ensuring crash consistency".into(),
                ),
                ("When", "WAL file > 50 MB or before backup".into()),
            ],
        )),
        "resolve_ghost_barcode" => Ok(prev(
            tool_name,
            "Link an unknown barcode to a product",
            vec![
                ("Barcode ID", req_v(input, "barcode_id")?),
                ("Product ID", req_v(input, "product_id")?),
            ],
        )),
        "resolve_sync_conflict" => Ok(prev(
            tool_name,
            "Resolve a multi-terminal data conflict",
            vec![
                ("Conflict ID", req_v(input, "conflict_id")?),
                ("Resolution", req_v(input, "action")?),
            ],
        )),
        "clear_ghost_sync_records" => Ok(prev(
            tool_name,
            "Clear orphan sync records",
            vec![
                (
                    "Effect",
                    "Mark pending sync records as synced when their parent entity no longer exists".into(),
                ),
                ("Scope", "All sync tables".into()),
            ],
        )),
        "run_diagnostics_and_fix" => Ok(prev(
            tool_name,
            "Run system diagnostics and fix common issues",
            vec![
                ("Checks", "DB integrity, stuck AI runs, stuck AI actions".into()),
                ("Auto-fixes", "Clears stuck runs (>5min) and actions (>10min)".into()),
            ],
        )),
        "bulk_import_products" => Ok(prev(
            tool_name,
            "Import products from CSV data",
            vec![
                ("Format", "base64-encoded CSV with columns: name,sku,barcode,category,tax_rule,price,cost,reorder_point".into()),
                ("Size hint", "Keep under 500 products per import".into()),
            ],
        )),
        "bulk_import_categories" => Ok(prev(
            tool_name,
            "Import categories from CSV data",
            vec![
                ("Format", "base64-encoded CSV with column: name".into()),
                ("Size hint", "One category name per line or row".into()),
            ],
        )),
        "create_products" => {
            let products = input
                .get("products")
                .and_then(|v| v.as_array())
                .ok_or_else(|| AppError::Validation("Missing products array".into()))?;
            if products.is_empty() {
                return Err(AppError::Validation("products array is empty".into()));
            }
            if products.len() > 500 {
                return Err(AppError::Validation(
                    "Max 500 products per create_products call".into(),
                ));
            }
            let mut fields: Vec<(&str, String)> =
                vec![("Products", format!("{} new product(s)", products.len()))];
            // Show the first rows so the admin confirms real data, not a count.
            for (i, p) in products.iter().take(8).enumerate() {
                let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                let price = p.get("price_minor").and_then(|v| v.as_i64()).unwrap_or(0);
                fields.push((
                    if i == 0 { "Items" } else { "" },
                    format!(
                        "{name} — {} | barcode: {}",
                        crate::domain::money::format_minor(price, currency_exp),
                        p.get("barcode").and_then(|v| v.as_str()).unwrap_or("—")
                    ),
                ));
            }
            if products.len() > 8 {
                fields.push(("", format!("…and {} more", products.len() - 8)));
            }
            Ok(prev(tool_name, "Create multiple products", fields))
        }
        "send_receipt_via_whatsapp" => Ok(prev(
            tool_name,
            "Send a sale receipt PDF via WhatsApp",
            vec![
                ("Receipt", req_v(input, "receipt_number")?),
                ("To phone", req_v(input, "phone")?),
            ],
        )),
        "create_customer_note" => Ok(prev(
            tool_name,
            "Attach note to customer",
            vec![
                ("Customer ID", req_v(input, "customer_id")?),
                ("Note", req_v(input, "note")?),
            ],
        )),
        other => crate::ai::tools_write_ext3::dry_run(pool, other, input, currency_exp).await,
    }
}

// ── Mutation executors ────────────────────────────────────────────────────────

pub async fn execute(
    pool: &SqlitePool,
    tool_name: &str,
    input: &serde_json::Value,
    currency_exp: u32,
) -> AppResult<MutationResult> {
    let actor_id = crate::ai::tool_policy::current_actor_id()
        .ok_or_else(|| AppError::Permission("Mutation actor context is missing".into()))?;
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
            // Tombstoned, not removed: the product's barcodes have to stop
            // resolving on every other till too, and only a tombstone travels.
            crate::db::repositories::product_repo::soft_delete_barcodes_for_product(pool, &id)
                .await?;
            sqlx::query("DELETE FROM products WHERE product_id = ?")
                .bind(&id)
                .execute(pool)
                .await?;
            audit2(
                pool,
                "product_deleted",
                &id,
                &format!("{{\"name\":\"{name}\"}}"),
            )
            .await;
            ok_mut(&format!("Product '{name}' deleted."), "product", &id)
        }

        "delete_category" => {
            let id = req_v(input, "category_id")?;
            let product_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE category_id = ?")
                    .bind(&id)
                    .fetch_one(pool)
                    .await?;
            if product_count > 0 {
                return Err(AppError::Validation(format!(
                    "Category has {product_count} product(s). Reassign them before deleting."
                )));
            }
            sqlx::query("DELETE FROM categories WHERE category_id = ?")
                .bind(&id)
                .execute(pool)
                .await?;
            audit2(pool, "category_deleted", &id, "{}").await;
            ok_mut(&format!("Category {id} deleted."), "category", &id)
        }

        "delete_tax_rule" => {
            let id = req_v(input, "tax_rule_id")?;
            let in_use: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE tax_rule_id = ?")
                    .bind(&id)
                    .fetch_one(pool)
                    .await?;
            if in_use > 0 {
                return Err(AppError::Validation(format!(
                    "Tax rule is assigned to {in_use} product(s). Reassign them first."
                )));
            }
            sqlx::query("DELETE FROM tax_rules WHERE tax_rule_id = ?")
                .bind(&id)
                .execute(pool)
                .await?;
            audit2(pool, "tax_rule_deleted", &id, "{}").await;
            ok_mut(&format!("Tax rule {id} deleted."), "tax_rule", &id)
        }

        "delete_user" => {
            let id = req_v(input, "user_id")?;
            let has_shifts: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM shifts WHERE cashier_user_id = ?")
                    .bind(&id)
                    .fetch_one(pool)
                    .await?;
            let has_sales: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM sales WHERE cashier_user_id = ?")
                    .bind(&id)
                    .fetch_one(pool)
                    .await?;
            if has_shifts > 0 || has_sales > 0 {
                return Err(AppError::Validation(
                    "Cannot delete user with shift or sales history. Set is_active=false instead."
                        .into(),
                ));
            }
            sqlx::query("DELETE FROM users WHERE user_id = ?")
                .bind(&id)
                .execute(pool)
                .await?;
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
            audit2(
                pool,
                "delivery_updated",
                &id,
                &format!("{{\"changes\":\"{}\"}}", parts.join(", ")),
            )
            .await;
            ok_mut(
                &format!("Delivery {id} updated: {}.", parts.join(", ")),
                "delivery",
                &id,
            )
        }

        "reassign_delivery_rider" => {
            let id = req_v(input, "delivery_id")?;
            let rider = req_v(input, "rider_name")?;
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE delivery_orders SET delivery_staff_name = ?, updated_at = ? WHERE delivery_id = ?")
                .bind(&rider).bind(&now).bind(&id).execute(pool).await?;
            audit2(
                pool,
                "delivery_rider_reassigned",
                &id,
                &format!("{{\"rider\":\"{rider}\"}}"),
            )
            .await;
            ok_mut(
                &format!("Delivery {id} reassigned to {rider}."),
                "delivery",
                &id,
            )
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
                return Err(AppError::Validation(
                    "delivery_ids must not be empty".into(),
                ));
            }
            let now = chrono::Utc::now().to_rfc3339();
            let mut dispatched = 0usize;
            for id in &ids {
                let rows = sqlx::query(
                    "UPDATE delivery_orders SET delivery_status = 'dispatched', updated_at = ? WHERE delivery_id = ? AND delivery_status = 'pending'",
                )
                .bind(&now).bind(id).execute(pool).await?.rows_affected();
                if rows > 0 {
                    dispatched += 1;
                }
            }
            ok_mut(
                &format!("{dispatched} delivery/deliveries dispatched."),
                "delivery_batch",
                "batch",
            )
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
            .bind(
                row.try_get::<Option<i64>, _>("price_minor")
                    .ok()
                    .flatten()
                    .unwrap_or(0),
            )
            .bind(row.try_get::<Option<i64>, _>("cost_minor").ok().flatten())
            .bind(
                row.try_get::<Option<String>, _>("category_id")
                    .ok()
                    .flatten(),
            )
            .bind(
                row.try_get::<Option<String>, _>("tax_rule_id")
                    .ok()
                    .flatten(),
            )
            .bind(
                row.try_get::<Option<i64>, _>("track_stock")
                    .ok()
                    .flatten()
                    .unwrap_or(0),
            )
            .bind(
                row.try_get::<Option<f64>, _>("reorder_point")
                    .ok()
                    .flatten(),
            )
            .bind(1i64)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await?;
            audit2(
                pool,
                "product_duplicated",
                &new_id,
                &format!("{{\"source\":\"{src_id}\",\"name\":\"{new_name}\"}}"),
            )
            .await;
            ok_mut(
                &format!("Product '{new_name}' created as clone of {src_id}."),
                "product",
                &new_id,
            )
        }

        "send_whatsapp_to_customer" => {
            let message = req_v(input, "message")?;
            let phone = if let Some(p) = input
                .get("phone")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                p.to_string()
            } else {
                let cid = req_v(input, "customer_id")?;
                sqlx::query_scalar::<_, Option<String>>(
                    "SELECT phone FROM customers WHERE customer_id = ?",
                )
                .bind(&cid)
                .fetch_optional(pool)
                .await?
                .flatten()
                .ok_or_else(|| {
                    AppError::Validation("Customer has no phone number on file.".into())
                })?
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
                return Err(AppError::Internal(format!(
                    "Sidecar error: {}",
                    resp.status()
                )));
            }
            ok_mut(
                &format!("WhatsApp message sent to {phone}."),
                "whatsapp",
                &phone,
            )
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
                .bind(&hash)
                .bind(&now)
                .bind(&uid)
                .execute(pool)
                .await?;
            audit2(pool, "user_pin_reset", &uid, "{}").await;
            ok_mut(&format!("PIN reset for user {uid}."), "user", &uid)
        }

        "lock_user" => {
            let uid = req_v(input, "user_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE users SET is_active = 0, updated_at = ? WHERE user_id = ?")
                .bind(&now)
                .bind(&uid)
                .execute(pool)
                .await?;
            audit2(pool, "user_locked", &uid, "{}").await;
            ok_mut(&format!("User {uid} locked."), "user", &uid)
        }

        "unlock_user" => {
            let uid = req_v(input, "user_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE users SET is_active = 1, updated_at = ? WHERE user_id = ?")
                .bind(&now)
                .bind(&uid)
                .execute(pool)
                .await?;
            audit2(pool, "user_unlocked", &uid, "{}").await;
            ok_mut(&format!("User {uid} unlocked."), "user", &uid)
        }

        "bulk_deactivate_products" => {
            let cid = req_v(input, "category_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            let count = sqlx::query(
                "UPDATE products SET is_active = 0, updated_at = ?, sync_status = 'pending' WHERE category_id = ? AND is_active = 1",
            )
            .bind(&now).bind(&cid).execute(pool).await?.rows_affected();
            audit2(
                pool,
                "bulk_deactivate",
                &cid,
                &format!("{{\"count\":{count}}}"),
            )
            .await;
            ok_mut(
                &format!("{count} product(s) deactivated in category {cid}."),
                "category",
                &cid,
            )
        }

        "bulk_activate_products" => {
            let cid = req_v(input, "category_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            let count = sqlx::query(
                "UPDATE products SET is_active = 1, updated_at = ?, sync_status = 'pending' WHERE category_id = ? AND is_active = 0",
            )
            .bind(&now).bind(&cid).execute(pool).await?.rows_affected();
            audit2(
                pool,
                "bulk_activate",
                &cid,
                &format!("{{\"count\":{count}}}"),
            )
            .await;
            ok_mut(
                &format!("{count} product(s) activated in category {cid}."),
                "category",
                &cid,
            )
        }

        "bulk_set_category" => {
            let from_cid = req_v(input, "from_category_id")?;
            let to_cid = req_v(input, "to_category_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            let count = sqlx::query(
                "UPDATE products SET category_id = ?, updated_at = ?, sync_status = 'pending' WHERE category_id = ?",
            )
            .bind(&to_cid)
            .bind(&now)
            .bind(&from_cid)
            .execute(pool)
            .await?
            .rows_affected();
            audit2(
                pool,
                "bulk_category_move",
                &from_cid,
                &format!("{{\"to\":\"{to_cid}\",\"count\":{count}}}"),
            )
            .await;
            ok_mut(
                &format!("{count} product(s) moved from {from_cid} to {to_cid}."),
                "category",
                &from_cid,
            )
        }

        "bulk_set_tax_rule" => {
            let cid = req_v(input, "category_id")?;
            let trid = req_v(input, "tax_rule_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            let count = sqlx::query(
                "UPDATE products SET tax_rule_id = ?, updated_at = ?, sync_status = 'pending' WHERE category_id = ?",
            )
            .bind(&trid)
            .bind(&now)
            .bind(&cid)
            .execute(pool)
            .await?
            .rows_affected();
            audit2(
                pool,
                "bulk_tax_rule",
                &cid,
                &format!("{{\"tax_rule_id\":\"{trid}\",\"count\":{count}}}"),
            )
            .await;
            ok_mut(
                &format!("Tax rule {trid} applied to {count} product(s) in category {cid}."),
                "category",
                &cid,
            )
        }

        "bulk_update_reorder_point" => {
            let cid = req_v(input, "category_id")?;
            let rp = input
                .get("reorder_point")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("reorder_point required".into()))?;
            let now = chrono::Utc::now().to_rfc3339();
            let count = sqlx::query(
                "UPDATE products SET reorder_point = ?, updated_at = ?, sync_status = 'pending' WHERE category_id = ?",
            )
            .bind(rp)
            .bind(&now)
            .bind(&cid)
            .execute(pool)
            .await?
            .rows_affected();
            audit2(
                pool,
                "bulk_reorder_point",
                &cid,
                &format!("{{\"reorder_point\":{rp},\"count\":{count}}}"),
            )
            .await;
            ok_mut(
                &format!("Reorder point set to {rp} for {count} product(s) in {cid}."),
                "category",
                &cid,
            )
        }

        "bulk_update_cost" => {
            let updates = input
                .get("updates")
                .and_then(|v| v.as_array())
                .ok_or_else(|| AppError::Validation("updates array required".into()))?;
            if updates.is_empty() {
                return Err(AppError::Validation(
                    "updates array must not be empty".into(),
                ));
            }
            let now = chrono::Utc::now().to_rfc3339();
            let mut count = 0u64;
            for upd in updates {
                let pid = str_v(upd, "product_id");
                let cost = upd.get("cost_minor").and_then(|v| v.as_i64()).unwrap_or(0);
                if pid.is_empty() {
                    continue;
                }
                sqlx::query(
                    "UPDATE products SET cost_minor = ?, updated_at = ?, sync_status = 'pending' WHERE product_id = ?",
                )
                .bind(cost)
                .bind(&now)
                .bind(&pid)
                .execute(pool)
                .await?;
                count += 1;
            }
            ok_mut(
                &format!("{count} product cost(s) updated."),
                "product_cost_bulk",
                "batch",
            )
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
            audit2(
                pool,
                "supplier_created",
                &id,
                &format!("{{\"name\":\"{name}\"}}"),
            )
            .await;
            ok_mut(
                &format!("Supplier '{name}' created ({id})."),
                "supplier",
                &id,
            )
        }

        "update_supplier" => {
            let id = req_v(input, "supplier_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            macro_rules! sup_set {
                ($field:expr, $col:expr) => {
                    if let Some(v) = input
                        .get($field)
                        .and_then(|x| x.as_str())
                        .filter(|s| !s.is_empty())
                    {
                        sqlx::query(&format!(
                            "UPDATE suppliers SET {} = ?, updated_at = ? WHERE supplier_id = ?",
                            $col
                        ))
                        .bind(v)
                        .bind(&now)
                        .bind(&id)
                        .execute(pool)
                        .await?;
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
                sqlx::query(
                    "UPDATE suppliers SET is_active = ?, updated_at = ? WHERE supplier_id = ?",
                )
                .bind(if active { 1i64 } else { 0i64 })
                .bind(&now)
                .bind(&id)
                .execute(pool)
                .await?;
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
                return Err(AppError::Validation(
                    "PO must have at least one line".into(),
                ));
            }
            let po_id = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            let supplier_id = str_v(input, "supplier_id");
            let expected_date = str_v(input, "expected_date");
            let notes = str_v(input, "notes");
            sqlx::query(
                "INSERT INTO purchase_orders (po_id, supplier_id, status, expected_date, notes, created_by, created_at, updated_at)
                 VALUES (?, ?, 'draft', ?, ?, ?, ?, ?)",
            )
            .bind(&po_id)
            .bind(if supplier_id.is_empty() { None } else { Some(&supplier_id) })
            .bind(if expected_date.is_empty() { None } else { Some(&expected_date) })
            .bind(if notes.is_empty() { None } else { Some(&notes) })
            .bind(&actor_id)
            .bind(&now).bind(&now)
            .execute(pool).await?;
            for line in lines {
                let line_id = ulid::Ulid::new().to_string();
                let product_id = str_v(line, "product_id");
                let product_name = str_v(line, "product_name");
                let ordered_qty = line
                    .get("ordered_qty")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let unit_cost = line
                    .get("unit_cost_minor")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let pname = if product_name.is_empty() && !product_id.is_empty() {
                    sqlx::query_scalar::<_, Option<String>>(
                        "SELECT name FROM products WHERE product_id = ?",
                    )
                    .bind(&product_id)
                    .fetch_optional(pool)
                    .await?
                    .flatten()
                    .unwrap_or_default()
                } else {
                    product_name
                };
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
            audit2(
                pool,
                "purchase_order_created",
                &po_id,
                &format!("{{\"lines\":{}}}", lines.len()),
            )
            .await;
            ok_mut(
                &format!("Purchase order {po_id} created ({} line(s)).", lines.len()),
                "purchase_order",
                &po_id,
            )
        }

        "update_purchase_order" => {
            let po_id = req_v(input, "po_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            if let Some(status) = input
                .get("status")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                sqlx::query(
                    "UPDATE purchase_orders SET status = ?, updated_at = ? WHERE po_id = ?",
                )
                .bind(status)
                .bind(&now)
                .bind(&po_id)
                .execute(pool)
                .await?;
            }
            if let Some(rd) = input
                .get("received_date")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                sqlx::query(
                    "UPDATE purchase_orders SET received_date = ?, updated_at = ? WHERE po_id = ?",
                )
                .bind(rd)
                .bind(&now)
                .bind(&po_id)
                .execute(pool)
                .await?;
            }
            if let Some(notes) = input
                .get("notes")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                sqlx::query("UPDATE purchase_orders SET notes = ?, updated_at = ? WHERE po_id = ?")
                    .bind(notes)
                    .bind(&now)
                    .bind(&po_id)
                    .execute(pool)
                    .await?;
            }
            audit2(pool, "purchase_order_updated", &po_id, "{}").await;
            ok_mut(
                &format!("Purchase order {po_id} updated."),
                "purchase_order",
                &po_id,
            )
        }

        "vacuum_database" => {
            sqlx::query("VACUUM").execute(pool).await?;
            ok_mut(
                "Database VACUUM complete — disk space reclaimed.",
                "database",
                "global",
            )
        }

        "reindex_database" => {
            let tables = [
                "products",
                "categories",
                "sales",
                "sale_items",
                "customers",
                "users",
                "delivery_orders",
                "shifts",
                "tax_rules",
                "suppliers",
                "purchase_orders",
                "audit_logs",
                "refunds",
                "held_carts",
                "cash_events",
                "stock_levels",
                "stock_movements",
            ];
            for t in &tables {
                sqlx::query(&format!("REINDEX \"{t}\""))
                    .execute(pool)
                    .await?;
            }
            ok_mut(
                &format!("REINDEX complete — {} indexes rebuilt.", tables.len()),
                "database",
                "global",
            )
        }

        "force_wal_checkpoint" => {
            sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
                .execute(pool)
                .await?;
            ok_mut(
                "WAL checkpoint complete — write-ahead log truncated.",
                "database",
                "global",
            )
        }

        "resolve_ghost_barcode" => {
            let id = req_v(input, "barcode_id")?;
            let product_id = req_v(input, "product_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "UPDATE unknown_barcodes SET status = 'found', product_id = ? WHERE id = ?",
            )
            .bind(&product_id)
            .bind(&id)
            .execute(pool)
            .await?;
            let barcode: String =
                sqlx::query_scalar("SELECT barcode FROM unknown_barcodes WHERE id = ?")
                    .bind(&id)
                    .fetch_one(pool)
                    .await?;
            // See tools_write_ext: an empty updated_at makes the row unservable.
            sqlx::query(
                "INSERT INTO product_barcodes
                   (barcode_id, product_id, barcode, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
                .bind(ulid::Ulid::new().to_string())
                .bind(&product_id)
                .bind(&barcode)
                .bind(&now)
            .bind(&now)
            .execute(pool).await?;
            audit2(
                pool,
                "ghost_barcode_resolved",
                &id,
                &format!("{{\"product_id\":\"{product_id}\"}}"),
            )
            .await;
            ok_mut(
                &format!("Ghost barcode {id} resolved to product {product_id}."),
                "ghost_barcode",
                &id,
            )
        }

        "resolve_sync_conflict" => {
            let conflict_id = req_v(input, "conflict_id")?;
            let action = req_v(input, "action")?;
            let resolved_at = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE sync_conflicts SET status = 'resolved', resolved_at = ? WHERE conflict_id = ?")
                .bind(&resolved_at)
                .bind(&conflict_id)
                .execute(pool).await?;
            audit2(
                pool,
                "sync_conflict_resolved",
                &conflict_id,
                &format!("{{\"action\":\"{action}\"}}"),
            )
            .await;
            ok_mut(
                &format!("Sync conflict {conflict_id} resolved ({action})."),
                "sync_conflict",
                &conflict_id,
            )
        }

        "create_customer_note" => {
            let cid = req_v(input, "customer_id")?;
            let note = req_v(input, "note")?;
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE customers SET notes = ?, updated_at = ? WHERE customer_id = ?")
                .bind(&note)
                .bind(&now)
                .bind(&cid)
                .execute(pool)
                .await?;
            audit2(pool, "customer_note_updated", &cid, "{}").await;
            ok_mut(&format!("Note saved for customer {cid}."), "customer", &cid)
        }

        "clear_ghost_sync_records" => {
            let mut cleared = 0u64;
            for table in &[
                "products",
                "categories",
                "sales",
                "sale_items",
                "customers",
                "users",
                "branches",
                "devices",
                "tax_rules",
                "suppliers",
                "purchase_orders",
                "purchase_order_lines",
                "refunds",
                "refund_items",
                "cash_events",
                "shifts",
                "delivery_orders",
                "audit_logs",
                "stock_movements",
                "stock_levels",
                "product_barcodes",
                "product_prices",
            ] {
                let count = sqlx::query(&format!(
                    "UPDATE \"{table}\" SET sync_attempts = 0, sync_status = 'synced'
                     WHERE sync_status = 'pending' AND sync_attempts >= 10"
                ))
                .execute(pool)
                .await?
                .rows_affected();
                if count > 0 {
                    tracing::info!("clear_ghost_sync_records: reset {count} stuck rows in {table}");
                }
                cleared += count;
            }
            ok_mut(
                &format!("Reset {cleared} stuck sync record(s) across all tables."),
                "sync",
                "global",
            )
        }

        "run_diagnostics_and_fix" => {
            let mut report = Vec::new();

            let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
                .fetch_one(pool)
                .await?;
            report.push(format!("Integrity check: {integrity}"));

            let stale_runs = sqlx::query(
                "UPDATE ai_runs SET status = 'failed', error = 'cleared by diagnostics (stale)',
                 updated_at = datetime('now') WHERE status = 'running' AND updated_at < datetime('now', '-5 minutes')",
            )
            .execute(pool)
            .await?
            .rows_affected();
            if stale_runs > 0 {
                report.push(format!("Cleared {stale_runs} stale AI run(s)"));
            }

            let stale_actions = sqlx::query(
                "UPDATE ai_actions SET status = 'failed', error_message = 'cleared by diagnostics (stale)',
                 executed_at = datetime('now') WHERE status = 'executing' AND prepared_at < datetime('now', '-10 minutes')",
            )
            .execute(pool)
            .await?
            .rows_affected();
            if stale_actions > 0 {
                report.push(format!("Cleared {stale_actions} stale AI action(s)"));
            }

            if stale_runs == 0 && stale_actions == 0 {
                report.push("No repairs needed.".into());
            }

            ok_mut(&report.join("\n"), "diagnostics", "global")
        }

        "bulk_import_products" => {
            let csv_b64 = req_v(input, "csv_base64")?;
            use base64::Engine as _;
            let csv_bytes = base64::engine::general_purpose::STANDARD
                .decode(csv_b64.as_bytes())
                .map_err(|e| AppError::Validation(format!("Invalid base64: {e}")))?;
            let csv_str = String::from_utf8(csv_bytes)
                .map_err(|e| AppError::Validation(format!("Invalid UTF-8: {e}")))?;

            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .flexible(true)
                .from_reader(csv_str.as_bytes());
            let now = chrono::Utc::now().to_rfc3339();
            let mut count = 0u64;
            // Barcode integrity: the importer must not silently create duplicate
            // barcodes — neither within the file nor against the database.
            let all_rows: Vec<csv::StringRecord> = reader
                .records()
                .collect::<Result<_, _>>()
                .map_err(|e| AppError::Validation(format!("CSV parse error: {e}")))?;
            let total_rows = all_rows.len() as u64;
            let mut seen_barcodes: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            let mut skipped: Vec<String> = Vec::new();
            let mut row_num = 0u64;
            for row in all_rows {
                row_num += 1;
                crate::app_events::emit_bulk_progress("bulk_import_products", row_num, total_rows);
                if row.is_empty() {
                    continue;
                }
                let name = row.get(0).unwrap_or("").trim().to_string();
                if name.is_empty() {
                    continue;
                }
                let sku = row.get(1).unwrap_or("").trim().to_string();
                let barcode = row.get(2).unwrap_or("").trim().to_string();
                let cat_name = row.get(3).unwrap_or("").trim().to_string();
                let tax_name = row.get(4).unwrap_or("").trim().to_string();
                let price_str = row.get(5).unwrap_or("").trim().to_string();
                let cost_str = row.get(6).unwrap_or("").trim().to_string();
                let rp_str = row.get(7).unwrap_or("0").trim().to_string();

                if !barcode.is_empty() {
                    if !seen_barcodes.insert(barcode.clone()) {
                        skipped.push(format!(
                            "row {row_num} '{name}': barcode {barcode} duplicated within the file"
                        ));
                        continue;
                    }
                    let exists: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM (
                             SELECT product_id FROM products WHERE barcode = ?
                             UNION ALL
                             SELECT product_id FROM product_barcodes WHERE barcode = ? AND deleted_at IS NULL
                         )",
                    )
                    .bind(&barcode)
                    .bind(&barcode)
                    .fetch_one(pool)
                    .await?;
                    if exists > 0 {
                        skipped.push(format!(
                            "row {row_num} '{name}': barcode {barcode} already exists in the database"
                        ));
                        continue;
                    }
                }

                let cat_id: Option<String> = if !cat_name.is_empty() {
                    sqlx::query_scalar(
                        "SELECT category_id FROM categories WHERE name = ? AND is_active = 1",
                    )
                    .bind(&cat_name)
                    .fetch_optional(pool)
                    .await?
                    .flatten()
                } else {
                    None
                };
                let tax_id: Option<String> = if !tax_name.is_empty() {
                    sqlx::query_scalar(
                        "SELECT tax_rule_id FROM tax_rules WHERE name = ? AND is_active = 1",
                    )
                    .bind(&tax_name)
                    .fetch_optional(pool)
                    .await?
                    .flatten()
                } else {
                    None
                };
                let price: i64 = price_str.parse().unwrap_or(0);
                let cost: i64 = cost_str.parse().unwrap_or(0);
                let rp: f64 = rp_str.parse().unwrap_or(0.0);

                let pid = ulid::Ulid::new().to_string();
                sqlx::query(
                    "INSERT INTO products (product_id, name, sku, barcode, category_id, tax_rule_id, cost_minor, reorder_point, is_active, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)",
                )
                .bind(&pid).bind(&name)
                .bind(if sku.is_empty() { None } else { Some(&sku) })
                .bind(if barcode.is_empty() { None } else { Some(&barcode) })
                .bind(cat_id.as_deref())
                .bind(tax_id.as_deref())
                .bind(cost).bind(rp)
                .bind(&now).bind(&now)
                .execute(pool).await?;
                if price > 0 {
                    sqlx::query(
                        "INSERT INTO product_prices (price_id, product_id, price_type, price_minor, effective_from, created_at, created_by_user_id)
                         VALUES (?, ?, 'selling', ?, ?, ?, 'zanai-bulk-import')",
                    )
                    .bind(ulid::Ulid::new().to_string()).bind(&pid).bind(price).bind(&now).bind(&now)
                    .execute(pool).await?;
                }
                count += 1;
            }
            let summary = if skipped.is_empty() {
                format!("Imported {count} product(s).")
            } else {
                let shown: Vec<&String> = skipped.iter().take(15).collect();
                let more = skipped.len().saturating_sub(shown.len());
                let mut msg = format!(
                    "Imported {count} product(s). Skipped {} row(s) for barcode conflicts:\n- {}",
                    skipped.len(),
                    shown
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join("\n- ")
                );
                if more > 0 {
                    msg.push_str(&format!("\n…and {more} more."));
                }
                msg
            };
            ok_mut(&summary, "product_bulk_import", "batch")
        }

        "create_products" => {
            let products = input
                .get("products")
                .and_then(|v| v.as_array())
                .ok_or_else(|| AppError::Validation("Missing products array".into()))?;
            if products.is_empty() {
                return Err(AppError::Validation("products array is empty".into()));
            }
            if products.len() > 500 {
                return Err(AppError::Validation(
                    "Max 500 products per create_products call".into(),
                ));
            }
            let now = chrono::Utc::now().to_rfc3339();
            let total = products.len() as u64;
            let mut count = 0u64;
            let mut seen_barcodes: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            let mut skipped: Vec<String> = Vec::new();
            for (idx, p) in products.iter().enumerate() {
                crate::app_events::emit_bulk_progress("create_products", idx as u64 + 1, total);
                let row_num = idx + 1;
                let name = p
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .unwrap_or("");
                if name.is_empty() {
                    skipped.push(format!("row {row_num}: missing name"));
                    continue;
                }
                let price = p.get("price_minor").and_then(|v| v.as_i64()).unwrap_or(0);
                let cost = p.get("cost_minor").and_then(|v| v.as_i64()).unwrap_or(0);
                if price < 0 || cost < 0 {
                    skipped.push(format!("row {row_num} '{name}': negative price/cost"));
                    continue;
                }
                let sku = p
                    .get("sku")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .unwrap_or("")
                    .to_string();
                let barcode = p
                    .get("barcode")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .unwrap_or("")
                    .to_string();

                // Barcode integrity — same rules as bulk_import_products.
                if !barcode.is_empty() {
                    if !seen_barcodes.insert(barcode.clone()) {
                        skipped.push(format!(
                            "row {row_num} '{name}': barcode {barcode} duplicated in this batch"
                        ));
                        continue;
                    }
                    let exists: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM (
                             SELECT product_id FROM products WHERE barcode = ?
                             UNION ALL
                             SELECT product_id FROM product_barcodes WHERE barcode = ? AND deleted_at IS NULL
                         )",
                    )
                    .bind(&barcode)
                    .bind(&barcode)
                    .fetch_one(pool)
                    .await?;
                    if exists > 0 {
                        skipped.push(format!(
                            "row {row_num} '{name}': barcode {barcode} already exists in the database"
                        ));
                        continue;
                    }
                }

                // Resolve category by id OR exact name.
                let cat_id: Option<String> = if let Some(cid) =
                    p.get("category_id").and_then(|v| v.as_str())
                {
                    sqlx::query_scalar(
                        "SELECT category_id FROM categories WHERE category_id = ? AND is_active = 1",
                    )
                    .bind(cid)
                    .fetch_optional(pool)
                    .await?
                    .flatten()
                } else if let Some(cname) = p.get("category_name").and_then(|v| v.as_str()) {
                    sqlx::query_scalar(
                        "SELECT category_id FROM categories WHERE name = ? AND is_active = 1",
                    )
                    .bind(cname.trim())
                    .fetch_optional(pool)
                    .await?
                    .flatten()
                } else {
                    None
                };
                // Resolve tax rule by id OR exact name.
                let tax_id: Option<String> =
                    if let Some(tid) = p.get("tax_rule_id").and_then(|v| v.as_str()) {
                        sqlx::query_scalar(
                        "SELECT tax_rule_id FROM tax_rules WHERE tax_rule_id = ? AND is_active = 1",
                    )
                    .bind(tid)
                    .fetch_optional(pool)
                    .await?
                    .flatten()
                    } else if let Some(tname) = p.get("tax_rule_name").and_then(|v| v.as_str()) {
                        sqlx::query_scalar(
                            "SELECT tax_rule_id FROM tax_rules WHERE name = ? AND is_active = 1",
                        )
                        .bind(tname.trim())
                        .fetch_optional(pool)
                        .await?
                        .flatten()
                    } else {
                        None
                    };
                let rp: f64 = p
                    .get("reorder_point")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);

                let pid = ulid::Ulid::new().to_string();
                sqlx::query(
                    "INSERT INTO products (product_id, name, sku, barcode, category_id, tax_rule_id, cost_minor, reorder_point, is_active, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)",
                )
                .bind(&pid).bind(name)
                .bind(if sku.is_empty() { None } else { Some(&sku) })
                .bind(if barcode.is_empty() { None } else { Some(&barcode) })
                .bind(cat_id.as_deref())
                .bind(tax_id.as_deref())
                .bind(cost).bind(rp)
                .bind(&now).bind(&now)
                .execute(pool).await?;
                if price > 0 {
                    sqlx::query(
                        "INSERT INTO product_prices (price_id, product_id, price_type, price_minor, effective_from, created_at, created_by_user_id)
                         VALUES (?, ?, 'selling', ?, ?, ?, 'zanai-create-products')",
                    )
                    .bind(ulid::Ulid::new().to_string()).bind(&pid).bind(price).bind(&now).bind(&now)
                    .execute(pool).await?;
                }
                count += 1;
            }
            let summary = if skipped.is_empty() {
                format!("Created {count} product(s).")
            } else {
                let shown: Vec<&String> = skipped.iter().take(15).collect();
                let more = skipped.len().saturating_sub(shown.len());
                let mut msg = format!(
                    "Created {count} product(s). Skipped {}:\n- {}",
                    skipped.len(),
                    shown
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join("\n- ")
                );
                if more > 0 {
                    msg.push_str(&format!("\n…and {more} more."));
                }
                msg
            };
            ok_mut(&summary, "product_batch_create", "batch")
        }

        "bulk_import_categories" => {
            let csv_b64 = req_v(input, "csv_base64")?;
            use base64::Engine as _;
            let csv_bytes = base64::engine::general_purpose::STANDARD
                .decode(csv_b64.as_bytes())
                .map_err(|e| AppError::Validation(format!("Invalid base64: {e}")))?;
            let csv_str = String::from_utf8(csv_bytes)
                .map_err(|e| AppError::Validation(format!("Invalid UTF-8: {e}")))?;

            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .flexible(true)
                .from_reader(csv_str.as_bytes());
            let now = chrono::Utc::now().to_rfc3339();
            let mut count = 0u64;
            for result in reader.records() {
                let row =
                    result.map_err(|e| AppError::Validation(format!("CSV parse error: {e}")))?;
                if row.is_empty() {
                    continue;
                }
                let name = row.get(0).unwrap_or("").trim().to_string();
                if name.is_empty() {
                    continue;
                }
                let cid = ulid::Ulid::new().to_string();
                sqlx::query(
                    "INSERT INTO categories (category_id, name, is_active, created_at, updated_at) VALUES (?, ?, 1, ?, ?)",
                )
                .bind(&cid).bind(&name).bind(&now).bind(&now)
                .execute(pool).await?;
                count += 1;
            }
            ok_mut(
                &format!("Imported {count} category/categories."),
                "category_bulk_import",
                "batch",
            )
        }

        "send_receipt_via_whatsapp" => {
            use base64::Engine as _;

            let receipt_number = req_v(input, "receipt_number")?;
            let phone = req_v(input, "phone")?;

            let sidecar_token = {
                let app_data = std::env::var("APPDATA").unwrap_or_default();
                let path = std::path::Path::new(&app_data)
                    .join("com.super.zanpos")
                    .join("wa-session")
                    .join(".sidecar_token");
                std::fs::read_to_string(&path)
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            };
            if sidecar_token.is_empty() {
                return Err(AppError::Internal(
                    "WhatsApp sidecar token not found".into(),
                ));
            }

            let client = reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(2))
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .unwrap_or_default();

            match client
                .get("http://127.0.0.1:3131/health")
                .header("X-Sidecar-Token", &sidecar_token)
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {}
                _ => return Err(AppError::Internal("WhatsApp sidecar is not running".into())),
            }

            let sale = sqlx::query(
                "SELECT s.sale_id, s.receipt_number, s.net_total_minor, s.tax_total_minor, s.discount_total_minor,
                        s.sold_at, s.currency,
                        b.name as branch_name, b.phone as branch_phone, b.tax_number, b.cr_number, b.address,
                        u.display_name as cashier_name
                 FROM sales s
                 JOIN branches b ON b.branch_id = s.branch_id
                 JOIN users u ON u.user_id = s.cashier_user_id
                 WHERE s.receipt_number = ?"
            )
            .bind(&receipt_number)
            .fetch_optional(pool).await?
            .ok_or_else(|| AppError::NotFound(format!("Sale {receipt_number} not found")))?;

            let net: i64 = sale.try_get("net_total_minor").unwrap_or(0);
            let tax: i64 = sale.try_get("tax_total_minor").unwrap_or(0);
            let disc: i64 = sale.try_get("discount_total_minor").unwrap_or(0);
            let sold_at: String = sale.try_get("sold_at").unwrap_or_default();
            let currency: String = sale.try_get("currency").unwrap_or_else(|_| "BHD".into());
            let b_name: String = sale.try_get("branch_name").unwrap_or_default();
            let b_phone: Option<String> = sale.try_get("branch_phone").ok().flatten();
            let b_tax: Option<String> = sale.try_get("tax_number").ok().flatten();
            let b_cr: Option<String> = sale.try_get("cr_number").ok().flatten();
            let b_addr: Option<String> = sale.try_get("address").ok().flatten();
            let cashier: String = sale.try_get("cashier_name").unwrap_or_default();

            let items = sqlx::query(
                "SELECT si.product_name_snapshot, si.quantity, si.unit_price_minor, si.line_total_minor
                 FROM sale_items si WHERE si.sale_id = (SELECT sale_id FROM sales WHERE receipt_number = ?)"
            )
            .bind(&receipt_number)
            .fetch_all(pool).await?;

            let mut receipt_items = Vec::new();
            for it in &items {
                let pname: String = it.try_get("product_name_snapshot").unwrap_or_default();
                let qty: f64 = it.try_get("quantity").unwrap_or(1.0);
                let unit: i64 = it.try_get("unit_price_minor").unwrap_or(0);
                let line: i64 = it.try_get("line_total_minor").unwrap_or(0);
                receipt_items.push(crate::commands::receipt_pdf::ReceiptItemInput {
                    product_name: pname,
                    quantity: qty.to_string(),
                    unit_price_minor: unit,
                    line_total_minor: line,
                });
            }

            let pmts = sqlx::query(
                "SELECT payment_method, amount_minor FROM payments WHERE sale_id = (SELECT sale_id FROM sales WHERE receipt_number = ?)"
            )
            .bind(&receipt_number)
            .fetch_all(pool).await?;

            let mut payments = Vec::new();
            for p in &pmts {
                let method: String = p.try_get("payment_method").unwrap_or_default();
                let amt: i64 = p.try_get("amount_minor").unwrap_or(0);
                payments.push(crate::commands::receipt_pdf::PaymentSummaryInput {
                    method,
                    change_minor: Some(0),
                    amount_minor: amt,
                });
            }

            let pdf_input = crate::commands::receipt_pdf::WhatsAppReceiptPdfInput {
                to: phone.clone(),
                receipt_number: receipt_number.clone(),
                branch_name: b_name,
                branch_phone: b_phone,
                cashier_name: cashier,
                sold_at,
                currency,
                currency_exponent: 3,
                items: receipt_items,
                net_total_minor: net,
                tax_total_minor: tax,
                discount_total_minor: disc,
                payments,
                caption: Some(format!("Receipt {} from ZANPOS", receipt_number)),
                address_text: b_addr,
                house_number: None,
                area: None,
                tax_number: b_tax,
                cr_number: b_cr,
            };

            let pdf_bytes = crate::commands::receipt_pdf::generate_receipt_pdf(&pdf_input)
                .map_err(|e| AppError::Internal(format!("PDF generation failed: {e}")))?;
            let pdf_b64 = base64::engine::general_purpose::STANDARD.encode(&pdf_bytes);
            let filename = format!("Receipt-{}.pdf", receipt_number);

            let client = reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default();

            let resp = client
                .post("http://127.0.0.1:3131/send-document")
                .header("X-Sidecar-Token", &sidecar_token)
                .json(&serde_json::json!({
                    "to": phone,
                    "caption": format!("Receipt {} from ZANPOS", receipt_number),
                    "document_base64": pdf_b64,
                    "mimetype": "application/pdf",
                    "filename": filename,
                }))
                .send()
                .await
                .map_err(|e| AppError::Internal(format!("WhatsApp sidecar unreachable: {e}")))?;

            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let ok = body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
            if ok {
                ok_mut(
                    &format!("Receipt {receipt_number} sent to {phone} via WhatsApp."),
                    "receipt",
                    &receipt_number,
                )
            } else {
                let err = body
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                Err(AppError::Internal(format!("WhatsApp send failed: {err}")))
            }
        }

        other => crate::ai::tools_write_ext3::execute(pool, other, input, currency_exp).await,
    }
}
