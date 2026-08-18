//! Round-2 extension mutation tools: supplier delete, PO receive/delete,
//! bulk_assign_supplier, force_close_shift.

pub use crate::ai::tools::MutationResult;
use crate::commands::purchasing_commands::{
    po_receive_inner, ReceivePurchaseOrderInput, ReceivePurchaseOrderLineInput,
};
use crate::domain::ai_admin::{ToolPreview, ToolPreviewField};
use crate::errors::{AppError, AppResult};
use sqlx::{Row, SqlitePool};
use ulid::Ulid;

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
            Ok(prev(
                tool_name,
                "Delete a supplier",
                vec![
                    ("Supplier", name.unwrap_or_else(|| id.clone())),
                    ("⚠ Warning", "Blocked if linked to POs or products".into()),
                ],
            ))
        }
        "receive_purchase_order" => {
            let po_id = rv(input, "po_id")?;
            let line_count = input
                .get("lines")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            Ok(prev(
                tool_name,
                "Mark PO lines as received",
                vec![
                    ("PO ID", po_id),
                    (
                        "Lines",
                        if line_count == 0 {
                            "All lines (full receipt)".into()
                        } else {
                            format!("{line_count} line(s)")
                        },
                    ),
                    (
                        "Effect",
                        "Stock will be incremented for each received line".into(),
                    ),
                ],
            ))
        }
        "delete_purchase_order" => Ok(prev(
            tool_name,
            "Cancel and delete a purchase order",
            vec![
                ("PO ID", rv(input, "po_id")?),
                (
                    "⚠ Warning",
                    "Blocked if PO has already been received".into(),
                ),
            ],
        )),
        "bulk_assign_supplier" => Ok(prev(
            tool_name,
            "Assign supplier to all products in category",
            vec![
                ("Category ID", rv(input, "category_id")?),
                ("Supplier ID", rv(input, "supplier_id")?),
            ],
        )),
        "force_close_shift" => {
            let shift_id = rv(input, "shift_id")?;
            Ok(prev(
                tool_name,
                "Force-close a stuck open shift",
                vec![
                    ("Shift ID", shift_id),
                    (
                        "Effect",
                        "Closes the shift with current timestamp. Logged in audit trail.".into(),
                    ),
                ],
            ))
        }
        "merge_products" => {
            let source_id = rv(input, "source_product_id")?;
            let target_id = rv(input, "target_product_id")?;
            if source_id == target_id {
                return Err(AppError::Validation(
                    "source and target product must be different".into(),
                ));
            }

            let source_name: Option<String> = sqlx::query_scalar(
                "SELECT name FROM products WHERE product_id = ? AND deleted_at IS NULL",
            )
            .bind(&source_id)
            .fetch_optional(pool)
            .await?
            .flatten();
            let source_name = source_name.ok_or_else(|| {
                AppError::Validation(format!(
                    "Source product {source_id} not found or already deleted"
                ))
            })?;

            let target_name: Option<String> = sqlx::query_scalar(
                "SELECT name FROM products WHERE product_id = ? AND deleted_at IS NULL",
            )
            .bind(&target_id)
            .fetch_optional(pool)
            .await?
            .flatten();
            let target_name = target_name.ok_or_else(|| {
                AppError::Validation(format!(
                    "Target product {target_id} not found or already deleted"
                ))
            })?;

            let transfer_history = input
                .get("transfer_history")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            Ok(prev(
                tool_name,
                &format!("Merge '{source_name}' into '{target_name}'"),
                vec![
                    ("Source", format!("{source_name} ({source_id})")),
                    ("Target", format!("{target_name} ({target_id})")),
                    ("Stock", "Source stock will be combined into target".into()),
                    (
                        "Sale History",
                        if transfer_history {
                            "Historical sale items will be reassigned to target".into()
                        } else {
                            "Historical sale items will remain linked to source".into()
                        },
                    ),
                    (
                        "Warning",
                        "Source product will be archived; this merge is irreversible".into(),
                    ),
                ],
            ))
        }
        other => Err(AppError::Validation(format!(
            "Unknown mutation tool: {other}"
        ))),
    }
}

// ── Mutation executors ────────────────────────────────────────────────────────

