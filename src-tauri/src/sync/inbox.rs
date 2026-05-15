/// Inbox — apply pulled sync events to local SQLite.
/// Only applies catalog entities (product, price, category, tax_rule, user).
/// Append-only entities from other devices are inserted with OR IGNORE.
use sqlx::SqlitePool;
use crate::errors::AppResult;
use crate::sync::supabase_client::SyncEventRow;

pub async fn apply_event(pool: &SqlitePool, event: &SyncEventRow) -> AppResult<()> {
    let p = &event.payload_json;

    match event.entity_type.as_str() {

        // ── Mutable catalog (last-write-wins via updated_at guard) ─────────────

        "product" => {
            let product_id    = str_field(p, "product_id")?;
            let category_id   = str_field(p, "category_id")?;
            let name          = str_field(p, "name")?;
            let sku           = opt_str(p, "sku");
            let barcode       = opt_str(p, "barcode");
            let description   = opt_str(p, "description");
            let track_inv     = bool_field(p, "track_inventory");
            let allow_dec     = bool_field(p, "allow_decimal_quantity");
            let is_active     = bool_field(p, "is_active");
            let tax_rule_id   = opt_str(p, "tax_rule_id");
            let cost_minor    = opt_i64(p, "cost_minor");
            let currency      = str_field(p, "currency").unwrap_or_else(|_| "BHD".into());
            let created_at    = str_field(p, "created_at")?;
            let updated_at    = str_field(p, "updated_at")?;
            let version       = i64_field(p, "version").unwrap_or(1);

            // Only apply if remote updated_at is newer than local
            sqlx::query(
                "INSERT INTO products
                 (product_id, category_id, name, sku, barcode, description,
                  track_inventory, allow_decimal_quantity, is_active,
                  tax_rule_id, cost_minor, currency, created_at, updated_at, version)
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                 ON CONFLICT(product_id) DO UPDATE SET
                   name                   = excluded.name,
                   sku                    = excluded.sku,
                   barcode                = excluded.barcode,
                   description            = excluded.description,
                   track_inventory        = excluded.track_inventory,
                   allow_decimal_quantity = excluded.allow_decimal_quantity,
                   is_active              = excluded.is_active,
                   tax_rule_id            = excluded.tax_rule_id,
                   cost_minor             = excluded.cost_minor,
                   updated_at             = excluded.updated_at,
                   version                = excluded.version
                 WHERE products.updated_at < excluded.updated_at"
            )
            .bind(&product_id).bind(&category_id).bind(&name).bind(sku).bind(barcode)
            .bind(description).bind(track_inv as i64).bind(allow_dec as i64)
            .bind(is_active as i64).bind(tax_rule_id).bind(cost_minor).bind(&currency)
            .bind(&created_at).bind(&updated_at).bind(version)
            .execute(pool)
            .await?;
        }

        "category" => {
            let category_id = str_field(p, "category_id")?;
            let name        = str_field(p, "name")?;
            let sort_order  = i64_field(p, "sort_order").unwrap_or(0);
            let is_active   = bool_field(p, "is_active");
            let created_at  = str_field(p, "created_at")?;
            let updated_at  = str_field(p, "updated_at")?;
            let version     = i64_field(p, "version").unwrap_or(1);

            sqlx::query(
                "INSERT INTO categories (category_id, name, sort_order, is_active, created_at, updated_at, version)
                 VALUES (?,?,?,?,?,?,?)
                 ON CONFLICT(category_id) DO UPDATE SET
                   name       = excluded.name,
                   sort_order = excluded.sort_order,
                   is_active  = excluded.is_active,
                   updated_at = excluded.updated_at,
                   version    = excluded.version
                 WHERE categories.updated_at < excluded.updated_at"
            )
            .bind(&category_id).bind(&name).bind(sort_order).bind(is_active as i64)
            .bind(&created_at).bind(&updated_at).bind(version)
            .execute(pool)
            .await?;
        }

        "tax_rule" => {
            let tax_rule_id       = str_field(p, "tax_rule_id")?;
            let name              = str_field(p, "name")?;
            let rate_basis_points = i64_field(p, "rate_basis_points").unwrap_or(0);
            let inclusive         = bool_field(p, "inclusive");
            let is_active         = bool_field(p, "is_active");
            let effective_from    = str_field(p, "effective_from")?;
            let effective_to      = opt_str(p, "effective_to");
            let version           = i64_field(p, "version").unwrap_or(1);

            sqlx::query(
                "INSERT INTO tax_rules (tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from, effective_to, version)
                 VALUES (?,?,?,?,?,?,?,?)
                 ON CONFLICT(tax_rule_id) DO UPDATE SET
                   name              = excluded.name,
                   rate_basis_points = excluded.rate_basis_points,
                   inclusive         = excluded.inclusive,
                   is_active         = excluded.is_active,
                   effective_to      = excluded.effective_to,
                   version           = excluded.version"
            )
            .bind(&tax_rule_id).bind(&name).bind(rate_basis_points).bind(inclusive as i64)
            .bind(is_active as i64).bind(&effective_from).bind(effective_to).bind(version)
            .execute(pool)
            .await?;
        }

        "user" => {
            let user_id      = str_field(p, "user_id")?;
            let display_name = str_field(p, "display_name")?;
            let username     = str_field(p, "username")?;
            let role_id      = str_field(p, "role_id")?;
            let branch_scope = str_field(p, "branch_scope").unwrap_or_else(|_| "[]".into());
            let is_active    = bool_field(p, "is_active");
            let created_at   = str_field(p, "created_at")?;
            let updated_at   = str_field(p, "updated_at")?;
            let version      = i64_field(p, "version").unwrap_or(1);

            sqlx::query(
                "INSERT INTO users (user_id, display_name, username, role_id, branch_scope, is_active, created_at, updated_at, version)
                 VALUES (?,?,?,?,?,?,?,?,?)
                 ON CONFLICT(user_id) DO UPDATE SET
                   display_name = excluded.display_name,
                   username     = excluded.username,
                   role_id      = excluded.role_id,
                   branch_scope = excluded.branch_scope,
                   is_active    = excluded.is_active,
                   updated_at   = excluded.updated_at,
                   version      = excluded.version
                 WHERE users.updated_at < excluded.updated_at"
            )
            .bind(&user_id).bind(&display_name).bind(&username).bind(&role_id)
            .bind(&branch_scope).bind(is_active as i64)
            .bind(&created_at).bind(&updated_at).bind(version)
            .execute(pool)
            .await?;
        }

        // ── Append-only from other devices (deduplicated by PK) ───────────────

        "product_price" => {
            let price_id               = str_field(p, "price_id")?;
            let product_id             = str_field(p, "product_id")?;
            let branch_id              = opt_str(p, "branch_id");
            let price_type             = str_field(p, "price_type").unwrap_or_else(|_| "selling".into());
            let price_minor            = i64_field(p, "price_minor")?;
            let currency               = str_field(p, "currency").unwrap_or_else(|_| "BHD".into());
            let effective_from         = str_field(p, "effective_from")?;
            let effective_to           = opt_str(p, "effective_to");
            let created_by_user_id     = str_field(p, "created_by_user_id")?;
            let created_by_ai_action_id = opt_str(p, "created_by_ai_action_id");
            let created_at             = str_field(p, "created_at")?;

            sqlx::query(
                "INSERT OR IGNORE INTO product_prices
                 (price_id, product_id, branch_id, price_type, price_minor, currency,
                  effective_from, effective_to, created_by_user_id, created_by_ai_action_id, created_at)
                 VALUES (?,?,?,?,?,?,?,?,?,?,?)"
            )
            .bind(&price_id).bind(&product_id).bind(branch_id).bind(&price_type)
            .bind(price_minor).bind(&currency).bind(&effective_from).bind(effective_to)
            .bind(&created_by_user_id).bind(created_by_ai_action_id).bind(&created_at)
            .execute(pool)
            .await?;
        }

        // ── Stock levels from other devices: apply LWW ────────────────────────
        "stock_level" => {
            let product_id       = str_field(p, "product_id")?;
            let branch_id_field  = str_field(p, "branch_id")?;
            let quantity_on_hand = str_field(p, "quantity_on_hand")?;
            let updated_at       = str_field(p, "updated_at")?;
            let stock_level_id   = format!("SL-{}", product_id);

            sqlx::query(
                "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, updated_at)
                 VALUES (?,?,?,?,?)
                 ON CONFLICT(product_id, branch_id) DO UPDATE SET
                   quantity_on_hand = excluded.quantity_on_hand,
                   updated_at       = excluded.updated_at
                 WHERE stock_levels.updated_at < excluded.updated_at"
            )
            .bind(&stock_level_id)
            .bind(&product_id)
            .bind(&branch_id_field)
            .bind(&quantity_on_hand)
            .bind(&updated_at)
            .execute(pool)
            .await?;
        }

        // ── Stock movements from other devices: append-only ────────────────────
        "stock_movement" => {
            let movement_id        = str_field(p, "movement_id")?;
            let product_id         = str_field(p, "product_id")?;
            let branch_id_field    = str_field(p, "branch_id")?;
            let device_id          = str_field(p, "device_id")?;
            let movement_type      = str_field(p, "movement_type")?;
            let quantity_delta     = str_field(p, "quantity_delta")?;
            let quantity_after     = str_field(p, "quantity_after")?;
            let reference_type     = opt_str(p, "reference_type");
            let reference_id       = opt_str(p, "reference_id");
            let notes              = opt_str(p, "notes");
            let created_by_user_id = opt_str(p, "created_by_user_id");
            let created_at         = str_field(p, "created_at")?;

            sqlx::query(
                "INSERT OR IGNORE INTO stock_movements
                 (movement_id, product_id, branch_id, device_id, movement_type,
                  quantity_delta, quantity_after, reference_type, reference_id,
                  notes, created_by_user_id, created_at, sync_status)
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,'synced')"
            )
            .bind(&movement_id).bind(&product_id).bind(&branch_id_field)
            .bind(&device_id).bind(&movement_type)
            .bind(&quantity_delta).bind(&quantity_after)
            .bind(reference_type).bind(reference_id)
            .bind(notes).bind(created_by_user_id).bind(&created_at)
            .execute(pool)
            .await?;
        }

        // Sales, payments, refunds, shifts, audit_logs from other devices:
        // We do NOT import these locally — each device owns its own transactional data.
        // The central DB is the archive. If cross-device reporting is needed, it queries
        // Supabase directly (future phase). Ignore silently.
        "sale" | "sale_item" | "payment" | "shift" |
        "refund" | "refund_item" | "audit_log" => {
            // Silently ignore — local device only reads its own transactional data
        }

        other => {
            tracing::warn!("inbox: unknown entity_type '{}', skipping", other);
        }
    }

    Ok(())
}

// ── Field extraction helpers ───────────────────────────────────────────────────

fn str_field(p: &serde_json::Value, key: &str) -> Result<String, crate::errors::AppError> {
    p.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| crate::errors::AppError::Internal(format!("inbox: missing field '{key}'")))
}

fn opt_str(p: &serde_json::Value, key: &str) -> Option<String> {
    p.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn bool_field(p: &serde_json::Value, key: &str) -> bool {
    p.get(key)
        .map(|v| v.as_bool().unwrap_or_else(|| v.as_i64().map(|n| n != 0).unwrap_or(false)))
        .unwrap_or(false)
}

fn i64_field(p: &serde_json::Value, key: &str) -> Result<i64, crate::errors::AppError> {
    p.get(key)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| crate::errors::AppError::Internal(format!("inbox: missing i64 field '{key}'")))
}

fn opt_i64(p: &serde_json::Value, key: &str) -> Option<i64> {
    p.get(key).and_then(|v| v.as_i64())
}
