//! Round-2 extension mutation tools: supplier delete, PO receive/delete,
//! bulk_assign_supplier, force_close_shift.

pub use crate::ai::tools::MutationResult;
use crate::domain::ai_admin::{ToolPreview, ToolPreviewField};
use crate::errors::{AppError, AppResult};
use sqlx::{Row, SqlitePool};

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

fn sv(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}
fn rv(v: &serde_json::Value, key: &str) -> AppResult<String> {
    let s = sv(v, key);
    if s.is_empty() {
        Err(AppError::Validation(format!("{key} is required")))
    } else {
        Ok(s)
    }
}

async fn audit3(pool: &SqlitePool, event: &str, entity_id: &str, after: &str) {
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

// ── Dry-run previews ──────────────────────────────────────────────────────────

pub async fn dry_run(
    pool: &SqlitePool,
    tool_name: &str,
    input: &serde_json::Value,
    _currency_exp: u32,
) -> AppResult<ToolPreview> {
    match tool_name {
        "delete_supplier" => {
            let id = rv(input, "supplier_id")?;
            let name: Option<String> =
                sqlx::query_scalar("SELECT name FROM suppliers WHERE supplier_id = ?")
                    .bind(&id)
                    .fetch_optional(pool)
                    .await?
                    .flatten();
            Ok(prev(tool_name, "Delete a supplier", vec![
                ("Supplier", name.unwrap_or_else(|| id.clone())),
                ("⚠ Warning", "Blocked if linked to POs or products".into()),
            ]))
        }
        "receive_purchase_order" => {
            let po_id = rv(input, "po_id")?;
            let line_count = input.get("lines").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            Ok(prev(tool_name, "Mark PO lines as received", vec![
                ("PO ID", po_id),
                ("Lines", if line_count == 0 { "All lines (full receipt)".into() } else { format!("{line_count} line(s)") }),
                ("Effect", "Stock will be incremented for each received line".into()),
            ]))
        }
        "delete_purchase_order" => Ok(prev(tool_name, "Cancel and delete a purchase order", vec![
            ("PO ID", rv(input, "po_id")?),
            ("⚠ Warning", "Blocked if PO has already been received".into()),
        ])),
        "bulk_assign_supplier" => Ok(prev(tool_name, "Assign supplier to all products in category", vec![
            ("Category ID", rv(input, "category_id")?),
            ("Supplier ID", rv(input, "supplier_id")?),
        ])),
        "force_close_shift" => {
            let shift_id = rv(input, "shift_id")?;
            Ok(prev(tool_name, "Force-close a stuck open shift", vec![
                ("Shift ID", shift_id),
                ("Effect", "Closes the shift with current timestamp. Logged in audit trail.".into()),
            ]))
        }
        other => Err(AppError::Validation(format!("Unknown mutation tool: {other}"))),
    }
}

// ── Mutation executors ────────────────────────────────────────────────────────

pub async fn execute(
    pool: &SqlitePool,
    tool_name: &str,
    input: &serde_json::Value,
    _currency_exp: u32,
) -> AppResult<MutationResult> {
    match tool_name {
        "delete_supplier" => {
            let id = rv(input, "supplier_id")?;
            let po_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM purchase_orders WHERE supplier_id = ?")
                    .bind(&id).fetch_one(pool).await?;
            if po_count > 0 {
                return Err(AppError::Validation(format!(
                    "Supplier has {po_count} purchase order(s). Cancel them before deleting the supplier."
                )));
            }
            let product_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE default_supplier_id = ?")
                    .bind(&id).fetch_one(pool).await?;
            if product_count > 0 {
                return Err(AppError::Validation(format!(
                    "Supplier is linked to {product_count} product(s). Reassign them first."
                )));
            }
            sqlx::query("DELETE FROM suppliers WHERE supplier_id = ?")
                .bind(&id).execute(pool).await?;
            audit3(pool, "supplier_deleted", &id, "{}").await;
            ok_mut(&format!("Supplier {id} deleted."), "supplier", &id)
        }

        "receive_purchase_order" => {
            let po_id = rv(input, "po_id")?;
            let now = chrono::Utc::now().to_rfc3339();

            // Check PO exists and isn't already fully received
            let status: Option<String> =
                sqlx::query_scalar("SELECT status FROM purchase_orders WHERE po_id = ?")
                    .bind(&po_id).fetch_optional(pool).await?.flatten();
            match status.as_deref() {
                None => return Err(AppError::NotFound(format!("PO {po_id} not found"))),
                Some("cancelled") => return Err(AppError::Validation("Cannot receive a cancelled PO.".into())),
                Some("received") => return Err(AppError::Validation("PO is already fully received.".into())),
                _ => {}
            }

            let explicit_lines = input.get("lines").and_then(|v| v.as_array());

            // Load all PO lines
            let po_lines = sqlx::query(
                "SELECT po_line_id, product_id, ordered_qty, received_qty, unit_cost_minor
                 FROM purchase_order_lines WHERE po_id = ?",
            )
            .bind(&po_id)
            .fetch_all(pool).await?;

            let mut received_count = 0u32;
            for line in &po_lines {
                let line_id: String = line.get("po_line_id");
                let product_id: Option<String> = line.try_get("product_id").ok();
                let ordered: f64 = line.try_get::<f64, _>("ordered_qty").unwrap_or(0.0);
                let already_received: f64 = line.try_get::<f64, _>("received_qty").unwrap_or(0.0);
                let unit_cost: i64 = line.try_get::<i64, _>("unit_cost_minor").unwrap_or(0);

                let recv_qty = if let Some(lines) = explicit_lines {
                    // Only process lines that appear in the explicit list
                    let found = lines.iter().find(|l| {
                        l.get("po_line_id").and_then(|v| v.as_str()) == Some(&line_id)
                    });
                    match found {
                        None => continue,
                        Some(l) => l.get("received_qty").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    }
                } else {
                    // Receive all remaining
                    ordered - already_received
                };

                if recv_qty <= 0.0 { continue; }

                // Update received qty on the line
                sqlx::query(
                    "UPDATE purchase_order_lines SET received_qty = received_qty + ? WHERE po_line_id = ?",
                )
                .bind(recv_qty).bind(&line_id).execute(pool).await?;

                // Increment product stock if product is linked
                if let Some(pid) = &product_id {
                    sqlx::query(
                        "UPDATE products SET stock_quantity = COALESCE(stock_quantity,0) + ?,
                         cost_minor = CASE WHEN ? > 0 THEN ? ELSE cost_minor END,
                         updated_at = ?
                         WHERE product_id = ?",
                    )
                    .bind(recv_qty)
                    .bind(unit_cost)
                    .bind(unit_cost)
                    .bind(&now)
                    .bind(pid)
                    .execute(pool).await?;
                }
                received_count += 1;
            }

            // Update PO status: check if fully received
            let (total_ordered, total_received): (f64, f64) = sqlx::query_as(
                "SELECT COALESCE(SUM(ordered_qty),0), COALESCE(SUM(received_qty),0)
                 FROM purchase_order_lines WHERE po_id = ?",
            )
            .bind(&po_id)
            .fetch_one(pool).await.unwrap_or((0.0, 0.0));

            let new_status = if total_received >= total_ordered && total_ordered > 0.0 {
                "received"
            } else if total_received > 0.0 {
                "partial"
            } else {
                "ordered"
            };

            sqlx::query("UPDATE purchase_orders SET status = ?, received_date = ?, updated_at = ? WHERE po_id = ?")
                .bind(new_status).bind(&now).bind(&now).bind(&po_id)
                .execute(pool).await?;

            audit3(pool, "po_received", &po_id, &format!("{{\"lines_received\":{received_count},\"status\":\"{new_status}\"}}")).await;
            ok_mut(
                &format!("PO {po_id}: {received_count} line(s) received, status → {new_status}."),
                "purchase_order",
                &po_id,
            )
        }

        "delete_purchase_order" => {
            let po_id = rv(input, "po_id")?;
            let status: Option<String> =
                sqlx::query_scalar("SELECT status FROM purchase_orders WHERE po_id = ?")
                    .bind(&po_id).fetch_optional(pool).await?.flatten();
            match status.as_deref() {
                None => return Err(AppError::NotFound(format!("PO {po_id} not found"))),
                Some("received") | Some("partial") => {
                    return Err(AppError::Validation(
                        "Cannot delete a PO that has been received (full or partial). Cancel it instead via update_purchase_order.".into(),
                    ))
                }
                _ => {}
            }
            sqlx::query("DELETE FROM purchase_order_lines WHERE po_id = ?")
                .bind(&po_id).execute(pool).await?;
            sqlx::query("DELETE FROM purchase_orders WHERE po_id = ?")
                .bind(&po_id).execute(pool).await?;
            audit3(pool, "purchase_order_deleted", &po_id, "{}").await;
            ok_mut(&format!("Purchase order {po_id} deleted."), "purchase_order", &po_id)
        }

        "bulk_assign_supplier" => {
            let cid = rv(input, "category_id")?;
            let sid = rv(input, "supplier_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            let count = sqlx::query(
                "UPDATE products SET default_supplier_id = ?, updated_at = ? WHERE category_id = ?",
            )
            .bind(&sid).bind(&now).bind(&cid)
            .execute(pool).await?.rows_affected();
            audit3(pool, "bulk_assign_supplier", &cid, &format!("{{\"supplier_id\":\"{sid}\",\"count\":{count}}}")).await;
            ok_mut(
                &format!("{count} product(s) in category {cid} assigned to supplier {sid}."),
                "category",
                &cid,
            )
        }

        "force_close_shift" => {
            let shift_id = rv(input, "shift_id")?;
            let existing = sqlx::query(
                "SELECT closed_at FROM shifts WHERE shift_id = ?",
            )
            .bind(&shift_id)
            .fetch_optional(pool).await?
            .ok_or_else(|| AppError::NotFound(format!("Shift {shift_id} not found")))?;
            let closed_at: Option<String> = existing.try_get("closed_at").ok().flatten();
            if closed_at.as_deref().map(|s| !s.is_empty()).unwrap_or(false) {
                return Err(AppError::Validation(format!(
                    "Shift {shift_id} is already closed."
                )));
            }
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "UPDATE shifts SET closed_at = ?, updated_at = ? WHERE shift_id = ?",
            )
            .bind(&now).bind(&now).bind(&shift_id)
            .execute(pool).await?;
            audit3(pool, "shift_force_closed", &shift_id, "{}").await;
            ok_mut(
                &format!("Shift {shift_id} force-closed at {now}."),
                "shift",
                &shift_id,
            )
        }

        other => Err(AppError::Validation(format!("Unknown mutation tool: {other}"))),
    }
}
