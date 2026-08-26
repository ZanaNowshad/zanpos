use crate::errors::{AppError, AppResult};
use serde_json::{Map, Value};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

/// Every table the sync protocol may read or write. The hub rejects any other
/// name.
///
/// Must equal the names in [`crate::sync_v2::registry`], which is asserted by
/// `registry::tests`. It is spelled out here rather than derived only because
/// it is consumed as a slice in twenty places; the test is what makes the two
/// incapable of drifting apart, which is the failure that let ten tables —
/// `sales` and `payments` among them — sync without ever being parity-checked.
pub const SYNC_TABLES: &[&str] = &[
    "branches",
    "roles",
    "users",
    "devices",
    "categories",
    "tax_rules",
    "app_config",
    "products",
    "product_barcodes",
    "product_prices",
    "product_cost_history",
    "stock_levels",
    "stock_movements",
    "customers",
    "loyalty_events",
    "suppliers",
    "riders",
    "purchase_orders",
    "purchase_order_lines",
    "po_receipts",
    "sales",
    "sale_items",
    "payments",
    "refunds",
    "refund_items",
    "cash_events",
    "shifts",
    "delivery_orders",
    "audit_logs",
];

/// app_config keys that are allowed to sync across devices.
pub const ALLOWED_CONFIG_KEYS: &[&str] = &[
    "ai_action_expiry_minutes",
    "ai_anthropic_max_tokens",
    "ai_anthropic_model",
    "ai_bulk_batch_size",
    "ai_connect_timeout_secs",
    "ai_confirm_non_destructive_actions",
    "ai_context_window_chars",
    "ai_enabled",
    "ai_max_turns",
    "ai_openai_max_tokens",
    "ai_provider",
    "ai_sensitive_protection_level",
    "ai_stream_timeout_secs",
    "ai_temperature",
    "feature_compare_prices",
    "feature_customer_insights",
    "feature_insights_engine",
    "feature_inventory_ops",
    "feature_market_price",
    "feature_proactive",
    "feature_smart_analytics",
    "feature_web_fetch",
    "feature_web_search",
    "flag_allow_negative_stock",
    "flag_auto_print_receipt",
    "flag_cashier_can_discount",
    "flag_require_discount_reason",
    "gemini_model",
    "idle_timeout_minutes",
    "loyalty_points_per_bhd",
    "openai_base_url",
    "openai_model",
    "quick_pos_products",
    "reports_device_scope",
    "retention_days_logs",
    "retention_days_sales",
    "storefront_auto_publish",
    "storefront_enabled",
    "storefront_locale",
    "storefront_public_url",
    "storefront_whatsapp_number",
    "sync_interval_hub_secs",
    "sync_interval_terminal_secs",
    "whatsapp_benefit_number",
    "whatsapp_commerce_enabled",
    "whatsapp_group_jid",
    "whatsapp_group_name",
    "whatsapp_owner_jid",
    "whatsapp_owner_name",
];

pub fn is_allowed_config_key(key: &str) -> bool {
    if ALLOWED_CONFIG_KEYS.contains(&key) {
        return true;
    }
    let Some(tool_name) = key.strip_prefix("ai_tool_enabled_") else {
        return false;
    };
    crate::ai::tool_registry::ToolRegistry::global()
        .ok()
        .and_then(|registry| registry.get(tool_name))
        .is_some()
}

pub(crate) const STOCK_DRIFT_TOLERANCE: f64 = 0.001;

/// Apply a single row to the local database, once.
///
/// Every path that receives a row goes through here — the worker's pull, the
/// hub's push handler, and reconciliation's targeted repair — so this is the one
/// place idempotency can be established for all of them. The inbox records the
/// arrival before anything is written and reports a byte-identical redelivery as
/// already handled.
///
/// Applying twice was already safe (last-writer-wins ignores a repeat, and the
/// append-only path reads the unique-constraint violation back). What was
/// missing was any *record* that it happened, so "did this sale ever reach this
/// terminal" had no answer.
pub async fn apply_row(pool: &SqlitePool, table: &str, row: &Value) -> AppResult<()> {
    let obj = row
        .as_object()
        .ok_or_else(|| AppError::Internal("apply_row: row is not a JSON object".into()))?;

    let entity_id = obj
        .get(pk_for_table(table))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // A row without its own primary key cannot be deduplicated or looked up
    // later, so it is applied without a record rather than not applied at all —
    // the existing constraint-based protection still covers it.
    if entity_id.is_empty() {
        return apply_row_inner(pool, table, obj).await;
    }

    if crate::sync_v2::inbox::claim(pool, table, &entity_id, obj).await?
        == crate::sync_v2::inbox::Decision::AlreadySeen
    {
        return Ok(());
    }

    let result = apply_row_inner(pool, table, obj).await;
    if result.is_ok() {
        crate::sync_v2::inbox::confirm(pool, table, &entity_id, obj).await;
    }
    result
}