pub async fn execute(
    pool: &SqlitePool,
    tool_name: &str,
    input: &serde_json::Value,
    _currency_exp: u32,
) -> AppResult<MutationResult> {
    let actor_id = crate::ai::tool_policy::current_actor_id()
        .ok_or_else(|| AppError::Permission("Mutation actor context is missing".into()))?;
    match tool_name {
        "delete_supplier" => {
            let id = rv(input, "supplier_id")?;
            let po_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM purchase_orders WHERE supplier_id = ?")
                    .bind(&id)
                    .fetch_one(pool)
                    .await?;
            if po_count > 0 {
                return Err(AppError::Validation(format!(
                    "Supplier has {po_count} purchase order(s). Cancel them before deleting the supplier."
                )));
            }
            let product_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE default_supplier_id = ?")
                    .bind(&id)
                    .fetch_one(pool)
                    .await?;
            if product_count > 0 {
                return Err(AppError::Validation(format!(
                    "Supplier is linked to {product_count} product(s). Reassign them first."
                )));
            }
            sqlx::query("DELETE FROM suppliers WHERE supplier_id = ?")
                .bind(&id)
                .execute(pool)
                .await?;
            audit3(pool, "supplier_deleted", &id, "{}").await;
            ok_mut(&format!("Supplier {id} deleted."), "supplier", &id)
        }

        "receive_purchase_order" => {
            let po_id = rv(input, "po_id")?;
            let lines = input.get("lines").and_then(|v| v.as_array()).map(|items| {
                items
                    .iter()
                    .filter_map(|line| {
                        let po_line_id = line.get("po_line_id")?.as_str()?.to_string();
                        let received_qty = line
                            .get("received_qty")
                            .map(|v| {
                                v.as_str()
                                    .map(ToString::to_string)
                                    .unwrap_or_else(|| v.to_string())
                            })
                            .unwrap_or_else(|| "0".to_string());
                        Some(ReceivePurchaseOrderLineInput {
                            po_line_id,
                            received_qty,
                            expiry_date: line
                                .get("expiry_date")
                                .and_then(|value| value.as_str())
                                .map(ToString::to_string),
                        })
                    })
                    .collect::<Vec<_>>()
            });
            let (device_id, branch_id) = crate::ai::tools::active_device_branch(pool).await?;
            let result = po_receive_inner(
                pool,
                ReceivePurchaseOrderInput {
                    po_id: po_id.clone(),
                    actor_user_id: actor_id.clone(),
                    lines,
                    // A fresh key per execution. Replay protection for this
                    // path lives one layer up: an `ai_actions` row carries the
                    // operation identity and its status transitions from
                    // prepared to executed, so a confirmed action cannot be
                    // executed twice. The key here records the receipt; it is
                    // not what prevents the replay.
                    idempotency_key: Ulid::new().to_string(),
                },
                &branch_id,
                &device_id,
            )
            .await?;

            audit3(
                pool,
                "po_received",
                &po_id,
                &format!(
                    "{{\"lines_received\":{},\"status\":\"{}\",\"cost_updates\":{}}}",
                    result.lines_received, result.status, result.cost_updates
                ),
            )
            .await;
            ok_mut(
                &format!(
                    "PO {po_id}: {} line(s), {} unit(s) received, status → {}.",
                    result.lines_received, result.units_received, result.status
                ),
                "purchase_order",
                &po_id,
            )
        }

        "delete_purchase_order" => {
            let po_id = rv(input, "po_id")?;
            let status: Option<String> =
                sqlx::query_scalar("SELECT status FROM purchase_orders WHERE po_id = ?")
                    .bind(&po_id)
                    .fetch_optional(pool)
                    .await?
                    .flatten();
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
                .bind(&po_id)
                .execute(pool)
                .await?;
            sqlx::query("DELETE FROM purchase_orders WHERE po_id = ?")
                .bind(&po_id)
                .execute(pool)
                .await?;
            audit3(pool, "purchase_order_deleted", &po_id, "{}").await;
            ok_mut(
                &format!("Purchase order {po_id} deleted."),
                "purchase_order",
                &po_id,
            )
        }

        "bulk_assign_supplier" => {
            let cid = rv(input, "category_id")?;
            let sid = rv(input, "supplier_id")?;
            let now = chrono::Utc::now().to_rfc3339();
            let count = sqlx::query(
                "UPDATE products SET default_supplier_id = ?, updated_at = ?, sync_status = 'pending' WHERE category_id = ?",
            )
            .bind(&sid)
            .bind(&now)
            .bind(&cid)
            .execute(pool)
            .await?
            .rows_affected();
            audit3(
                pool,
                "bulk_assign_supplier",
                &cid,
                &format!("{{\"supplier_id\":\"{sid}\",\"count\":{count}}}"),
            )
            .await;
            ok_mut(
                &format!("{count} product(s) in category {cid} assigned to supplier {sid}."),
                "category",
                &cid,
            )
        }

        "force_close_shift" => {
            let shift_id = rv(input, "shift_id")?;
            let existing = sqlx::query("SELECT closed_at FROM shifts WHERE shift_id = ?")
                .bind(&shift_id)
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("Shift {shift_id} not found")))?;
            let closed_at: Option<String> = existing.try_get("closed_at").ok().flatten();
            if closed_at.as_deref().map(|s| !s.is_empty()).unwrap_or(false) {
                return Err(AppError::Validation(format!(
                    "Shift {shift_id} is already closed."
                )));
            }
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE shifts SET closed_at = ?, updated_at = ? WHERE shift_id = ?")
                .bind(&now)
                .bind(&now)
                .bind(&shift_id)
                .execute(pool)
                .await?;
            audit3(pool, "shift_force_closed", &shift_id, "{}").await;
            ok_mut(
                &format!("Shift {shift_id} force-closed at {now}."),
                "shift",
                &shift_id,
            )
        }

        "merge_products" => {
            let source_id = rv(input, "source_product_id")?;
            let target_id = rv(input, "target_product_id")?;
            let transfer_history = input
                .get("transfer_history")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let outcome = crate::db::repositories::product_dedup_repo::merge_products(
                pool,
                &source_id,
                &target_id,
                transfer_history,
            )
            .await?;

            audit3(
                pool,
                "product_merged",
                &source_id,
                &format!(
                    r#"{{"merged_into":"{}","transfer_history":{}}}"#,
                    target_id, transfer_history
                ),
            )
            .await;

            let history_note = if transfer_history {
                "Sale history reassigned to target."
            } else {
                "Historical sale records left on source (archived)."
            };

            ok_mut(
                &format!(
                    "Merged \"{}\" into \"{}\". Stock combined. {history_note} Source archived.",
                    outcome.source_name, outcome.target_name
                ),
                "product",
                &source_id,
            )
        }

        other => Err(AppError::Validation(format!(
            "Unknown mutation tool: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn merge_products_preview_names_both_products_without_mutating_them() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");
        sqlx::query(
            "INSERT INTO categories
             (category_id, name, sort_order, is_active, created_at, updated_at, version)
             VALUES ('CAT-MERGE-PREVIEW', 'Merge Preview', 1, 1, datetime('now'), datetime('now'), 1)",
        )
        .execute(&pool)
        .await
        .expect("seed category");
        for (id, name) in [("P-SOURCE", "Duplicate Cola"), ("P-TARGET", "Cola")] {
            sqlx::query(
                "INSERT INTO products
                 (product_id, category_id, name, track_inventory, is_active, currency,
                  created_at, updated_at, version)
                 VALUES (?, 'CAT-MERGE-PREVIEW', ?, 1, 1, 'BHD', datetime('now'), datetime('now'), 1)",
            )
            .bind(id)
            .bind(name)
            .execute(&pool)
            .await
            .expect("seed product");
        }

        let preview = dry_run(
            &pool,
            "merge_products",
            &serde_json::json!({
                "source_product_id": "P-SOURCE",
                "target_product_id": "P-TARGET",
                "transfer_history": true
            }),
            3,
        )
        .await
        .expect("merge preview");

        assert_eq!(preview.tool_name, "merge_products");
        assert!(preview.description.contains("Duplicate Cola"));
        assert!(preview.description.contains("Cola"));
        assert!(preview
            .fields
            .iter()
            .any(|field| field.label == "Sale History" && field.value.contains("reassigned")));

        let source_state: (i64, Option<String>) = sqlx::query_as(
            "SELECT is_active, deleted_at FROM products WHERE product_id = 'P-SOURCE'",
        )
        .fetch_one(&pool)
        .await
        .expect("source state");
        assert_eq!(source_state, (1, None));
    }
}
