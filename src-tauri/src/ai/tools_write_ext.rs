//! Extension mutation tools — 21 new mutations dispatched from tools.rs catch-all arms.

pub use crate::ai::tools::MutationResult;
use crate::db::repositories::{delivery_repo, held_cart_repo, refund_repo};
use crate::domain::ai_admin::{ToolPreview, ToolPreviewField};
use crate::domain::delivery::RevertPaymentInput;
use crate::domain::money;
use crate::domain::refund::RefundItemInput;
use crate::errors::{AppError, AppResult};
use sqlx::SqlitePool;

// ── Local helpers ─────────────────────────────────────────────────────────────

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

async fn audit_ext(pool: &SqlitePool, event: &str, entity_id: &str, after: &str) {
    let id = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let hash = format!("{:x}", hash8(&format!("{}{}{}", id, event, now)));
    if let Err(error) = sqlx::query(
        "INSERT INTO audit_logs (audit_log_id, event_type, entity_type, entity_id,
         actor_user_id, actor_type, after_json, created_at, hash)
         VALUES (?, ?, 'ai_mutation', ?, ?, 'ai_agent', ?, ?, ?)",
    )
    .bind(&id)
    .bind(event)
    .bind(entity_id)
    .bind(crate::ai::tool_policy::current_actor_id().unwrap_or_else(|| "unknown".into()))
    .bind(after)
    .bind(&now)
    .bind(&hash)
    .execute(pool)
    .await
    {
        tracing::error!("AI audit write failed: {error}");
    }
}

fn hash8(s: &str) -> u64 {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let b = h.finalize();
    u64::from_le_bytes(b[..8].try_into().unwrap_or([0u8; 8]))
}

async fn cfg(pool: &SqlitePool, key: &str) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT value FROM app_config WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten()
}

async fn sidecar_send(phone: &str, msg: &str) -> AppResult<()> {
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
        .json(&serde_json::json!({ "to": phone, "message": msg }))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("WhatsApp sidecar unreachable: {e}")))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(AppError::Internal(format!(
            "Sidecar error: {}",
            resp.status()
        )))
    }
}

fn str(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}
fn req(v: &serde_json::Value, key: &str) -> AppResult<String> {
    let s = str(v, key);
    if s.is_empty() {
        Err(AppError::Validation(format!("{key} is required")))
    } else {
        Ok(s)
    }
}

// ── Dry-run previews ──────────────────────────────────────────────────────────