async fn apply_row_inner(
    pool: &SqlitePool,
    table: &str,
    obj: &Map<String, Value>,
) -> AppResult<()> {
    match table {
        "categories" => apply_lww(pool, "categories", "category_id", obj, &[]).await,
        "tax_rules" => apply_lww(pool, "tax_rules", "tax_rule_id", obj, &[]).await,
        "products" => apply_lww(pool, "products", "product_id", obj, &[]).await,
        "product_barcodes" => apply_lww(pool, "product_barcodes", "barcode", obj, &[]).await,
        "suppliers" => apply_lww(pool, "suppliers", "supplier_id", obj, &[]).await,
        "purchase_orders" => apply_lww(pool, "purchase_orders", "po_id", obj, &[]).await,
        "purchase_order_lines" => {
            apply_lww(pool, "purchase_order_lines", "po_line_id", obj, &[]).await
        }
        "devices" => {
            // Retiring a device is destructive and not idempotent — only a row
            // that is actually news may trigger it.
            let fresh = is_fresh(pool, "devices", "device_id", obj).await?;
            if let (true, Some(device_id), Some(branch_id), Some(device_code)) = (
                fresh,
                obj.get("device_id").and_then(|v| v.as_str()),
                obj.get("branch_id").and_then(|v| v.as_str()),
                obj.get("device_code").and_then(|v| v.as_str()),
            ) {
                let now = chrono::Utc::now().to_rfc3339();
                let _ = sqlx::query(
                    "UPDATE devices
                     SET is_active = 0,
                         deleted_at = ?,
                         device_code = device_code || '-RETIRED-' || substr(device_id, -6),
                         updated_at = ?
                     WHERE branch_id = ? AND device_code = ? AND device_id <> ?",
                )
                .bind(&now)
                .bind(&now)
                .bind(branch_id)
                .bind(device_code)
                .bind(device_id)
                .execute(pool)
                .await;
            }
            let mut obj_norm = obj.clone();
            if !obj_norm.contains_key("created_at")
                || obj_norm.get("created_at") == Some(&Value::Null)
            {
                let fallback = obj_norm
                    .get("updated_at")
                    .cloned()
                    .filter(|v| !matches!(v, Value::Null))
                    .unwrap_or_else(|| Value::String(chrono::Utc::now().to_rfc3339()));
                obj_norm.insert("created_at".to_string(), fallback);
            }
            apply_lww(pool, "devices", "device_id", &obj_norm, &[]).await
        }
        "roles" => apply_lww(pool, "roles", "role_id", obj, &[]).await,
        "branches" => apply_lww(pool, "branches", "branch_id", obj, &[]).await,
        "shifts" => {
            // Closing the other open shift on a till is destructive: a stale
            // 'open' row re-pulled after that shift already ended would close
            // whichever shift is live now, and the till stops taking sales.
            let fresh = is_fresh(pool, "shifts", "shift_id", obj).await?;
            if fresh && obj.get("status").and_then(|v| v.as_str()) == Some("open") {
                if let (Some(shift_id), Some(device_id)) = (
                    obj.get("shift_id").and_then(|v| v.as_str()),
                    obj.get("device_id").and_then(|v| v.as_str()),
                ) {
                    let now = chrono::Utc::now().to_rfc3339();
                    let _ = sqlx::query(
                        "UPDATE shifts SET status = 'closed', updated_at = ?
                         WHERE device_id = ? AND status = 'open' AND shift_id <> ?",
                    )
                    .bind(&now)
                    .bind(device_id)
                    .bind(shift_id)
                    .execute(pool)
                    .await;
                }
            }
            let mut obj_norm = obj.clone();
            if !obj_norm.contains_key("created_at")
                || obj_norm.get("created_at") == Some(&Value::Null)
            {
                let fallback = obj_norm
                    .get("opened_at")
                    .cloned()
                    .filter(|v| !matches!(v, Value::Null))
                    .or_else(|| {
                        obj_norm
                            .get("updated_at")
                            .cloned()
                            .filter(|v| !matches!(v, Value::Null))
                    })
                    .unwrap_or_else(|| Value::String(chrono::Utc::now().to_rfc3339()));
                obj_norm.insert("created_at".to_string(), fallback);
            }
            apply_lww(pool, "shifts", "shift_id", &obj_norm, &[]).await
        }
        "delivery_orders" => apply_lww(pool, "delivery_orders", "delivery_id", obj, &[]).await,
        "stock_levels" => apply_stock_level_seed(pool, obj).await,
        "users" => {
            let user_id = obj.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(username) = obj.get("username").and_then(|v| v.as_str()) {
                let now = chrono::Utc::now().to_rfc3339();
                let _ = sqlx::query(
                    "UPDATE users SET is_active = 0, deleted_at = ? WHERE username = ? AND user_id <> ?",
                )
                .bind(&now)
                .bind(username)
                .bind(user_id)
                .execute(pool)
                .await;
            }

            let mut obj_norm = obj.clone();

            if !obj_norm.contains_key("branch_id") {
                let fallback_branch: Option<String> =
                    sqlx::query_scalar("SELECT branch_id FROM branches WHERE is_active=1 LIMIT 1")
                        .fetch_optional(pool)
                        .await
                        .ok()
                        .flatten();
                let branch =
                    fallback_branch.unwrap_or_else(|| "01JBRANCH0000000000000001".to_string());
                obj_norm.insert("branch_id".to_string(), Value::String(branch));
            }

            let has_remote_pin_hash = obj_norm
                .get("pin_hash")
                .and_then(|v| v.as_str())
                .is_some_and(|s| !s.trim().is_empty());
            let existing_user =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE user_id = ?")
                    .bind(user_id)
                    .fetch_one(pool)
                    .await
                    .map(|n| n > 0)
                    .unwrap_or(false);

            if !has_remote_pin_hash {
                obj_norm.insert(
                    "pin_hash".to_string(),
                    Value::String("*REMOTE-ONLY*".to_string()),
                );
            }

            if let Some(rid) = obj_norm
                .get("role_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
            {
                let role_exists: bool =
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM roles WHERE role_id = ?")
                        .bind(&rid)
                        .fetch_one(pool)
                        .await
                        .map(|n| n > 0)
                        .unwrap_or(false);
                if !role_exists {
                    tracing::warn!(
                        "Sync v2: unknown role_id '{rid}' for remote user — remapping to 'owner'"
                    );
                    obj_norm.insert(
                        "role_id".to_string(),
                        Value::String("01JROLES000000000000000001".to_string()),
                    );
                }
            }

            if has_remote_pin_hash || !existing_user {
                apply_lww(pool, "users", "user_id", &obj_norm, &[]).await
            } else {
                apply_lww(pool, "users", "user_id", &obj_norm, &["pin_hash"]).await
            }
        }
        "customers" => apply_customer(pool, obj).await,
        "sales"
        | "loyalty_events"
        | "sale_items"
        | "payments"
        | "refunds"
        | "refund_items"
        | "stock_movements"
        | "audit_logs"
        | "product_prices"
        | "product_cost_history"
        | "cash_events" => apply_append_only(pool, table, obj).await,
        "app_config" => {
            let key = obj.get("key").and_then(|v| v.as_str()).unwrap_or("");
            if is_allowed_config_key(key) {
                let value = obj.get("value").and_then(|v| v.as_str()).unwrap_or("");
                let remote_ts = obj
                    .get("updated_at")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
                sqlx::query(
                    "INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)
                     ON CONFLICT(key) DO UPDATE SET
                       value      = excluded.value,
                       updated_at = excluded.updated_at
                     WHERE datetime(app_config.updated_at) < datetime(excluded.updated_at)",
                )
                .bind(key)
                .bind(value)
                .bind(&remote_ts)
                .execute(pool)
                .await?;
            }
            Ok(())
        }
        "riders" => apply_lww(pool, "riders", "rider_id", obj, &[]).await,
        "po_receipts" => apply_lww(pool, "po_receipts", "receipt_id", obj, &[]).await,
        other => {
            // Not tolerance for an unknown table — a wiring bug. The worker only
            // applies tables it pulled from its own PULL_ORDER, so arriving here
            // means the table is listed for sync but has no handler, and every
            // row for it was being dropped while the pull reported success.
            // Failing the table surfaces that instead of losing data quietly.
            tracing::error!(
                "Sync v2: no apply handler for table '{other}' — refusing to discard rows"
            );
            Err(AppError::Internal(format!(
                "No sync apply handler for table '{other}'"
            )))
        }
    }
}

/// True when this row is new to us, or newer than the copy we hold.
///
/// `apply_lww` already refuses to overwrite a newer local row, but several
/// tables run *side effects* before that check — retiring a device, closing a
/// shift. Those are not idempotent, so a re-pulled or out-of-order row could
/// retire a device that is in use or close a shift that is still open, which
/// stops the till taking sales. Gate the side effect on the same freshness the
/// write itself is subject to.
async fn is_fresh(
    pool: &SqlitePool,
    table: &str,
    pk: &str,
    obj: &Map<String, Value>,
) -> AppResult<bool> {
    let Some(pk_value) = obj.get(pk).and_then(|v| v.as_str()) else {
        return Ok(false);
    };
    let Some(incoming) = obj.get("updated_at").and_then(|v| v.as_str()) else {
        // No timestamp to compare: treat as fresh only if we hold nothing yet.
        return Ok(!row_exists(pool, table, pk, pk_value).await?);
    };

    let local: Option<String> =
        sqlx::query_scalar(&format!("SELECT updated_at FROM {table} WHERE {pk} = ?"))
            .bind(pk_value)
            .fetch_optional(pool)
            .await?;

    let Some(local) = local else {
        return Ok(true); // never seen — this row is news
    };

    let newer: Option<i64> = sqlx::query_scalar("SELECT datetime(?) < datetime(?)")
        .bind(&local)
        .bind(incoming)
        .fetch_optional(pool)
        .await?;
    Ok(newer.unwrap_or(0) == 1)
}

pub(crate) fn is_safe_col(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The columns of an incoming row that may be written locally.
///
/// One definition, called from all three apply paths. It was copied into each
/// of them, and the copies drifted: a fix to the tombstone rule in `apply_lww`
/// left customers — the table that most needed it — still discarding restores.
///
/// Nulls are skipped so a partial row cannot blank a column the sender never
/// knew about. `deleted_at` is the single exception, because there a null is
/// not an absence but the message itself: it is how a restore is expressed.
pub(crate) fn syncable_columns(obj: &Map<String, Value>) -> Vec<&String> {
    obj.keys()
        .filter(|k| {
            is_safe_col(k)
                && *k != "sync_status"
                && *k != "sync_attempts"
                && (*k == "deleted_at" || !matches!(obj.get(*k), Some(Value::Null)))
        })
        .collect()
}

pub(crate) async fn apply_lww(
    pool: &SqlitePool,
    table: &str,
    pk: &str,
    obj: &Map<String, Value>,
    exclude_cols: &[&str],
) -> AppResult<()> {
    let cols = syncable_columns(obj);

    if cols.is_empty() {
        return Ok(());
    }

    let col_list = format!(
        "{}, sync_status",
        cols.iter()
            .map(|c| c.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let val_list = format!(
        "{}, 'synced'",
        cols.iter()
            .map(|c| json_to_sql_literal(&obj[*c]))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let set_parts: Vec<String> = cols
        .iter()
        .filter(|c| **c != pk && !exclude_cols.contains(&c.as_str()))
        .map(|c| format!("{} = excluded.{}", c, c))
        .collect();

    let set_clause = set_parts.join(", ");
    let has_updated_at = obj.contains_key("updated_at");

    let set_with_sync = if set_clause.is_empty() {
        "sync_status = 'synced'".to_string()
    } else {
        format!("{}, sync_status = 'synced'", set_clause)
    };
    // The edit counter must never go backwards. Taking the incoming value
    // wholesale would let a terminal that has seen fewer edits reset the count,
    // and the next comparison would then read as agreement rather than as the
    // conflict it is.
    let set_with_sync = set_with_sync.replace(
        "version = excluded.version",
        &format!("version = MAX(excluded.version, {table}.version)"),
    );

    record_concurrent_edit(pool, table, pk, obj).await;

    let sql = if has_updated_at && !set_with_sync.is_empty() {
        format!(
            "INSERT INTO {} ({}) VALUES ({})
             ON CONFLICT({}) DO UPDATE SET {}
             WHERE datetime({0}.updated_at) < datetime(excluded.updated_at)",
            table, col_list, val_list, pk, set_with_sync,
        )
    } else {
        format!(
            "INSERT INTO {} ({}) VALUES ({})
             ON CONFLICT({}) DO UPDATE SET {}",
            table, col_list, val_list, pk, set_with_sync,
        )
    };

    sqlx::query(&sql).execute(pool).await?;
    Ok(())
}

pub(crate) async fn apply_customer(pool: &SqlitePool, obj: &Map<String, Value>) -> AppResult<()> {
    let _ = sqlx::query(
        "UPDATE customers SET phone = NULL WHERE phone IS NOT NULL AND TRIM(phone) = ''",
    )
    .execute(pool)
    .await;
    let mut normalized_obj = obj.clone();
    if normalized_obj
        .get("phone")
        .and_then(|v| v.as_str())
        .is_some_and(|phone| phone.trim().is_empty())
    {
        normalized_obj.insert("phone".to_string(), Value::Null);
    }

    if let (Some(incoming_id), Some(phone)) = (
        normalized_obj.get("customer_id").and_then(|v| v.as_str()),
        normalized_obj
            .get("phone")
            .and_then(|v| v.as_str())
            .map(str::trim),
    ) {
        if !phone.is_empty() {
            let existing_id: Option<String> = sqlx::query_scalar(
                "SELECT customer_id FROM customers
                 WHERE phone = ? AND customer_id <> ?
                 LIMIT 1",
            )
            .bind(phone)
            .bind(incoming_id)
            .fetch_optional(pool)
            .await?
            .flatten();

            if let Some(existing_id) = existing_id {
                let mut merged = normalized_obj.clone();
                merged.insert("customer_id".to_string(), Value::String(existing_id));
                return apply_customer_by_primary_key(pool, &merged).await;
            }
        }
    }

    apply_customer_by_primary_key(pool, &normalized_obj).await
}

async fn apply_customer_by_primary_key(
    pool: &SqlitePool,
    obj: &Map<String, Value>,
) -> AppResult<()> {
    let cols = syncable_columns(obj);

    if cols.is_empty() {
        return Ok(());
    }

    let col_list = format!(
        "{}, sync_status",
        cols.iter()
            .map(|c| c.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let val_list = format!(
        "{}, 'synced'",
        cols.iter()
            .map(|c| json_to_sql_literal(&obj[*c]))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let set_parts: Vec<String> = cols
        .iter()
        .filter(|c| **c != "customer_id")
        .map(|c| {
            if c.as_str() == "loyalty_points" {
                format!(
                    "{} = MAX(customers.loyalty_points, excluded.loyalty_points)",
                    c
                )
            } else {
                format!("{} = excluded.{}", c, c)
            }
        })
        .collect();

    let set_clause = format!("{}, sync_status = 'synced'", set_parts.join(", "));
    let has_updated_at = obj.contains_key("updated_at");

    let sql = if has_updated_at && !set_clause.is_empty() {
        format!(
            "INSERT INTO customers ({}) VALUES ({})
             ON CONFLICT(customer_id) DO UPDATE SET {}
             WHERE datetime(customers.updated_at) < datetime(excluded.updated_at)",
            col_list, val_list, set_clause,
        )
    } else {
        format!(
            "INSERT INTO customers ({}) VALUES ({})
             ON CONFLICT(customer_id) DO UPDATE SET {}",
            col_list, val_list, set_clause,
        )
    };

    sqlx::query(&sql).execute(pool).await?;
    Ok(())
}

/// True when the insert failed because a UNIQUE or PRIMARY KEY index rejected it.
///
/// Matched on the message rather than the extended result code: the code is not
/// consistently surfaced through the driver, and a missed match here would turn
/// a recoverable collision into a hard sync failure.
fn is_unique_violation(e: &sqlx::Error) -> bool {
    let msg = e.to_string();
    msg.contains("UNIQUE constraint failed") || msg.contains("PRIMARY KEY constraint failed")
}

async fn row_exists(pool: &SqlitePool, table: &str, pk: &str, pk_value: &str) -> AppResult<bool> {
    if pk_value.is_empty() {
        return Ok(false);
    }
    let found: Option<i64> =
        sqlx::query_scalar(&format!("SELECT 1 FROM {table} WHERE {pk} = ? LIMIT 1"))
            .bind(pk_value)
            .fetch_optional(pool)
            .await?;
    Ok(found.is_some())
}

/// Record an append-only row that could not be stored.
///
/// Severity is critical because the alternative to noticing is losing a
/// financial record: the originating terminal has already marked the row synced,
/// so nothing will re-offer it.
/// Notice when two terminals edited the same row independently.
///
/// Last-writer-wins compares clocks, and a clock says nothing about whether the
/// winner ever saw what it is overwriting. `version` does: every local edit
/// increments it, so a row arriving with a *later* `updated_at` but no more
/// edits than the local copy was not built on top of the local copy — the two
/// were written in parallel and one is about to be discarded silently.
///
/// This only reports. The LWW outcome is unchanged, because changing what wins
/// is a separate decision from being able to see that a decision was made — and
/// [`crate::sync_v2::reconcile`] already refuses to auto-repair anything
/// genuinely contested. A shop needs the record before it needs the policy.
///
/// Costs one indexed lookup per LWW row, and only for rows that carry a version.
/// The append-only tables — every financial one — do not come through here.
async fn record_concurrent_edit(
    pool: &SqlitePool,
    table: &str,
    pk: &str,
    obj: &Map<String, Value>,
) {
    let (Some(incoming_version), Some(incoming_at), Some(entity_id)) = (
        obj.get("version").and_then(Value::as_i64),
        obj.get("updated_at").and_then(Value::as_str),
        obj.get(pk).and_then(Value::as_str),
    ) else {
        return;
    };

    let local: Option<(i64, String)> = sqlx::query_as(&format!(
        "SELECT COALESCE(version, 0), COALESCE(updated_at, '') FROM {table} WHERE {pk} = ?"
    ))
    .bind(entity_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let Some((local_version, local_at)) = local else {
        return; // A row we do not hold cannot have been edited here.
    };

    // Only rows that are about to win are interesting; a stale arrival is
    // discarded by the freshness guard and overwrites nothing.
    if incoming_at <= local_at.as_str() || incoming_version > local_version {
        return;
    }

    tracing::warn!(
        table,
        entity_id,
        local_version,
        incoming_version,
        "Sync: concurrent edit — a newer row with no more edits is overwriting local changes"
    );
    let _ = sqlx::query(
        "INSERT INTO sync_conflicts
           (conflict_id, conflict_type, table_name, entity_id, severity, title, detail,
            status, created_at)
         VALUES (?, 'concurrent_edit', ?, ?, 'warning', ?, ?, 'open', ?)",
    )
    .bind(ulid::Ulid::new().to_string())
    .bind(table)
    .bind(entity_id)
    .bind(format!("Concurrent edit to {table}"))
    .bind(format!(
        "Another terminal's copy (version {incoming_version}, {incoming_at}) replaced this \
         one (version {local_version}, {local_at}). It carries no more edits than the copy \
         it replaced, so both were probably edited at the same time and one set of changes \
         has been discarded."
    ))
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await;
}

async fn record_append_conflict(
    pool: &SqlitePool,
    table: &str,
    entity_id: &str,
    detail: &str,
) -> AppResult<()> {
    tracing::error!(
        table,
        entity_id,
        "Sync: append-only row rejected by a unique index — recorded as a conflict"
    );
    sqlx::query(
        "INSERT INTO sync_conflicts
           (conflict_id, conflict_type, table_name, entity_id, severity, title, detail,
            status, created_at)
         VALUES (?, 'unique_collision', ?, ?, 'critical', ?, ?, 'open', ?)",
    )
    .bind(ulid::Ulid::new().to_string())
    .bind(table)
    .bind(entity_id)
    .bind(format!("Rejected {table} row from another terminal"))
    .bind(detail)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn apply_append_only(
    pool: &SqlitePool,
    table: &str,
    obj: &Map<String, Value>,
) -> AppResult<()> {
    let cols = syncable_columns(obj);

    if cols.is_empty() {
        return Ok(());
    }

    let col_list = cols
        .iter()
        .map(|c| c.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let val_list = cols
        .iter()
        .map(|c| json_to_sql_literal(&obj[*c]))
        .collect::<Vec<_>>()
        .join(", ");

    // Deliberately not INSERT OR IGNORE. These tables are append-only business
    // records, and OR IGNORE cannot tell a harmless re-delivery of a row we
    // already hold from a genuine collision — two terminals minting the same
    // receipt_number or idempotency_key. It swallowed both, and the pull was
    // still reported successful, so a lost sale left no trace anywhere.
    let sql = format!(
        "INSERT INTO {} ({}, sync_status) VALUES ({}, 'synced')",
        table, col_list, val_list,
    );

    if let Err(e) = sqlx::query(&sql).execute(pool).await {
        if !is_unique_violation(&e) {
            return Err(e.into());
        }
        let pk = pk_for_table(table);
        let pk_value = obj.get(pk).and_then(|v| v.as_str()).unwrap_or_default();

        // Same primary key already present: this is the idempotent replay the
        // sync protocol is built on. Nothing is lost, nothing to report.
        if !row_exists(pool, table, pk, pk_value).await? {
            // A *different* row already holds one of this row's unique keys.
            // The incoming record cannot be stored and would otherwise vanish.
            record_append_conflict(pool, table, pk_value, &e.to_string()).await?;
        }
    }

    if table == "stock_movements" {
        if let (Some(product_id), Some(branch_id), Some(created_at)) = (
            obj.get("product_id").and_then(|v| v.as_str()),
            obj.get("branch_id").and_then(|v| v.as_str()),
            obj.get("created_at").and_then(|v| v.as_str()),
        ) {
            let _ = recompute_stock_level(pool, product_id, branch_id, created_at).await;
        }
    }

    // A loyalty event arriving from another till changes what this one should
    // show for that customer. Same shape as stock: the ledger moved, so the
    // cached figure is recomputed from it rather than trusted from the wire.
    if table == "loyalty_events" {
        if let Some(customer_id) = obj.get("customer_id").and_then(|v| v.as_str()) {
            let _ = crate::db::repositories::loyalty_repo::recompute(pool, customer_id).await;
        }
    }

    Ok(())
}

/// Whether a table has an `origin_device_id` column.
/// Pull queries add a `neq` filter for tables that have this column.
pub fn has_origin_device_id(table: &str) -> bool {
    matches!(
        table,
        "shifts"
            | "sales"
            | "sale_items"
            | "payments"
            | "refunds"
            | "refund_items"
            | "stock_movements"
            | "audit_logs"
            | "delivery_orders"
            | "cash_events"
            | "loyalty_events"
    )
}

/// Return the primary key column name for a table.
pub fn pk_for_table(table: &str) -> &str {
    match table {
        "branches" => "branch_id",
        "stock_levels" => "stock_level_id",
        "categories" => "category_id",
        "tax_rules" => "tax_rule_id",
        "products" => "product_id",
        "product_barcodes" => "barcode",
        "suppliers" => "supplier_id",
        "purchase_orders" => "po_id",
        "purchase_order_lines" => "po_line_id",
        "po_receipts" => "receipt_id",
        "riders" => "rider_id",
        "devices" => "device_id",
        "roles" => "role_id",
        "customers" => "customer_id",
        "loyalty_events" => "loyalty_event_id",
        "shifts" => "shift_id",
        "sales" => "sale_id",
        "sale_items" => "sale_item_id",
        "payments" => "payment_id",
        "refunds" => "refund_id",
        "refund_items" => "refund_item_id",
        "stock_movements" => "movement_id",
        "audit_logs" => "audit_log_id",
        "delivery_orders" => "delivery_id",
        "product_prices" => "price_id",
        "product_cost_history" => "cost_history_id",
        "users" => "user_id",
        "cash_events" => "cash_event_id",
        "app_config" => "key",
        _ => "id",
    }
}

/// Columns omitted from JSON sent to or pulled from the hub.
///
/// Split from [`skip_in_fingerprint`] deliberately. One function used to serve
/// both purposes, and that coupling is what made the `deleted_at` bug
/// undetectable: the column was stripped from every payload *and* from the
/// parity checksum, so the terminals disagreed and no check could see it.
///
/// Keeping them separate means a column can travel without moving every row's
/// fingerprint — which matters, because changing a fingerprint makes the whole
/// fleet report divergence until the last terminal upgrades.
pub fn skip_on_wire(table: &str, col_name: &str) -> bool {
    // `version` travels, even though it is per-device bookkeeping in every other
    // respect. Every local edit does `version = version + 1` and nothing ever
    // reads it back to reject a write, so it is a pure count of how many edits a
    // row has accumulated — and that is exactly the signal missing from
    // last-writer-wins. Two terminals that edited independently produce a row
    // that is newer by the clock but has seen no more edits, which is
    // distinguishable from an ordinary stale write only if the count crosses.
    //
    // It stays out of `skip_in_fingerprint`, so no checksum moves and no
    // terminal reports false divergence while the fleet upgrades.
    if col_name == "version" {
        return false;
    }
    // Derived figures never travel. Each terminal computes them from the ledger
    // underneath, so accepting another terminal's copy can only overwrite a
    // correct local total with one derived from a different subset of events —
    // the same defect `stock_levels` had, and for the same reason.
    if derived_locally(table, col_name) {
        return true;
    }
    per_device_bookkeeping(table, col_name)
}

/// Columns that are a consequence of another table rather than a fact of their
/// own, and so are neither sent nor compared.
fn derived_locally(table: &str, col_name: &str) -> bool {
    matches!((table, col_name), ("customers", "loyalty_points"))
}

/// Columns excluded from the parity fingerprint.
///
/// Deliberately *not* the same question as [`skip_on_wire`]. This one decides
/// what two terminals must agree about; that one decides what crosses the
/// network. Per-device bookkeeping answers both today, and any future
/// divergence between them belongs here rather than at a call site.
pub fn skip_in_fingerprint(table: &str, col_name: &str) -> bool {
    // A derived figure is in flux whenever events are mid-flight, so comparing
    // it reports divergence that is timing rather than disagreement. The ledger
    // it comes from is parity-checked instead, which is the thing that actually
    // has to match.
    derived_locally(table, col_name) || per_device_bookkeeping(table, col_name)
}

/// State that belongs to one terminal and means nothing on another.
fn per_device_bookkeeping(table: &str, col_name: &str) -> bool {
    // `deleted_at` is deliberately NOT here. It used to be, alongside the
    // per-device bookkeeping columns, and the consequence was that a deletion
    // never crossed the wire at all: stripped from every push and from every
    // pull. Products survived that because `soft_delete_product` also clears
    // `is_active`, which does sync and which every catalogue read filters on.
    // `customers` and `shifts` have no such flag, so a row deleted on one
    // terminal stayed live on the others forever — and, because the parity
    // checksum used this same function, invisibly so.
    if col_name == "sync_status" || col_name == "sync_attempts" || col_name == "version" {
        return true;
    }
    matches!(
        (table, col_name),
        (
            "users",
            "failed_pin_attempts" | "locked_until" | "last_login_at"
        ) | ("devices", "next_receipt_seq" | "last_seen_at" | "version")
            | ("customers", "origin_device_id" | "version")
            | (
                "shifts",
                "expected_cash_minor"
                    | "cash_difference_minor"
                    | "business_date"
                    | "created_at"
                    | "version"
            )
            | ("audit_logs", "override_used")
    )
}

/// Extract a typed value from a sqlx Row column by name.
/// Returns Value::Null for missing or null columns.
pub fn value_from_row_column(row: &SqliteRow, col: &str) -> Value {
    if let Ok(v) = row.try_get::<Option<i64>, _>(col) {
        return match v {
            Some(n) => Value::Number(n.into()),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(col) {
        return match v {
            Some(n) => {
                if let Some(num) = serde_json::Number::from_f64(n) {
                    Value::Number(num)
                } else {
                    Value::Null
                }
            }
            None => Value::Null,
        };
    }
    if let Ok(v) = row.try_get::<i64, _>(col) {
        return Value::Number(v.into());
    }
    if let Ok(v) = row.try_get::<f64, _>(col) {
        if let Some(n) = serde_json::Number::from_f64(v) {
            return Value::Number(n);
        }
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(col) {
        return match v {
            Some(s) if !s.is_empty() => Value::String(s),
            _ => Value::Null,
        };
    }
    if let Ok(v) = row.try_get::<String, _>(col) {
        if v.is_empty() {
            return Value::Null;
        }
        return Value::String(v);
    }
    Value::Null
}

/// Convert a serde_json Value to a SQLite-safe literal string for embedding in SQL.
pub(crate) fn json_to_sql_literal(v: &Value) -> String {
    match v {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => (if *b { "1" } else { "0" }).to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        Value::Array(_) | Value::Object(_) => {
            let s = serde_json::to_string(v).unwrap_or_default();
            format!("'{}'", s.replace('\'', "''"))
        }
    }
}

/// Recompute stock_levels quantity_on_hand from the stock_movements ledger
/// for a given (product, branch). Called after applying a remote movement.
/// Authoritative on-hand quantity for a product from the movement ledger.
///
/// The balance is the oldest *surviving* movement's own post-state plus every
/// delta recorded after it — not the newest movement's `quantity_after`.
///
/// `quantity_after` is a snapshot of what the writing terminal believed at the
/// time, computed from its own cache. When two terminals sell the same last
/// unit while offline they both record `quantity_after = 9`, and taking the
/// newest leaves stock at 9 when the truth is 8; the deltas (-1 and -1) are the
/// only part of those rows that composes correctly.
///
/// It is anchored rather than a plain `SUM` because `stock_movements` is pruned
/// once rows are synced (`worker::prune_old_data`), so summing every surviving
/// delta would silently discard all pruned history.
///
/// Returns `None` when no movements survive for this product.
/// Accept a `stock_levels` row only as an opening balance, never as an overwrite.
///
/// Stock has one source of truth — the `stock_movements` ledger — and
/// [`recompute_stock_level`] derives the cached figure from it. But that derived
/// row is itself synced, and it used to arrive here through `apply_lww`. So a
/// cached value computed on another till, from *that* till's subset of
/// movements, could overwrite a correct local figure purely by carrying a later
/// `updated_at`. It corrected itself only when the next movement for that
/// product arrived and triggered a recompute; for a slow-moving line that is
/// weeks of a wrong number on screen.
///
/// The row still has to cross the wire, because a freshly onboarded terminal has
/// no movements yet and would otherwise show zero stock for everything. That is
/// why `stock_levels` sits before `stock_movements` in the worker's pull order.
/// So the rule is not "never accept" but "never overwrite":
///
/// * no movements held for this product and branch → nothing to contradict the
///   incoming figure, take it as the opening balance
/// * any movements held → the ledger is authoritative, discard the incoming row
///
/// [`ledger_balance`] already returns `None` for the first case, so the seed
/// condition needs no query of its own.
async fn apply_stock_level_seed(pool: &SqlitePool, obj: &Map<String, Value>) -> AppResult<()> {
    let (Some(product_id), Some(branch_id)) = (
        obj.get("product_id").and_then(|v| v.as_str()),
        obj.get("branch_id").and_then(|v| v.as_str()),
    ) else {
        // Without both identifiers the ledger cannot be consulted, so authority
        // cannot be established. Declining to write is the safe direction: the
        // cost is a missing opening balance, not a corrupted stock figure.
        tracing::warn!("stock_levels row without product_id/branch_id ignored");
        return Ok(());
    };

    if ledger_balance(pool, product_id, branch_id).await?.is_some() {
        return Ok(());
    }
    apply_lww(pool, "stock_levels", "stock_level_id", obj, &[]).await
}

async fn ledger_balance(
    pool: &SqlitePool,
    product_id: &str,
    branch_id: &str,
) -> AppResult<Option<f64>> {
    let anchor: Option<(f64, String, i64)> = sqlx::query_as(
        "SELECT CAST(quantity_after AS REAL), created_at, rowid
           FROM stock_movements
          WHERE product_id = ? AND branch_id = ?
          ORDER BY datetime(created_at) ASC, rowid ASC
          LIMIT 1",
    )
    .bind(product_id)
    .bind(branch_id)
    .fetch_optional(pool)
    .await?;

    let Some((anchor_after, anchor_at, anchor_rowid)) = anchor else {
        return Ok(None);
    };

    let delta_sum: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(CAST(quantity_delta AS REAL)), 0.0)
           FROM stock_movements
          WHERE product_id = ? AND branch_id = ?
            AND (datetime(created_at) > datetime(?)
                 OR (datetime(created_at) = datetime(?) AND rowid > ?))",
    )
    .bind(product_id)
    .bind(branch_id)
    .bind(&anchor_at)
    .bind(&anchor_at)
    .bind(anchor_rowid)
    .fetch_one(pool)
    .await?;

    Ok(Some(anchor_after + delta_sum))
}

pub(crate) async fn recompute_stock_level(
    pool: &SqlitePool,
    product_id: &str,
    branch_id: &str,
    applied_at: &str,
) -> AppResult<()> {
    let Some(ledger) = ledger_balance(pool, product_id, branch_id).await? else {
        // No surviving movements to recompute from — leave the cached level as
        // it is rather than resetting a real stock figure to zero.
        return Ok(());
    };

    let stock_level_id = format!("SL-{}-{}", product_id, branch_id);
    let qty_str = format!("{:.3}", ledger)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string();

    sqlx::query(
        "INSERT INTO stock_levels
            (stock_level_id, product_id, branch_id, quantity_on_hand, last_movement_at, created_at, updated_at, sync_status)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'pending')
         ON CONFLICT(product_id, branch_id) DO UPDATE SET
            quantity_on_hand = excluded.quantity_on_hand,
            last_movement_at = excluded.last_movement_at,
            updated_at       = excluded.updated_at,
            sync_status      = 'pending'",
    )
    .bind(&stock_level_id)
    .bind(product_id)
    .bind(branch_id)
    .bind(&qty_str)
    .bind(applied_at)
    .bind(applied_at)
    .bind(applied_at)
    .execute(pool)
    .await?;

    let cached: Option<f64> = sqlx::query_scalar(
        "SELECT CAST(quantity_on_hand AS REAL) FROM stock_levels
         WHERE product_id = ? AND branch_id = ?",
    )
    .bind(product_id)
    .bind(branch_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    if let Some(c) = cached {
        if (ledger - c).abs() > STOCK_DRIFT_TOLERANCE {
            tracing::warn!(
                "stock drift: product={product_id} branch={branch_id} ledger={ledger} cached={c}"
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn test_pool() -> SqlitePool {
        let path = std::env::temp_dir().join(format!("zanpos_apply_{}.db", ulid::Ulid::new()));
        let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", path.display()))
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    // ── stock_levels is a seed, never an overwrite ───────────────────────────
    //
    // The defect these cover: `stock_levels` is derived from the movement ledger
    // *and* synced, so a cached figure computed on another till could overwrite
    // a correct local one just by carrying a later timestamp — and stay wrong
    // until that product next moved.

    async fn stocked_pool() -> SqlitePool {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO categories (category_id, name, created_at, updated_at)
             VALUES ('cat_1','Grocery','2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO products (product_id, category_id, name, is_active, created_at, updated_at)
             VALUES ('prd_1','cat_1','Rice 5kg',1,'2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    /// A movement this terminal actually holds, so the ledger is non-empty.
    async fn add_movement(pool: &SqlitePool, id: &str, delta: &str, after: &str, at: &str) {
        sqlx::query(
            "INSERT INTO stock_movements (movement_id, product_id, branch_id, device_id,
                 movement_type, quantity_delta, quantity_after, created_at)
             VALUES (?, 'prd_1', 'br_1', 'dev_local', 'adjustment', ?, ?, ?)",
        )
        .bind(id)
        .bind(delta)
        .bind(after)
        .bind(at)
        .execute(pool)
        .await
        .unwrap();
    }

    fn remote_level(qty: &str, updated_at: &str) -> Value {
        json!({
            "stock_level_id": "SL-prd_1-br_1",
            "product_id": "prd_1",
            "branch_id": "br_1",
            "quantity_on_hand": qty,
            "last_movement_at": updated_at,
            "created_at": "2026-08-01T00:00:00Z",
            "updated_at": updated_at,
        })
    }

    async fn level_of(pool: &SqlitePool) -> Option<String> {
        sqlx::query_scalar("SELECT quantity_on_hand FROM stock_levels WHERE product_id='prd_1'")
            .fetch_optional(pool)
            .await
            .unwrap()
    }

    /// The reported defect. A newer remote cache must not win over a local
    /// ledger — "newer" says nothing about which subset of movements it saw.
    #[tokio::test]
    async fn a_terminal_holding_movements_ignores_a_newer_remote_stock_level() {
        let pool = stocked_pool().await;
        add_movement(&pool, "mv_1", "40", "40", "2026-08-02T09:00:00Z").await;
        recompute_stock_level(&pool, "prd_1", "br_1", "2026-08-02T09:00:00Z")
            .await
            .unwrap();
        assert_eq!(level_of(&pool).await.as_deref(), Some("40"));

        // Far in the future, so nothing but authority can decide this.
        apply_row(&pool, "stock_levels", &remote_level("999", "2030-01-01T00:00:00Z"))
            .await
            .unwrap();

        assert_eq!(
            level_of(&pool).await.as_deref(),
            Some("40"),
            "a remote cache overwrote a ledger-derived figure"
        );
    }

    /// The other half: without this, a freshly onboarded till shows zero stock
    /// for every product that has not moved since it joined.
    #[tokio::test]
    async fn a_terminal_with_no_movements_accepts_the_row_as_an_opening_balance() {
        let pool = stocked_pool().await;

        apply_row(&pool, "stock_levels", &remote_level("25", "2026-08-02T09:00:00Z"))
            .await
            .unwrap();

        assert_eq!(level_of(&pool).await.as_deref(), Some("25"));
    }

    /// Seeding must not queue the foreign figure straight back out; the row it
    /// wrote came from the hub and is already agreed.
    #[tokio::test]
    async fn an_accepted_seed_is_not_queued_for_push() {
        let pool = stocked_pool().await;
        apply_row(&pool, "stock_levels", &remote_level("25", "2026-08-02T09:00:00Z"))
            .await
            .unwrap();

        let status: String =
            sqlx::query_scalar("SELECT sync_status FROM stock_levels WHERE product_id='prd_1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "synced");
    }

    /// Once seeded, the ledger takes over: a movement recomputes from local
    /// evidence and the seed stops being authoritative.
    #[tokio::test]
    async fn a_movement_arriving_after_a_seed_recomputes_from_the_ledger() {
        let pool = stocked_pool().await;
        apply_row(&pool, "stock_levels", &remote_level("25", "2026-08-02T09:00:00Z"))
            .await
            .unwrap();

        // A movement whose own snapshot says 30 — the ledger anchor.
        add_movement(&pool, "mv_1", "5", "30", "2026-08-03T09:00:00Z").await;
        recompute_stock_level(&pool, "prd_1", "br_1", "2026-08-03T09:00:00Z")
            .await
            .unwrap();
        assert_eq!(level_of(&pool).await.as_deref(), Some("30"));

        // And from here the seed can no longer come back and undo it.
        apply_row(&pool, "stock_levels", &remote_level("25", "2031-01-01T00:00:00Z"))
            .await
            .unwrap();
        assert_eq!(level_of(&pool).await.as_deref(), Some("30"));
    }

    /// Authority cannot be established without both identifiers, so the safe
    /// direction is to write nothing rather than guess.
    #[tokio::test]
    async fn a_stock_level_row_missing_its_identifiers_is_not_written() {
        let pool = stocked_pool().await;
        let malformed = json!({
            "stock_level_id": "SL-prd_1-br_1",
            "quantity_on_hand": "999",
            "created_at": "2026-08-01T00:00:00Z",
            "updated_at": "2030-01-01T00:00:00Z",
        });

        apply_row(&pool, "stock_levels", &malformed).await.unwrap();
        assert_eq!(level_of(&pool).await, None);
    }

    // ── concurrent edits are visible, not silent ─────────────────────────────

    async fn conflicts(pool: &SqlitePool) -> Vec<(String, String)> {
        sqlx::query_as("SELECT conflict_type, entity_id FROM sync_conflicts ORDER BY created_at")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    fn category(name: &str, version: i64, updated_at: &str) -> Value {
        json!({
            "category_id": "cat_1",
            "name": name,
            "created_at": "2026-08-01T00:00:00Z",
            "updated_at": updated_at,
            "version": version,
        })
    }

    /// `version` has to cross the wire for any of this to work, but it must stay
    /// out of the fingerprint — carrying it there would change every row's
    /// checksum at once and make the whole fleet report divergence.
    #[test]
    fn version_travels_but_is_never_fingerprinted() {
        for table in ["products", "customers", "devices", "shifts"] {
            assert!(!skip_on_wire(table, "version"), "{table}");
            assert!(
                skip_in_fingerprint(table, "version"),
                "{table} would move every checksum"
            );
        }
    }

    /// A newer row that has seen no more edits than the one it replaces was
    /// written in parallel, not on top. That is the case worth reporting.
    #[tokio::test]
    async fn a_newer_row_with_no_more_edits_is_recorded_as_a_concurrent_edit() {
        let pool = test_pool().await;
        apply_row(&pool, "categories", &category("Grocery", 5, "2026-08-01T10:00:00Z"))
            .await
            .unwrap();

        // Later by the clock, but only 3 edits deep against our 5.
        apply_row(&pool, "categories", &category("Produce", 3, "2026-08-01T11:00:00Z"))
            .await
            .unwrap();

        let found = conflicts(&pool).await;
        assert_eq!(found, vec![("concurrent_edit".into(), "cat_1".into())]);

        // Detection only: the LWW outcome is deliberately unchanged.
        let name: String =
            sqlx::query_scalar("SELECT name FROM categories WHERE category_id='cat_1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(name, "Produce");
    }

    /// An ordinary edit built on top of ours carries a higher count. Reporting
    /// that would bury the real conflicts in noise.
    #[tokio::test]
    async fn an_edit_built_on_top_of_ours_is_not_a_conflict() {
        let pool = test_pool().await;
        apply_row(&pool, "categories", &category("Grocery", 5, "2026-08-01T10:00:00Z"))
            .await
            .unwrap();
        apply_row(&pool, "categories", &category("Produce", 6, "2026-08-01T11:00:00Z"))
            .await
            .unwrap();

        assert!(conflicts(&pool).await.is_empty());
    }

    /// A stale arrival overwrites nothing, so it is not a conflict either.
    #[tokio::test]
    async fn a_stale_row_that_loses_is_not_reported() {
        let pool = test_pool().await;
        apply_row(&pool, "categories", &category("Grocery", 5, "2026-08-01T10:00:00Z"))
            .await
            .unwrap();
        apply_row(&pool, "categories", &category("Old", 2, "2026-07-01T09:00:00Z"))
            .await
            .unwrap();

        assert!(conflicts(&pool).await.is_empty());
        let name: String =
            sqlx::query_scalar("SELECT name FROM categories WHERE category_id='cat_1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(name, "Grocery", "the freshness guard stopped working");
    }

    /// The counter must not go backwards, or the next comparison reads a
    /// conflict as agreement.
    #[tokio::test]
    async fn the_edit_counter_never_moves_backwards() {
        let pool = test_pool().await;
        apply_row(&pool, "categories", &category("Grocery", 9, "2026-08-01T10:00:00Z"))
            .await
            .unwrap();
        apply_row(&pool, "categories", &category("Produce", 3, "2026-08-01T11:00:00Z"))
            .await
            .unwrap();

        let version: i64 =
            sqlx::query_scalar("SELECT version FROM categories WHERE category_id='cat_1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(version, 9, "a lower incoming version reset the edit count");
    }

    /// A row this terminal has never held cannot have been edited here.
    #[tokio::test]
    async fn a_first_delivery_is_not_a_concurrent_edit() {
        let pool = test_pool().await;
        apply_row(&pool, "categories", &category("Grocery", 1, "2026-08-01T10:00:00Z"))
            .await
            .unwrap();

        assert!(conflicts(&pool).await.is_empty());
    }

    #[tokio::test]
    async fn lww_newer_wins_older_loses() {
        let pool = test_pool().await;
        // The hub always sends full rows — all NOT NULL columns must be present
        // because SQLite checks NOT NULL before ON CONFLICT evaluation.
        let base = json!({"category_id":"C1","name":"Old","sort_order":0,"is_active":1,
            "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"});
        apply_lww(
            &pool,
            "categories",
            "category_id",
            base.as_object().unwrap(),
            &[],
        )
        .await
        .unwrap();
        let newer = json!({"category_id":"C1","name":"New","sort_order":0,"is_active":1,
            "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"});
        apply_lww(
            &pool,
            "categories",
            "category_id",
            newer.as_object().unwrap(),
            &[],
        )
        .await
        .unwrap();
        let n: String = sqlx::query_scalar("SELECT name FROM categories WHERE category_id='C1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, "New");
        let older = json!({"category_id":"C1","name":"Stale","sort_order":0,"is_active":1,
            "created_at":"2026-01-01T00:00:00Z","updated_at":"2025-12-31T00:00:00Z"});
        apply_lww(
            &pool,
            "categories",
            "category_id",
            older.as_object().unwrap(),
            &[],
        )
        .await
        .unwrap();
        let n: String = sqlx::query_scalar("SELECT name FROM categories WHERE category_id='C1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, "New", "older update must not overwrite");
    }

    #[tokio::test]
    async fn append_only_is_idempotent() {
        let pool = test_pool().await;
        let branch_id: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        let device_id: String =
            sqlx::query_scalar("SELECT device_id FROM devices WHERE is_active=1 LIMIT 1")
                .fetch_optional(&pool)
                .await
                .unwrap()
                .unwrap_or_else(|| "01JDEVICE0000000000000001".to_string());
        let shift_id = ulid::Ulid::new().to_string();
        sqlx::query("INSERT INTO shifts (shift_id, branch_id, device_id, cashier_user_id, status, opened_at, created_at, updated_at) VALUES (?,?,?,?,'open',datetime('now'),datetime('now'),datetime('now'))")
            .bind(&shift_id).bind(&branch_id).bind(&device_id).bind("01JUSER000000000000ADMIN1")
            .execute(&pool).await.unwrap();
        let sale = json!({"sale_id":"S1","branch_id":branch_id,
            "device_id":device_id,"origin_device_id":device_id,
            "receipt_number":"R-1","shift_id":shift_id,"cashier_user_id":"01JUSER000000000000ADMIN1",
            "status":"completed","gross_total_minor":100,"discount_total_minor":0,
            "tax_total_minor":0,"net_total_minor":100,"sold_at":"2026-01-01T00:00:00Z",
            "business_date":"2026-01-01","idempotency_key":"S1-ik",
            "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"});
        apply_row(&pool, "sales", &sale).await.unwrap();
        apply_row(&pool, "sales", &sale).await.unwrap();
        let c: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sales WHERE sale_id='S1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(c, 1);
    }

    #[tokio::test]
    async fn users_pin_hash_syncs_when_remote_sends_hash() {
        let pool = test_pool().await;
        let uid: String = sqlx::query_scalar("SELECT user_id FROM users LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        let upd = json!({"user_id":uid,"display_name":"Renamed","username":"renamed-unique-x",
            "pin_hash":"$argon2id$v=19$m=19456,t=2,p=1$remote$hash",
            "role_id":"01JROLES000000000000000001","is_active":1,"created_at":"2026-01-01T00:00:00Z",
            "updated_at":"2030-01-01T00:00:00Z"});
        apply_row(&pool, "users", &upd).await.unwrap();
        let after: String = sqlx::query_scalar("SELECT pin_hash FROM users WHERE user_id=?")
            .bind(&uid)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            after, "$argon2id$v=19$m=19456,t=2,p=1$remote$hash",
            "remote hashed PIN should sync so credentials match across terminals"
        );
    }

    #[tokio::test]
    async fn device_pull_retires_a_local_code_collision_before_insert() {
        let pool = test_pool().await;
        let local = sqlx::query("SELECT device_id, branch_id, device_code FROM devices LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        let local_id: String = local.get("device_id");
        let branch_id: String = local.get("branch_id");
        let device_code: String = local.get("device_code");
        let remote = json!({
            "device_id": "REMOTE-DEVICE",
            "branch_id": branch_id,
            "device_code": device_code,
            "name": "Hub Canonical Device",
            "status": "online",
            "is_active": 1,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2030-01-01T00:00:00Z"
        });

        apply_row(&pool, "devices", &remote).await.unwrap();

        let remote_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE device_id='REMOTE-DEVICE'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let retired_code: String =
            sqlx::query_scalar("SELECT device_code FROM devices WHERE device_id=?")
                .bind(local_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(remote_count, 1);
        assert_ne!(retired_code, device_code);
    }

    #[tokio::test]
    async fn users_missing_remote_pin_hash_does_not_clobber_existing_hash() {
        let pool = test_pool().await;
        let uid: String = sqlx::query_scalar("SELECT user_id FROM users LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        let before: String = sqlx::query_scalar("SELECT pin_hash FROM users WHERE user_id=?")
            .bind(&uid)
            .fetch_one(&pool)
            .await
            .unwrap();
        let upd = json!({"user_id":uid,"display_name":"Legacy Remote","username":"legacy-remote-x",
            "role_id":"01JROLES000000000000000001","is_active":1,"created_at":"2026-01-01T00:00:00Z",
            "updated_at":"2030-01-01T00:00:00Z"});
        apply_row(&pool, "users", &upd).await.unwrap();
        let after: String = sqlx::query_scalar("SELECT pin_hash FROM users WHERE user_id=?")
            .bind(&uid)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            before, after,
            "legacy remote user rows without a hash must not overwrite local credentials"
        );
    }

    #[tokio::test]
    async fn customer_pull_merges_duplicate_phone_instead_of_blocking_sync() {
        let pool = test_pool().await;
        let branch_id: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO customers
             (customer_id, branch_id, origin_device_id, name, phone, loyalty_points,
              created_at, updated_at, sync_status, sync_attempts)
             VALUES ('LOCAL-CUSTOMER', ?, 'LOCAL', 'Local Name', '+97333112233', 4,
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'synced', 0)",
        )
        .bind(&branch_id)
        .execute(&pool)
        .await
        .unwrap();

        let remote = json!({
            "customer_id": "REMOTE-CUSTOMER",
            "branch_id": branch_id,
            "origin_device_id": "REMOTE",
            "name": "Remote Name",
            "phone": "+97333112233",
            "email": "remote@example.com",
            "loyalty_points": 9,
            "created_at": "2026-01-02T00:00:00Z",
            "updated_at": "2026-01-02T00:00:00Z"
        });

        apply_row(&pool, "customers", &remote).await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM customers")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            count, 1,
            "duplicate phone must merge, not create another row"
        );

        let row = sqlx::query("SELECT customer_id, name, email, loyalty_points FROM customers")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("customer_id"), "LOCAL-CUSTOMER");
        assert_eq!(row.get::<String, _>("name"), "Remote Name");
        assert_eq!(
            row.get::<Option<String>, _>("email").as_deref(),
            Some("remote@example.com")
        );
        assert_eq!(row.get::<i64, _>("loyalty_points"), 9);
    }

    #[tokio::test]
    async fn customer_pull_treats_blank_phone_as_missing() {
        let pool = test_pool().await;
        let branch_id: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO customers
             (customer_id, branch_id, origin_device_id, name, phone, loyalty_points,
              created_at, updated_at, sync_status, sync_attempts)
             VALUES ('LOCAL-BLANK', ?, 'LOCAL', 'Local Blank', '', 0,
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'synced', 0)",
        )
        .bind(&branch_id)
        .execute(&pool)
        .await
        .unwrap();

        let remote = json!({
            "customer_id": "REMOTE-BLANK",
            "branch_id": branch_id,
            "origin_device_id": "REMOTE",
            "name": "Remote Blank",
            "phone": "",
            "email": "blank@example.com",
            "loyalty_points": 0,
            "created_at": "2026-01-02T00:00:00Z",
            "updated_at": "2026-01-02T00:00:00Z"
        });

        apply_row(&pool, "customers", &remote).await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM customers")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2, "blank phone customers must not block sync");

        let blank_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM customers WHERE phone = ''")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            blank_count, 0,
            "blank phones should be normalized to NULL so future rows do not collide"
        );
    }

    #[tokio::test]
    async fn app_config_rejects_non_allowlisted_keys() {
        let pool = test_pool().await;
        let evil = json!({"key":"supabase_service_key","value":"stolen","updated_at":"2030-01-01T00:00:00Z"});
        apply_row(&pool, "app_config", &evil).await.unwrap();
        let v: Option<String> =
            sqlx::query_scalar("SELECT value FROM app_config WHERE key='supabase_service_key'")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert!(v.is_none() || v.as_deref() == Some(""));
        let ok = json!({"key":"flag_auto_print_receipt","value":"1","updated_at":"2030-01-01T00:00:00Z"});
        apply_row(&pool, "app_config", &ok).await.unwrap();
        let v: String =
            sqlx::query_scalar("SELECT value FROM app_config WHERE key='flag_auto_print_receipt'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(v, "1");
    }

    #[test]
    fn config_key_allowlist_rejects_secret_like_names() {
        for key in [
            "anthropic_api_key",
            "openai_api_key",
            "gemini_api_key",
            "hub_store_token",
            "supabase_service_key",
            "refresh_secret",
        ] {
            assert!(!is_allowed_config_key(key), "{key} must never sync");
        }
    }

    #[test]
    fn confirmation_preference_is_safe_to_sync() {
        assert!(is_allowed_config_key("ai_confirm_non_destructive_actions"));
    }

    #[test]
    fn ai_policy_preferences_sync_only_for_registered_tools() {
        assert!(is_allowed_config_key("ai_sensitive_protection_level"));
        assert!(is_allowed_config_key("ai_tool_enabled_create_product"));
        assert!(!is_allowed_config_key(
            "ai_tool_enabled_future_unknown_tool"
        ));
        assert!(!is_allowed_config_key("ai_tool_enabled_openai_api_key"));
    }

    // Identity must never replicate. If device_id or hub_url crossed the wire,
    // a terminal would adopt a sibling's identity and mint colliding receipt
    // numbers under it. reports_device_scope *is* deliberately synced — it is a
    // fleet-wide policy ("each till reports on itself" vs "everyone sees
    // everything"), documented in sync::scope — so it is not listed here.
    #[test]
    fn identity_never_syncs() {
        assert!(!is_allowed_config_key("device_id"));
        assert!(!is_allowed_config_key("hub_url"));
        assert!(!is_allowed_config_key("hub_mode"));
        assert!(!is_allowed_config_key("setup_complete"));
        assert!(is_allowed_config_key("reports_device_scope"));
    }

    // Regression: both tables carried the full sync contract (sync_status,
    // updated_at, a stable PK) but were never listed, so their rows stayed on
    // whichever terminal created them. riders matters most — delivery_orders
    // *does* sync and carries rider_id, so an unsynced roster leaves deliveries
    // pointing at a rider the receiving terminal has never seen.
    #[test]
    fn tables_built_to_sync_are_actually_wired() {
        for table in ["riders", "po_receipts"] {
            assert!(
                SYNC_TABLES.contains(&table),
                "{table} has sync scaffolding but is not in SYNC_TABLES"
            );
            assert_ne!(
                pk_for_table(table),
                "id",
                "{table} needs a real primary key mapping, not the fallback"
            );
        }
    }

    // A table listed for sync but missing an apply_row arm used to be pulled and
    // thrown away with only a log line, while the pull reported success. Every
    // synced table must reach a real handler.
    #[tokio::test]
    async fn every_synced_table_has_an_apply_handler() {
        let pool = test_pool().await;
        for table in SYNC_TABLES {
            // An empty object exercises dispatch without satisfying NOT NULL, so
            // a handled table fails on the row and only an *unhandled* one fails
            // on dispatch. Distinguish by message.
            let err = apply_row(&pool, table, &json!({}))
                .await
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default();
            assert!(
                !err.contains("No sync apply handler"),
                "{table} is in SYNC_TABLES but apply_row has no arm for it"
            );
        }
    }

    /// Branch, device and an open shift — the FK spine a `sales` row needs.
    /// The hub always sends full rows, so every NOT NULL column must be present.
    async fn sale_context(pool: &SqlitePool) -> (String, String, String) {
        let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
            .fetch_one(pool)
            .await
            .unwrap();
        let device: String = sqlx::query_scalar("SELECT device_id FROM devices LIMIT 1")
            .fetch_one(pool)
            .await
            .unwrap();
        let shift = ulid::Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO shifts (shift_id, branch_id, device_id, cashier_user_id, status,
                                 opened_at, created_at, updated_at)
             VALUES (?,?,?,'01JUSER000000000000ADMIN1','open',
                     datetime('now'), datetime('now'), datetime('now'))",
        )
        .bind(&shift)
        .bind(&branch)
        .bind(&device)
        .execute(pool)
        .await
        .unwrap();
        (branch, device, shift)
    }

    // Two terminals left on the seeded identity mint the same receipt_number
    // from their own counters. receipt_number is UNIQUE, so the second sale to
    // reach the hub cannot be stored — it used to be swallowed by INSERT OR
    // IGNORE while the pull reported success, and the sale was simply gone.
    #[tokio::test]
    async fn colliding_sale_is_recorded_as_a_conflict_not_dropped() {
        let pool = test_pool().await;
        let (branch, device, shift) = sale_context(&pool).await;

        let sale = |id: &str, ik: &str| {
            json!({"sale_id":id,"branch_id":branch,"device_id":device,"shift_id":shift,
                "origin_device_id":device,"receipt_number":"MAIN-POS01-00000001",
                "cashier_user_id":"01JUSER000000000000ADMIN1","status":"completed",
                "gross_total_minor":100,"discount_total_minor":0,"tax_total_minor":0,
                "net_total_minor":100,"sold_at":"2026-01-01T00:00:00Z",
                "business_date":"2026-01-01","idempotency_key":ik,
                "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"})
        };

        apply_row(&pool, "sales", &sale("SALE-LANE-1", "ik-1"))
            .await
            .expect("first sale stores");

        // A different sale from another till carrying the same receipt number.
        apply_row(&pool, "sales", &sale("SALE-LANE-2", "ik-2"))
            .await
            .expect("collision must not fail the pull");

        let conflicts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sync_conflicts
              WHERE table_name = 'sales' AND entity_id = 'SALE-LANE-2'
                AND severity = 'critical' AND status = 'open'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(conflicts, 1, "the rejected sale must be recorded, not lost");
    }

    // The protocol re-delivers rows routinely; that must stay silent or the
    // conflicts list fills with noise and real collisions get missed.
    #[tokio::test]
    async fn redelivering_the_same_row_records_no_conflict() {
        let pool = test_pool().await;
        let (branch, device, shift) = sale_context(&pool).await;
        let sale = json!({"sale_id":"SALE-1","branch_id":branch,"device_id":device,
            "shift_id":shift,
            "origin_device_id":device,"receipt_number":"MAIN-POS01-00000009",
            "cashier_user_id":"01JUSER000000000000ADMIN1","status":"completed",
            "gross_total_minor":100,"discount_total_minor":0,"tax_total_minor":0,
            "net_total_minor":100,"sold_at":"2026-01-01T00:00:00Z",
            "business_date":"2026-01-01","idempotency_key":"ik-9",
            "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"});

        apply_row(&pool, "sales", &sale).await.expect("first");
        apply_row(&pool, "sales", &sale).await.expect("replay");

        let conflicts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sync_conflicts")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(conflicts, 0, "an idempotent replay is not a conflict");
    }

    // A shift row re-delivered after that shift has already ended must not close
    // whichever shift is live now — that stops the till taking sales.
    #[tokio::test]
    async fn stale_open_shift_does_not_close_the_live_one() {
        let pool = test_pool().await;
        let (branch, device, old_shift) = sale_context(&pool).await;

        // The old shift has since been closed locally, later than the row the
        // hub still holds for it.
        sqlx::query(
            "UPDATE shifts SET status='closed', updated_at='2026-01-02T00:00:00Z' WHERE shift_id=?",
        )
        .bind(&old_shift)
        .execute(&pool)
        .await
        .unwrap();

        // The shift actually running now.
        let live = "SHIFT-LIVE";
        sqlx::query(
            "INSERT INTO shifts (shift_id, branch_id, device_id, cashier_user_id, status,
                                 opened_at, created_at, updated_at)
             VALUES (?,?,?,'01JUSER000000000000ADMIN1','open',
                     '2026-01-03T00:00:00Z','2026-01-03T00:00:00Z','2026-01-03T00:00:00Z')",
        )
        .bind(live)
        .bind(&branch)
        .bind(&device)
        .execute(&pool)
        .await
        .unwrap();

        // The hub re-offers the old shift, still marked open and stamped before
        // the local close.
        let stale = json!({"shift_id":old_shift,"branch_id":branch,"device_id":device,
            "origin_device_id":device,"cashier_user_id":"01JUSER000000000000ADMIN1",
            "status":"open","opened_at":"2026-01-01T00:00:00Z",
            "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"});
        apply_row(&pool, "shifts", &stale)
            .await
            .expect("apply stale");

        let status: String = sqlx::query_scalar("SELECT status FROM shifts WHERE shift_id = ?")
            .bind(live)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "open", "the live shift must survive a stale replay");
    }

    // A genuinely new open shift still closes the previous one on that till.
    #[tokio::test]
    async fn a_new_open_shift_still_closes_the_previous_one() {
        let pool = test_pool().await;
        let (branch, device, previous) = sale_context(&pool).await;

        let incoming = json!({"shift_id":"SHIFT-NEW","branch_id":branch,"device_id":device,
            "origin_device_id":device,"cashier_user_id":"01JUSER000000000000ADMIN1",
            "status":"open","opened_at":"2030-01-01T00:00:00Z",
            "created_at":"2030-01-01T00:00:00Z","updated_at":"2030-01-01T00:00:00Z"});
        apply_row(&pool, "shifts", &incoming).await.expect("apply");

        let status: String = sqlx::query_scalar("SELECT status FROM shifts WHERE shift_id = ?")
            .bind(&previous)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            status, "closed",
            "a real handover still closes the old shift"
        );
    }

    #[tokio::test]
    async fn riders_round_trip_through_apply() {
        let pool = test_pool().await;
        let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        let row = json!({
            "rider_id": "RD-1", "branch_id": branch, "origin_device_id": "OTHER-TERMINAL",
            "name": "Ali", "phone": "+97333050666", "notes": Value::Null, "is_active": 1,
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
            "deleted_at": Value::Null, "version": 1,
        });
        apply_row(&pool, "riders", &row).await.expect("apply rider");

        let name: String = sqlx::query_scalar("SELECT name FROM riders WHERE rider_id = 'RD-1'")
            .fetch_one(&pool)
            .await
            .expect("rider arrived from another terminal");
        assert_eq!(name, "Ali");

        // And a later edit from that terminal wins on updated_at.
        let mut newer = row.clone();
        newer["name"] = json!("Ali Hassan");
        newer["updated_at"] = json!("2026-02-01T00:00:00Z");
        apply_row(&pool, "riders", &newer)
            .await
            .expect("apply edit");

        let name: String = sqlx::query_scalar("SELECT name FROM riders WHERE rider_id = 'RD-1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(name, "Ali Hassan");
    }
}
