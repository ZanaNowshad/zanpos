use crate::errors::{AppError, AppResult};
use serde_json::{Map, Value};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

/// Every table the sync protocol may read or write. Hub rejects any other name.
pub const SYNC_TABLES: &[&str] = &[
    "branches", "categories", "tax_rules", "products", "devices", "users",
    "customers", "shifts", "sales", "sale_items", "payments", "refunds",
    "refund_items", "stock_movements", "stock_levels", "audit_logs",
    "delivery_orders", "product_prices", "cash_events", "app_config",
];

/// app_config keys that are allowed to sync across devices.
pub const ALLOWED_CONFIG_KEYS: &[&str] = &[
    "flag_allow_negative_stock",
    "flag_require_discount_reason",
    "flag_cashier_can_discount",
    "flag_auto_print_receipt",
    "whatsapp_benefit_number",
    "reports_device_scope",
];

pub(crate) const STOCK_DRIFT_TOLERANCE: f64 = 0.001;

/// Apply a single row to the local database.
pub async fn apply_row(pool: &SqlitePool, table: &str, row: &Value) -> AppResult<()> {
    let obj = row.as_object().ok_or_else(|| {
        AppError::Internal("apply_row: row is not a JSON object".into())
    })?;

    match table {
        "categories" => {
            apply_lww(pool, "categories", "category_id", obj, &[]).await
        }
        "tax_rules" => {
            apply_lww(pool, "tax_rules", "tax_rule_id", obj, &[]).await
        }
        "products" => {
            apply_lww(pool, "products", "product_id", obj, &[]).await
        }
        "devices" => {
            if let (Some(device_id), Some(branch_id), Some(device_code)) = (
                obj.get("device_id").and_then(|v| v.as_str()),
                obj.get("branch_id").and_then(|v| v.as_str()),
                obj.get("device_code").and_then(|v| v.as_str()),
            ) {
                let now = chrono::Utc::now().to_rfc3339();
                let _ = sqlx::query(
                    "UPDATE devices SET is_active = 0, deleted_at = ? WHERE branch_id = ? AND device_code = ? AND device_id <> ?",
                )
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
        "branches" => {
            apply_lww(pool, "branches", "branch_id", obj, &[]).await
        }
        "shifts" => {
            if obj.get("status").and_then(|v| v.as_str()) == Some("open") {
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
                let fallback = obj_norm.get("opened_at")
                    .cloned()
                    .filter(|v| !matches!(v, Value::Null))
                    .or_else(|| {
                        obj_norm.get("updated_at").cloned()
                            .filter(|v| !matches!(v, Value::Null))
                    })
                    .unwrap_or_else(|| Value::String(chrono::Utc::now().to_rfc3339()));
                obj_norm.insert("created_at".to_string(), fallback);
            }
            apply_lww(pool, "shifts", "shift_id", &obj_norm, &[]).await
        }
        "delivery_orders" => {
            apply_lww(pool, "delivery_orders", "delivery_id", obj, &[]).await
        }
        "stock_levels" => {
            apply_lww(pool, "stock_levels", "stock_level_id", obj, &[]).await
        }
        "users" => {
            if let (Some(user_id), Some(username)) = (
                obj.get("user_id").and_then(|v| v.as_str()),
                obj.get("username").and_then(|v| v.as_str()),
            ) {
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
                let fallback_branch: Option<String> = sqlx::query_scalar(
                    "SELECT branch_id FROM branches WHERE is_active=1 LIMIT 1",
                )
                .fetch_optional(pool)
                .await
                .ok()
                .flatten();
                let branch = fallback_branch
                    .unwrap_or_else(|| "01JBRANCH0000000000000001".to_string());
                obj_norm.insert("branch_id".to_string(), Value::String(branch));
            }

            if !obj_norm.contains_key("pin_hash")
                || obj_norm.get("pin_hash") == Some(&Value::Null)
            {
                obj_norm.insert(
                    "pin_hash".to_string(),
                    Value::String("*REMOTE-ONLY*".to_string()),
                );
            }

            if let Some(rid) = obj_norm.get("role_id").and_then(|v| v.as_str()).map(|s| s.to_string()) {
                let role_exists: bool = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM roles WHERE role_id = ?",
                )
                .bind(&rid)
                .fetch_one(pool)
                .await
                .map(|n| n > 0)
                .unwrap_or(false);
                if !role_exists {
                    tracing::warn!("Sync v2: unknown role_id '{rid}' for remote user — remapping to 'owner'");
                    obj_norm.insert(
                        "role_id".to_string(),
                        Value::String("01JROLES000000000000000001".to_string()),
                    );
                }
            }

            apply_lww(pool, "users", "user_id", &obj_norm, &["pin_hash"]).await
        }
        "customers" => {
            apply_customer(pool, obj).await
        }
        "sales" | "sale_items" | "payments" | "refunds"
        | "refund_items" | "stock_movements" | "audit_logs"
        | "product_prices" | "cash_events" => {
            apply_append_only(pool, table, obj).await
        }
        "app_config" => {
            let key = obj.get("key").and_then(|v| v.as_str()).unwrap_or("");
            if ALLOWED_CONFIG_KEYS.contains(&key) {
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
        other => {
            tracing::warn!("Sync v2: unknown table '{}', skipping apply", other);
            Ok(())
        }
    }
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

pub(crate) async fn apply_lww(
    pool: &SqlitePool,
    table: &str,
    pk: &str,
    obj: &Map<String, Value>,
    exclude_cols: &[&str],
) -> AppResult<()> {
    let cols: Vec<&String> = obj
        .keys()
        .filter(|k| {
            is_safe_col(k)
                && *k != "sync_status" && *k != "sync_attempts"
                && !matches!(obj.get(*k), Some(Value::Null))
        })
        .collect();

    if cols.is_empty() {
        return Ok(());
    }

    let col_list = format!("{}, sync_status", cols.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(", "));
    let val_list = format!("{}, 'synced'", cols
        .iter()
        .map(|c| json_to_sql_literal(&obj[*c]))
        .collect::<Vec<_>>()
        .join(", "));

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

pub(crate) async fn apply_customer(
    pool: &SqlitePool,
    obj: &Map<String, Value>,
) -> AppResult<()> {
    let cols: Vec<&String> = obj
        .keys()
        .filter(|k| {
            is_safe_col(k)
                && *k != "sync_status" && *k != "sync_attempts"
                && !matches!(obj.get(*k), Some(Value::Null))
        })
        .collect();

    if cols.is_empty() {
        return Ok(());
    }

    let col_list = format!(
        "{}, sync_status",
        cols.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(", ")
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

pub(crate) async fn apply_append_only(
    pool: &SqlitePool,
    table: &str,
    obj: &Map<String, Value>,
) -> AppResult<()> {
    let cols: Vec<&String> = obj
        .keys()
        .filter(|k| {
            is_safe_col(k)
                && *k != "sync_status" && *k != "sync_attempts"
                && !matches!(obj.get(*k), Some(Value::Null))
        })
        .collect();

    if cols.is_empty() {
        return Ok(());
    }

    let col_list = cols.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(", ");
    let val_list = cols
        .iter()
        .map(|c| json_to_sql_literal(&obj[*c]))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "INSERT OR IGNORE INTO {} ({}, sync_status) VALUES ({}, 'synced')",
        table, col_list, val_list,
    );

    sqlx::query(&sql).execute(pool).await?;

    if table == "stock_movements" {
        if let (Some(product_id), Some(branch_id), Some(created_at)) = (
            obj.get("product_id").and_then(|v| v.as_str()),
            obj.get("branch_id").and_then(|v| v.as_str()),
            obj.get("created_at").and_then(|v| v.as_str()),
        ) {
            let _ = recompute_stock_level(
                pool,
                product_id,
                branch_id,
                created_at,
            )
            .await;
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
        "devices" => "device_id",
        "customers" => "customer_id",
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
        "users" => "user_id",
        "cash_events" => "cash_event_id",
        "app_config" => "key",
        _ => "id",
    }
}

/// Returns true if a column is local-only and must not be included
/// in JSON payloads sent to the hub or pulled from the hub.
pub fn should_skip_column(table: &str, col_name: &str) -> bool {
    if col_name == "sync_status"
        || col_name == "sync_attempts"
        || col_name == "pin_hash"
        || col_name == "deleted_at"
        || col_name == "version"
    {
        return true;
    }
    match (table, col_name) {
        ("users", "failed_pin_attempts" | "locked_until" | "last_login_at") => true,
        ("devices", "next_receipt_seq" | "last_seen_at" | "version") => true,
        ("customers", "origin_device_id" | "version") => true,
        ("shifts", "expected_cash_minor" | "cash_difference_minor" | "business_date" | "created_at" | "version") => true,
        ("audit_logs", "override_used") => true,
        _ => false,
    }
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
pub(crate) async fn recompute_stock_level(
    pool: &SqlitePool,
    product_id: &str,
    branch_id: &str,
    applied_at: &str,
) -> AppResult<()> {
    let ledger_sum: Option<f64> = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(CAST(quantity_delta AS REAL)), 0) AS REAL)
         FROM stock_movements
         WHERE product_id = ? AND branch_id = ?",
    )
    .bind(product_id)
    .bind(branch_id)
    .fetch_one(pool)
    .await?;

    let ledger = ledger_sum.unwrap_or(0.0);

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
        let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", path.display())).await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn lww_newer_wins_older_loses() {
        let pool = test_pool().await;
        // The hub always sends full rows — all NOT NULL columns must be present
        // because SQLite checks NOT NULL before ON CONFLICT evaluation.
        let base = json!({"category_id":"C1","name":"Old","sort_order":0,"is_active":1,
            "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"});
        apply_lww(&pool, "categories", "category_id", base.as_object().unwrap(), &[]).await.unwrap();
        let newer = json!({"category_id":"C1","name":"New","sort_order":0,"is_active":1,
            "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"});
        apply_lww(&pool, "categories", "category_id", newer.as_object().unwrap(), &[]).await.unwrap();
        let n: String = sqlx::query_scalar("SELECT name FROM categories WHERE category_id='C1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(n, "New");
        let older = json!({"category_id":"C1","name":"Stale","sort_order":0,"is_active":1,
            "created_at":"2026-01-01T00:00:00Z","updated_at":"2025-12-31T00:00:00Z"});
        apply_lww(&pool, "categories", "category_id", older.as_object().unwrap(), &[]).await.unwrap();
        let n: String = sqlx::query_scalar("SELECT name FROM categories WHERE category_id='C1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(n, "New", "older update must not overwrite");
    }

    #[tokio::test]
    async fn append_only_is_idempotent() {
        let pool = test_pool().await;
        let branch_id: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
            .fetch_one(&pool).await.unwrap();
        let device_id: String = sqlx::query_scalar("SELECT device_id FROM devices WHERE is_active=1 LIMIT 1")
            .fetch_optional(&pool).await.unwrap()
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
            .fetch_one(&pool).await.unwrap();
        assert_eq!(c, 1);
    }

    #[tokio::test]
    async fn users_pin_hash_never_clobbered() {
        let pool = test_pool().await;
        let uid: String = sqlx::query_scalar("SELECT user_id FROM users LIMIT 1")
            .fetch_one(&pool).await.unwrap();
        let before: String = sqlx::query_scalar("SELECT pin_hash FROM users WHERE user_id=?")
            .bind(&uid).fetch_one(&pool).await.unwrap();
        let upd = json!({"user_id":uid,"display_name":"Renamed","username":"renamed-unique-x",
            "role_id":"01JROLES000000000000000001","is_active":1,"created_at":"2026-01-01T00:00:00Z",
            "updated_at":"2030-01-01T00:00:00Z"});
        apply_row(&pool, "users", &upd).await.unwrap();
        let after: String = sqlx::query_scalar("SELECT pin_hash FROM users WHERE user_id=?")
            .bind(&uid).fetch_one(&pool).await.unwrap();
        assert_eq!(before, after, "pin_hash must survive remote user updates");
    }

    #[tokio::test]
    async fn app_config_rejects_non_allowlisted_keys() {
        let pool = test_pool().await;
        let evil = json!({"key":"supabase_service_key","value":"stolen","updated_at":"2030-01-01T00:00:00Z"});
        apply_row(&pool, "app_config", &evil).await.unwrap();
        let v: Option<String> = sqlx::query_scalar(
            "SELECT value FROM app_config WHERE key='supabase_service_key'")
            .fetch_optional(&pool).await.unwrap();
        assert!(v.is_none() || v.as_deref() == Some(""));
        let ok = json!({"key":"flag_auto_print_receipt","value":"1","updated_at":"2030-01-01T00:00:00Z"});
        apply_row(&pool, "app_config", &ok).await.unwrap();
        let v: String = sqlx::query_scalar(
            "SELECT value FROM app_config WHERE key='flag_auto_print_receipt'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(v, "1");
    }
}