pub async fn dry_run(
    pool: &SqlitePool,
    tool_name: &str,
    input: &serde_json::Value,
    currency_exp: u32,
) -> AppResult<ToolPreview> {
    // Parse a BHD string amount and re-format it as "BHD X.XXX" (3 decimal places).
    // Falls back to the raw string if parsing fails.
    let fmt_bhd = |raw: &str| -> String {
        money::parse_major_to_minor(raw, 3)
            .map(|minor| format!("BHD {}", money::format_minor(minor, currency_exp)))
            .unwrap_or_else(|| raw.to_string())
    };
    match tool_name {
        "create_refund" => Ok(prev(
            tool_name,
            "Process a refund for a past sale",
            vec![
                ("Receipt", req(input, "receipt_number")?),
                ("Reason", str(input, "reason")),
            ],
        )),
        "create_cash_event" => Ok(prev(
            tool_name,
            "Record a cash event (paid in/out/safe drop)",
            vec![
                ("Type", req(input, "event_type")?),
                ("Amount", fmt_bhd(&req(input, "amount_bhd")?)),
                ("Note", str(input, "note")),
            ],
        )),
        "open_shift" => {
            let raw = str(input, "opening_cash_bhd");
            let opening_display = if raw.is_empty() {
                "0.000".to_string()
            } else {
                fmt_bhd(&raw)
            };
            Ok(prev(
                tool_name,
                "Open a new cashier shift",
                vec![
                    ("Cashier", req(input, "cashier_user_id")?),
                    ("Opening Cash", opening_display),
                ],
            ))
        }
        "close_shift" => {
            let raw = str(input, "counted_cash_bhd");
            let counted_display = if raw.is_empty() {
                "(not counted)".to_string()
            } else {
                fmt_bhd(&raw)
            };
            Ok(prev(
                tool_name,
                "Close the current shift",
                vec![
                    ("Shift ID", req(input, "shift_id")?),
                    ("Counted Cash", counted_display),
                ],
            ))
        }
        "add_product_barcode" => Ok(prev(
            tool_name,
            "Register an additional barcode for a product",
            vec![
                ("Product ID", req(input, "product_id")?),
                ("Barcode", req(input, "barcode")?),
            ],
        )),
        "remove_product_barcode" => Ok(prev(
            tool_name,
            "Remove a barcode registration",
            vec![("Barcode ID", req(input, "barcode_id")?)],
        )),
        "trigger_sync_now" => Ok(prev(
            tool_name,
            "Reset sync retry counters so worker picks up pending rows immediately",
            vec![(
                "Action",
                "Reset sync_attempts to 0 for all pending rows".to_string(),
            )],
        )),
        "force_full_resync" => Ok(prev(
            tool_name,
            "Force complete resync — marks all rows pending and resets watermarks",
            vec![(
                "⚠ Warning",
                "This re-uploads everything. Sync may take several minutes.".to_string(),
            )],
        )),
        "revert_delivery_payment" => Ok(prev(
            tool_name,
            "Reverse a delivery payment status back to unpaid",
            vec![
                ("Delivery ID", req(input, "delivery_id")?),
                ("Reason", str(input, "reason")),
            ],
        )),
        "update_branch_settings" => Ok(prev(
            tool_name,
            "Update branch/store configuration",
            vec![
                ("Name", str(input, "name")),
                ("Phone", str(input, "phone")),
                ("Tax#", str(input, "tax_number")),
            ],
        )),
        "register_device" => Ok(prev(
            tool_name,
            "Register a new POS device/terminal",
            vec![
                ("Code", req(input, "device_code")?),
                ("Name", req(input, "name")?),
            ],
        )),
        "send_whatsapp_delivery_alert"
        | "send_whatsapp_payment_reminder"
        | "send_whatsapp_arrival_notice" => Ok(prev(
            tool_name,
            "Send a WhatsApp notification",
            vec![
                ("Phone", req(input, "phone")?),
                ("Message", str(input, "message")),
            ],
        )),
        "disconnect_whatsapp" => Ok(prev(
            tool_name,
            "Disconnect the active WhatsApp session",
            vec![],
        )),
        "update_thermal_config" => Ok(prev(
            tool_name,
            "Update thermal printer configuration",
            vec![
                ("Port", str(input, "port")),
                ("Baud", str(input, "baud")),
                ("Enabled", str(input, "enabled")),
            ],
        )),
        "open_cash_drawer" => Ok(prev(
            tool_name,
            "Send ESC/POS pulse to open the cash drawer",
            vec![],
        )),
        "reprint_receipt" => Ok(prev(
            tool_name,
            "Reprint a past receipt to the thermal printer",
            vec![("Receipt#", req(input, "receipt_number")?)],
        )),
        "delete_held_cart" => Ok(prev(
            tool_name,
            "Discard a held/parked cart",
            vec![("Held Cart ID", req(input, "held_cart_id")?)],
        )),
        "update_benefit_number" => Ok(prev(
            tool_name,
            "Update the Benefit/Sadad payment phone number",
            vec![("Number", req(input, "benefit_number")?)],
        )),
        other => crate::ai::tools_write_ext2::dry_run(pool, other, input, currency_exp).await,
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
        "create_refund" => {
            let receipt = req(input, "receipt_number")?;
            let reason = str(input, "reason");
            let sale = refund_repo::get_sale_by_receipt(pool, &receipt).await?;
            let items: Vec<RefundItemInput> = sale
                .items
                .iter()
                .map(|i| RefundItemInput {
                    sale_item_id: i.sale_item_id.clone(),
                    product_name_snapshot: i.product_name_snapshot.clone(),
                    quantity: i.quantity.to_string(),
                    unit_price_minor: i.unit_price_minor,
                    refund_amount_minor: i.line_total_minor,
                })
                .collect();
            let r = refund_repo::create_refund(
                pool,
                &sale.sale_id,
                items,
                &reason,
                &actor_id,
                &actor_id,
                true,
            )
            .await?;
            audit_ext(
                pool,
                "refund_created",
                &r.refund_id,
                &format!("{{\"receipt\":\"{}\"}}", r.refund_receipt_number),
            )
            .await;
            // B-03: format refund total as BHD X.XXX (3 decimal places) not raw fils integer
            let fmt_total = money::format_minor(r.refund_total_minor, currency_exp);
            ok_mut(
                &format!(
                    "Refund #{} created — BD {}",
                    r.refund_receipt_number, fmt_total
                ),
                "refund",
                &r.refund_id,
            )
        }

        "create_cash_event" => {
            let shift_id = req(input, "shift_id")?;
            let event_type = req(input, "event_type")?;
            let amount_bhd = req(input, "amount_bhd")?;
            let note = str(input, "note");
            let amount_minor = money::parse_major_to_minor(&amount_bhd, 3)
                .ok_or_else(|| AppError::Validation(format!("Invalid BHD amount: {amount_bhd}")))?;
            let (device, branch) = crate::ai::tools::active_device_branch(pool).await?;
            let id = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("INSERT INTO cash_events (cash_event_id, shift_id, branch_id, device_id, origin_device_id, event_type, amount_minor, note, created_by_user_id, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?)")
                .bind(&id).bind(&shift_id).bind(&branch).bind(&device).bind(&device).bind(&event_type).bind(amount_minor).bind(&note).bind(&actor_id).bind(&now).bind(&now)
                .execute(pool).await?;
            audit_ext(
                pool,
                "cash_event_created",
                &id,
                &format!("{{\"type\":\"{event_type}\",\"amount_minor\":{amount_minor}}}"),
            )
            .await;
            ok_mut(
                &format!("Cash event '{event_type}' of BHD {amount_bhd} recorded."),
                "cash_event",
                &id,
            )
        }

        "open_shift" => {
            let cashier_id = req(input, "cashier_user_id")?;
            let opening_bhd = str(input, "opening_cash_bhd");
            let opening_minor = money::parse_major_to_minor(&opening_bhd, 3).unwrap_or(0);
            let (device, branch) = crate::ai::tools::active_device_branch(pool).await?;
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT shift_id FROM shifts WHERE device_id=? AND status='open'",
            )
            .bind(&device)
            .fetch_optional(pool)
            .await?;
            if existing.is_some() {
                return Err(AppError::Validation(
                    "A shift is already open on this device. Close it first.".into(),
                ));
            }
            let id = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            // Business date uses local time (Bahrain UTC+3) so the correct
            // calendar day is recorded even for shifts opened just after midnight UTC.
            let bd = chrono::Local::now().format("%Y-%m-%d").to_string();
            sqlx::query("INSERT INTO shifts (shift_id, branch_id, device_id, cashier_user_id, status, opening_cash_minor, business_date, opened_at) VALUES (?,?,?,?,'open',?,?,?)")
                .bind(&id).bind(&branch).bind(&device).bind(&cashier_id).bind(opening_minor).bind(&bd).bind(&now)
                .execute(pool).await?;
            audit_ext(
                pool,
                "shift_opened",
                &id,
                &format!("{{\"cashier\":\"{cashier_id}\",\"opening_minor\":{opening_minor}}}"),
            )
            .await;
            ok_mut(&format!("Shift {id} opened."), "shift", &id)
        }

        "close_shift" => {
            let shift_id = req(input, "shift_id")?;
            let counted_bhd = str(input, "counted_cash_bhd");
            let counted_minor = if counted_bhd.trim().is_empty() {
                None
            } else {
                money::parse_major_to_minor(&counted_bhd, 3)
            };
            let notes = str(input, "notes");
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE shifts SET status='closed', closed_at=?, counted_cash_minor=?, notes=? WHERE shift_id=?")
                .bind(&now).bind(counted_minor).bind(&notes).bind(&shift_id).execute(pool).await?;
            audit_ext(
                pool,
                "shift_closed",
                &shift_id,
                &format!("{{\"counted_minor\":{counted_minor:?}}}"),
            )
            .await;
            ok_mut(&format!("Shift {shift_id} closed."), "shift", &shift_id)
        }

        "add_product_barcode" => {
            let product_id = req(input, "product_id")?;
            let barcode = req(input, "barcode")?;
            let id = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            // updated_at, not just created_at: the hub's pull predicate runs
            // strftime() on it, and strftime('') is NULL, so a row left with the
            // column's empty default can never be served to any terminal.
            sqlx::query(
                "INSERT INTO product_barcodes
                   (barcode_id, product_id, barcode, created_at, updated_at)
                 VALUES (?,?,?,?,?)",
            )
                .bind(&id).bind(&product_id).bind(&barcode).bind(&now)
            .bind(&now)
            .execute(pool).await?;
            audit_ext(
                pool,
                "barcode_added",
                &id,
                &format!("{{\"barcode\":\"{barcode}\"}}"),
            )
            .await;
            ok_mut(
                &format!("Barcode '{barcode}' added to product {product_id}."),
                "product_barcode",
                &id,
            )
        }

        "remove_product_barcode" => {
            let barcode_id = req(input, "barcode_id")?;
            crate::db::repositories::product_repo::soft_delete_barcode(pool, &barcode_id).await?;
            ok_mut(
                &format!("Barcode {barcode_id} removed."),
                "product_barcode",
                &barcode_id,
            )
        }

        "trigger_sync_now" => {
            // Reset retry counters so background worker picks them up immediately
            for tbl in crate::commands::sync_commands::SYNC_TABLES.iter() {
                let _ = sqlx::query(&format!(
                    "UPDATE {tbl} SET sync_attempts=0 WHERE sync_status='pending'"
                ))
                .execute(pool)
                .await;
            }
            ok_mut(
                "Sync retry counters reset. Background worker will attempt sync within 30 seconds.",
                "sync",
                "global",
            )
        }

        "force_full_resync" => {
            let now_epoch = "1970-01-01T00:00:00Z";
            for tbl in crate::commands::sync_commands::SYNC_TABLES.iter() {
                let _ = sqlx::query(&format!("UPDATE {tbl} SET sync_status='pending', sync_attempts=0 WHERE sync_status='synced'")).execute(pool).await;
            }
            let _ = sqlx::query("UPDATE sync_watermark SET last_pulled_at=?")
                .bind(now_epoch)
                .execute(pool)
                .await;
            ok_mut("All tables marked pending. Watermarks reset to epoch. Full resync will run within 30 seconds.", "sync", "global")
        }

        "revert_delivery_payment" => {
            let delivery_id = req(input, "delivery_id")?;
            let reason = str(input, "reason");
            let d = delivery_repo::revert_payment(
                pool,
                &RevertPaymentInput {
                    delivery_id: delivery_id.clone(),
                    actor_user_id: actor_id.clone(),
                    reason: if reason.is_empty() {
                        None
                    } else {
                        Some(reason)
                    },
                },
            )
            .await?;
            audit_ext(
                pool,
                "delivery_payment_reverted",
                &delivery_id,
                &format!("{{\"status\":\"{}\"}}", d.payment_status),
            )
            .await;
            ok_mut(
                &format!("Delivery {delivery_id} payment reverted to unpaid."),
                "delivery",
                &delivery_id,
            )
        }

        "update_branch_settings" => {
            let now = chrono::Utc::now().to_rfc3339();
            let branch_id = crate::ai::tools::active_branch_id(pool).await?;
            sqlx::query("UPDATE branches SET name=COALESCE(NULLIF(?,  ''), name), timezone=COALESCE(NULLIF(?,  ''), timezone), address=COALESCE(NULLIF(?,''),address), phone=COALESCE(NULLIF(?,''),phone), receipt_header=COALESCE(NULLIF(?,''),receipt_header), receipt_footer=COALESCE(NULLIF(?,''),receipt_footer), tax_number=COALESCE(NULLIF(?,''),tax_number), cr_number=COALESCE(NULLIF(?,''),cr_number), updated_at=? WHERE branch_id=?")
                .bind(str(input,"name")).bind(str(input,"timezone")).bind(str(input,"address")).bind(str(input,"phone"))
                .bind(str(input,"receipt_header")).bind(str(input,"receipt_footer")).bind(str(input,"tax_number")).bind(str(input,"cr_number"))
                .bind(&now).bind(&branch_id).execute(pool).await?;
            audit_ext(pool, "branch_updated", "branch", &input.to_string()).await;
            ok_mut("Branch settings updated.", "branch", "active")
        }

        "register_device" => {
            let branch = crate::ai::tools::active_branch_id(pool).await?;
            let code = req(input, "device_code")?;
            let name = req(input, "name")?;
            let id = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("INSERT INTO devices (device_id, branch_id, device_code, name, status, is_active, created_at, updated_at) VALUES (?,?,?,?,'offline',1,?,?)")
                .bind(&id).bind(&branch).bind(&code).bind(&name).bind(&now).bind(&now).execute(pool).await?;
            audit_ext(
                pool,
                "device_registered",
                &id,
                &format!("{{\"code\":\"{code}\",\"name\":\"{name}\"}}"),
            )
            .await;
            ok_mut(
                &format!("Device '{name}' ({code}) registered with ID {id}."),
                "device",
                &id,
            )
        }

        "send_whatsapp_delivery_alert" => {
            let phone = req(input, "phone")?;
            let msg = str(input, "message");
            sidecar_send(
                &phone,
                &if msg.is_empty() {
                    "Your delivery order is on the way!".to_string()
                } else {
                    msg
                },
            )
            .await?;
            ok_mut(
                &format!("Delivery alert sent to {phone}."),
                "whatsapp",
                &phone,
            )
        }

        "send_whatsapp_payment_reminder" => {
            let phone = req(input, "phone")?;
            let msg = str(input, "message");
            sidecar_send(
                &phone,
                &if msg.is_empty() {
                    "Friendly reminder: your delivery payment is due.".to_string()
                } else {
                    msg
                },
            )
            .await?;
            ok_mut(
                &format!("Payment reminder sent to {phone}."),
                "whatsapp",
                &phone,
            )
        }

        "send_whatsapp_arrival_notice" => {
            let phone = req(input, "phone")?;
            let msg = str(input, "message");
            sidecar_send(
                &phone,
                &if msg.is_empty() {
                    "Your order has arrived! Please collect it.".to_string()
                } else {
                    msg
                },
            )
            .await?;
            ok_mut(
                &format!("Arrival notice sent to {phone}."),
                "whatsapp",
                &phone,
            )
        }

        "disconnect_whatsapp" => {
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
                .build()
                .unwrap_or_default();
            let _ = client
                .post("http://127.0.0.1:3131/disconnect")
                .header("X-Sidecar-Token", token.trim())
                .send()
                .await;
            ok_mut("WhatsApp session disconnected.", "whatsapp", "session")
        }

        "update_thermal_config" => {
            let port = str(input, "port");
            let baud = str(input, "baud");
            let enabled = str(input, "enabled");
            let now = chrono::Utc::now().to_rfc3339();
            if !port.is_empty() {
                sqlx::query("INSERT OR REPLACE INTO app_config (key, value, updated_at) VALUES ('thermal_printer_port',?,?)").bind(&port).bind(&now).execute(pool).await?;
            }
            if !baud.is_empty() {
                sqlx::query("INSERT OR REPLACE INTO app_config (key, value, updated_at) VALUES ('thermal_printer_baud',?,?)").bind(&baud).bind(&now).execute(pool).await?;
            }
            if !enabled.is_empty() {
                sqlx::query("INSERT OR REPLACE INTO app_config (key, value, updated_at) VALUES ('thermal_printer_enabled',?,?)").bind(&enabled).bind(&now).execute(pool).await?;
            }
            ok_mut(
                "Thermal printer config updated.",
                "thermal_config",
                "global",
            )
        }

        "open_cash_drawer" => {
            let port = cfg(pool, "thermal_printer_port").await.unwrap_or_default();
            if port.is_empty() {
                return Err(AppError::Validation(
                    "No thermal printer port configured — use update_thermal_config first.".into(),
                ));
            }
            let baud: u32 = cfg(pool, "thermal_printer_baud")
                .await
                .unwrap_or("9600".into())
                .parse()
                .unwrap_or(9600);
            let payload = vec![0x1B_u8, b'p', 0x00, 0x19, 0xFA]; // ESC p 0 t1 t2
            tokio::task::spawn_blocking(move || {
                crate::commands::thermal_commands::write_to_port(&port, baud, payload)
            })
            .await
            .map_err(|e| AppError::Internal(format!("Thread error: {e}")))?
            .map_err(|e| AppError::Internal(format!("Drawer error: {e}")))?;
            ok_mut("Cash drawer opened.", "cash_drawer", "local")
        }

        "reprint_receipt" => {
            let receipt = req(input, "receipt_number")?;
            let branch_id = crate::ai::tools::active_branch_id(pool).await?;
            let store_name: String =
                sqlx::query_scalar("SELECT name FROM branches WHERE branch_id=? AND is_active=1")
                    .bind(&branch_id)
                    .fetch_optional(pool)
                    .await?
                    .flatten()
                    .unwrap_or("ZANPOS".into());
            let port = cfg(pool, "thermal_printer_port").await.unwrap_or_default();
            if port.is_empty() {
                return Err(AppError::Validation(
                    "No thermal printer port configured.".into(),
                ));
            }
            let baud: u32 = cfg(pool, "thermal_printer_baud")
                .await
                .unwrap_or("9600".into())
                .parse()
                .unwrap_or(9600);
            let sale = refund_repo::get_sale_by_receipt(pool, &receipt).await?;
            let mut lines = vec![
                format!("REPRINT"),
                format!("Receipt: #{}", sale.receipt_number),
                format!("Date: {}", sale.sold_at),
                format!("Cashier: {}", sale.cashier_name),
            ];
            for item in &sale.items {
                lines.push(format!(
                    "  {} x {} @ {} = {}",
                    item.quantity,
                    item.product_name_snapshot,
                    item.unit_price_minor,
                    item.line_total_minor
                ));
            }
            lines.push(format!("TOTAL: BHD {}", sale.net_total_minor));
            let payload =
                crate::commands::thermal_commands::build_receipt_bytes(&store_name, &lines);
            let port2 = port.clone();
            tokio::task::spawn_blocking(move || {
                crate::commands::thermal_commands::write_to_port(&port2, baud, payload)
            })
            .await
            .map_err(|e| AppError::Internal(format!("Thread error: {e}")))?
            .map_err(|e| AppError::Internal(format!("Print error: {e}")))?;
            ok_mut(
                &format!("Receipt #{receipt} reprinted."),
                "receipt",
                &receipt,
            )
        }

        "delete_held_cart" => {
            let held_cart_id = req(input, "held_cart_id")?;
            held_cart_repo::delete_held_cart(pool, &held_cart_id).await?;
            audit_ext(pool, "held_cart_deleted", &held_cart_id, "{}").await;
            ok_mut(
                &format!("Held cart {held_cart_id} deleted."),
                "held_cart",
                &held_cart_id,
            )
        }

        "update_benefit_number" => {
            let number = req(input, "benefit_number")?;
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("INSERT OR REPLACE INTO app_config (key, value, updated_at) VALUES ('whatsapp_benefit_number',?,?)")
                .bind(&number).bind(&now).execute(pool).await?;
            ok_mut(
                &format!("Benefit/Sadad number updated to {number}."),
                "benefit_number",
                &number,
            )
        }

        other => crate::ai::tools_write_ext2::execute(pool, other, input, currency_exp).await,
    }
}
