use crate::errors::AppResult;
use crate::sync::supabase_client::SyncEventRow;
/// Inbox — apply pulled sync events to local SQLite.
/// Only applies catalog entities (product, price, category, tax_rule, user).
/// Append-only entities from other devices are inserted with OR IGNORE.
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

// ── FK-resilience for out-of-order catalog sync ───────────────────────────────
// Pulled events are applied in global-sequence order, but a bulk catalog import
// interleaves parents and children (a product_price can land before its product,
// a product before its category). With foreign_keys ON that throws a constraint
// error (code 787), which halts the inbox watermark and strands the entire rest
// of the catalog behind it. These helpers pre-create a minimal, hidden, ancient-
// timestamped stub for a missing FK parent so the child applies now; the real
// parent event (newer updated_at) overwrites the stub via last-write-wins when it
// arrives. Stubs are local-only (inbox never enqueues to the cloud). Idempotent.

const SYNC_STUB_TS: &str = "1970-01-01T00:00:00Z";
/// Hidden synthetic category that parks transient stub products until their real
/// product event re-parents them. Never collides with a real ULID.
const SYNC_STUB_CATEGORY_ID: &str = "00000000000000000000000000";

async fn ensure_category(pool: &SqlitePool, category_id: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO categories
           (category_id, name, sort_order, is_active, created_at, updated_at, version)
         VALUES (?, '(syncing)', 0, 0, ?, ?, 0)",
    )
    .bind(category_id).bind(SYNC_STUB_TS).bind(SYNC_STUB_TS)
    .execute(pool).await?;
    Ok(())
}

async fn ensure_tax_rule(pool: &SqlitePool, tax_rule_id: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO tax_rules
           (tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from, updated_at, version)
         VALUES (?, '(syncing)', 0, 0, 0, ?, ?, 0)",
    )
    .bind(tax_rule_id).bind(SYNC_STUB_TS).bind(SYNC_STUB_TS)
    .execute(pool).await?;
    Ok(())
}

async fn ensure_product(pool: &SqlitePool, product_id: &str) -> AppResult<()> {
    // Stub products live in a hidden synthetic category to satisfy the NOT NULL
    // category_id FK; the real product event overwrites category + everything else.
    ensure_category(pool, SYNC_STUB_CATEGORY_ID).await?;
    sqlx::query(
        "INSERT OR IGNORE INTO products
           (product_id, category_id, name, track_inventory, allow_decimal_quantity,
            is_active, currency, created_at, updated_at, version)
         VALUES (?, ?, '(syncing)', 1, 0, 0, 'BHD', ?, ?, 0)",
    )
    .bind(product_id).bind(SYNC_STUB_CATEGORY_ID).bind(SYNC_STUB_TS).bind(SYNC_STUB_TS)
    .execute(pool).await?;
    Ok(())
}

pub async fn apply_event(pool: &SqlitePool, event: &SyncEventRow) -> AppResult<()> {
    let p = &event.payload_json;

    match event.entity_type.as_str() {
        // ── Mutable catalog (last-write-wins via updated_at guard) ─────────────
        "product" => {
            let product_id = str_field(p, "product_id")?;
            let category_id = str_field(p, "category_id")?;
            let name = str_field(p, "name")?;
            let sku = opt_str(p, "sku");
            let barcode = opt_str(p, "barcode");
            let description = opt_str(p, "description");
            let track_inv = bool_field(p, "track_inventory");
            let allow_dec = bool_field(p, "allow_decimal_quantity");
            let is_active = bool_field(p, "is_active");
            let tax_rule_id = opt_str(p, "tax_rule_id");
            let cost_minor = opt_i64(p, "cost_minor");
            let currency = str_field(p, "currency").unwrap_or_else(|_| "BHD".into());
            let created_at = str_field(p, "created_at")?;
            let updated_at = str_field(p, "updated_at")?;
            let version = i64_field(p, "version").unwrap_or(1);

            // Only apply if remote updated_at is newer than local
            let reorder_point = i64_field(p, "reorder_point").unwrap_or(0);
            let image_path = opt_str(p, "image_path");
            let default_supplier_id = opt_str(p, "default_supplier_id");

            // FK-resilience: ensure the category (and tax rule, if any) exist so an
            // out-of-order product can't halt the pull on a constraint error.
            ensure_category(pool, &category_id).await?;
            if let Some(tr) = &tax_rule_id { ensure_tax_rule(pool, tr).await?; }

            // M8: category_id, currency, created_at were previously missing from the UPDATE SET,
            // so changes to those fields on another device were silently not applied locally.
            // M9: Use datetime() for LWW comparison to handle RFC3339 format drift
            // (timezone offsets, fractional seconds) that could break string comparison.
            sqlx::query(
                "INSERT INTO products
                 (product_id, category_id, name, sku, barcode, description,
                   track_inventory, allow_decimal_quantity, is_active,
                   tax_rule_id, cost_minor, currency, reorder_point, image_path, default_supplier_id, created_at, updated_at, version)
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                 ON CONFLICT(product_id) DO UPDATE SET
                   category_id            = excluded.category_id,
                   name                   = excluded.name,
                   sku                    = excluded.sku,
                   barcode                = excluded.barcode,
                   description            = excluded.description,
                   track_inventory        = excluded.track_inventory,
                   allow_decimal_quantity = excluded.allow_decimal_quantity,
                   is_active              = excluded.is_active,
                   tax_rule_id            = excluded.tax_rule_id,
                   cost_minor             = excluded.cost_minor,
                   currency               = excluded.currency,
                   reorder_point          = excluded.reorder_point,
                   image_path             = excluded.image_path,
                   default_supplier_id    = excluded.default_supplier_id,
                   created_at             = excluded.created_at,
                   updated_at             = excluded.updated_at,
                   version                = excluded.version
                 WHERE datetime(products.updated_at) < datetime(excluded.updated_at)",
            )
            .bind(&product_id)
            .bind(&category_id)
            .bind(&name)
            .bind(sku)
            .bind(barcode)
            .bind(description)
            .bind(track_inv as i64)
            .bind(allow_dec as i64)
            .bind(is_active as i64)
            .bind(tax_rule_id)
            .bind(cost_minor)
            .bind(&currency)
            .bind(reorder_point)
            .bind(image_path)
            .bind(default_supplier_id)
            .bind(&created_at)
            .bind(&updated_at)
            .bind(version)
            .execute(pool)
            .await?;
        }

        "category" => {
            let category_id = str_field(p, "category_id")?;
            let name = str_field(p, "name")?;
            let sort_order = i64_field(p, "sort_order").unwrap_or(0);
            let is_active = bool_field(p, "is_active");
            let created_at = str_field(p, "created_at")?;
            let updated_at = str_field(p, "updated_at")?;
            let version = i64_field(p, "version").unwrap_or(1);

            let parent_category_id = opt_str(p, "parent_category_id");

            // FK-resilience: a child category may arrive before its parent.
            if let Some(parent) = &parent_category_id { ensure_category(pool, parent).await?; }

            sqlx::query(
                "INSERT INTO categories (category_id, name, sort_order, is_active, parent_category_id, created_at, updated_at, version)
                 VALUES (?,?,?,?,?,?,?,?)
                 ON CONFLICT(category_id) DO UPDATE SET
                   name               = excluded.name,
                   sort_order         = excluded.sort_order,
                   is_active          = excluded.is_active,
                   parent_category_id = excluded.parent_category_id,
                   updated_at         = excluded.updated_at,
                   version            = excluded.version
                 WHERE datetime(categories.updated_at) < datetime(excluded.updated_at)"
            )
            .bind(&category_id).bind(&name).bind(sort_order).bind(is_active as i64)
            .bind(parent_category_id).bind(&created_at).bind(&updated_at).bind(version)
            .execute(pool)
            .await?;
        }

        "tax_rule" => {
            let tax_rule_id = str_field(p, "tax_rule_id")?;
            let name = str_field(p, "name")?;
            let rate_basis_points = i64_field(p, "rate_basis_points").unwrap_or(0);
            let inclusive = bool_field(p, "inclusive");
            let is_active = bool_field(p, "is_active");
            let effective_from = str_field(p, "effective_from")?;
            let effective_to = opt_str(p, "effective_to");
            let updated_at = opt_str(p, "updated_at").unwrap_or_else(|| effective_from.clone());
            let version = i64_field(p, "version").unwrap_or(1);

            sqlx::query(
                "INSERT INTO tax_rules (tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from, effective_to, updated_at, version)
                 VALUES (?,?,?,?,?,?,?,?,?)
                 ON CONFLICT(tax_rule_id) DO UPDATE SET
                   name              = excluded.name,
                   rate_basis_points = excluded.rate_basis_points,
                   inclusive         = excluded.inclusive,
                   is_active         = excluded.is_active,
                   effective_to      = excluded.effective_to,
                   updated_at        = excluded.updated_at,
                   version           = excluded.version
                 WHERE datetime(COALESCE(tax_rules.updated_at, tax_rules.effective_from)) < datetime(excluded.updated_at)"
            )
            .bind(&tax_rule_id).bind(&name).bind(rate_basis_points).bind(inclusive as i64)
            .bind(is_active as i64).bind(&effective_from).bind(effective_to).bind(&updated_at).bind(version)
            .execute(pool)
            .await?;
        }

        "user" => {
            let user_id = str_field(p, "user_id")?;
            let display_name = str_field(p, "display_name")?;
            let username = str_field(p, "username")?;
            let role_id = str_field(p, "role_id")?;
            let branch_scope = str_field(p, "branch_scope").unwrap_or_else(|_| "[]".into());
            let is_active = bool_field(p, "is_active");
            let created_at = str_field(p, "created_at")?;
            let updated_at = str_field(p, "updated_at")?;
            let version = i64_field(p, "version").unwrap_or(1);

            // The users table has BOTH a PK on user_id AND a UNIQUE index on username.
            // A naive `ON CONFLICT(user_id)` upsert fails with a UNIQUE violation when
            // an incoming user has a NEW user_id but a username that already exists
            // locally (classic case: the seeded 'admin' on a joining terminal vs the
            // real owner 'admin' from device 1). That failure is exactly what left
            // joining terminals with no usable login. Reconcile the username collision
            // FIRST so the subsequent upsert can never trip the UNIQUE index.
            //
            // Strategy: if a DIFFERENT local row already owns this username, remove it.
            // The incoming (cloud-authoritative) row is the source of truth. The local
            // colliding row is a stale seed (its PIN was a placeholder anyway).
            sqlx::query("DELETE FROM users WHERE username = ? AND user_id <> ?")
                .bind(&username)
                .bind(&user_id)
                .execute(pool)
                .await?;

            // Resolve the FK target: if the incoming role_id doesn't exist locally,
            // fall back to a role of the same name, then to any role, so the insert
            // can't fail on a missing-role FK across independently-seeded databases.
            let role_exists: Option<String> =
                sqlx::query_scalar("SELECT role_id FROM roles WHERE role_id = ?")
                    .bind(&role_id)
                    .fetch_optional(pool)
                    .await?;
            let effective_role_id = match role_exists {
                Some(r) => r,
                None => sqlx::query_scalar(
                    "SELECT role_id FROM roles ORDER BY (name='owner') DESC LIMIT 1",
                )
                .fetch_optional(pool)
                .await?
                .unwrap_or(role_id.clone()),
            };

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
                 WHERE datetime(users.updated_at) < datetime(excluded.updated_at)"
            )
            .bind(&user_id).bind(&display_name).bind(&username).bind(&effective_role_id)
            .bind(&branch_scope).bind(is_active as i64)
            .bind(&created_at).bind(&updated_at).bind(version)
            .execute(pool)
            .await?;
        }

        // ── Append-only from other devices (deduplicated by PK) ───────────────
        "product_price" => {
            let price_id = str_field(p, "price_id")?;
            let product_id = str_field(p, "product_id")?;
            let branch_id = opt_str(p, "branch_id");
            let price_type = str_field(p, "price_type").unwrap_or_else(|_| "selling".into());
            let price_minor = i64_field(p, "price_minor")?;
            let currency = str_field(p, "currency").unwrap_or_else(|_| "BHD".into());
            let effective_from = str_field(p, "effective_from")?;
            let effective_to = opt_str(p, "effective_to");
            let created_by_user_id = str_field(p, "created_by_user_id")?;
            let created_by_ai_action_id = opt_str(p, "created_by_ai_action_id");
            let created_at = str_field(p, "created_at")?;

            // FK-resilience: a price commonly lands before its product in a bulk
            // import (this is the seq-355 poison) — stub the product so it applies.
            ensure_product(pool, &product_id).await?;

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
            let product_id = str_field(p, "product_id")?;
            let branch_id_field = str_field(p, "branch_id")?;
            let quantity_on_hand = str_field(p, "quantity_on_hand")?;
            let updated_at = str_field(p, "updated_at")?;
            // Include branch_id in PK to avoid collision when the same product
        // has stock levels for multiple branches.
        let stock_level_id = format!("SL-{}-{}", product_id, branch_id_field);

            ensure_product(pool, &product_id).await?;

            sqlx::query(
                "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, updated_at)
                 VALUES (?,?,?,?,?)
                 ON CONFLICT(product_id, branch_id) DO UPDATE SET
                   quantity_on_hand = excluded.quantity_on_hand,
                   updated_at       = excluded.updated_at
                 WHERE datetime(stock_levels.updated_at) < datetime(excluded.updated_at)"
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
            let movement_id = str_field(p, "movement_id")?;
            let product_id = str_field(p, "product_id")?;
            let branch_id_field = str_field(p, "branch_id")?;
            let device_id = str_field(p, "device_id")?;
            let origin_device_id = opt_str(p, "origin_device_id")
                .unwrap_or_else(|| device_id.clone());
            let movement_type = str_field(p, "movement_type")?;
            let quantity_delta = str_field(p, "quantity_delta")?;
            let quantity_after = str_field(p, "quantity_after")?;
            let reference_type = opt_str(p, "reference_type");
            let reference_id = opt_str(p, "reference_id");
            let notes = opt_str(p, "notes");
            let created_by_user_id = opt_str(p, "created_by_user_id");
            let created_at = str_field(p, "created_at")?;

            ensure_product(pool, &product_id).await?;

            sqlx::query(
                "INSERT OR IGNORE INTO stock_movements
                 (movement_id, product_id, branch_id, device_id, origin_device_id, movement_type,
                  quantity_delta, quantity_after, reference_type, reference_id,
                  notes, created_by_user_id, created_at, sync_status)
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,'synced')",
            )
            .bind(&movement_id)
            .bind(&product_id)
            .bind(&branch_id_field)
            .bind(&device_id)
            .bind(&origin_device_id)
            .bind(&movement_type)
            .bind(&quantity_delta)
            .bind(&quantity_after)
            .bind(reference_type)
            .bind(reference_id)
            .bind(notes)
            .bind(created_by_user_id)
            .bind(&created_at)
            .execute(pool)
            .await?;

            // D-PRIME: recompute stock_levels cache from the ledger so all
            // devices converge to the same quantity. Drift is logged via a
            // sync_queue entry that the manager dashboard surfaces.
            recompute_stock_level(pool, &product_id, &branch_id_field, &created_at).await?;
        }

        // ── Device registration from other terminals ──────────────────────────────
        "device" => {
            let device_id   = str_field(p, "device_id")?;
            let branch_id   = str_field(p, "branch_id")?;
            let device_code = str_field(p, "device_code")?;
            let name        = str_field(p, "name")?;
            let is_active   = bool_field(p, "is_active");

            // M11: Add updated_at to devices so stale events cannot overwrite fresh metadata.
            // Devices that don't carry updated_at (legacy rows) always accept the update.
            let updated_at = opt_str(p, "updated_at")
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

            // devices has a UNIQUE(branch_id, device_code) index. A joining terminal
            // may still hold the seeded 'POS01' under a different device_id, which would
            // make the incoming device_code collide. Remove any DIFFERENT local row that
            // owns this (branch_id, device_code) so the upsert can't fail on the UNIQUE
            // index (same class of bug as the user/username collision above).
            sqlx::query(
                "DELETE FROM devices WHERE branch_id = ? AND device_code = ? AND device_id <> ?",
            )
            .bind(&branch_id)
            .bind(&device_code)
            .bind(&device_id)
            .execute(pool)
            .await?;

            sqlx::query(
                "INSERT INTO devices
                   (device_id, branch_id, device_code, name, status, is_active, updated_at)
                 VALUES (?,?,?,?,'online',?,?)
                 ON CONFLICT(device_id) DO UPDATE SET
                   name        = excluded.name,
                   device_code = excluded.device_code,
                   is_active   = excluded.is_active,
                   updated_at  = excluded.updated_at
                 WHERE devices.updated_at IS NULL
                    OR datetime(devices.updated_at) < datetime(excluded.updated_at)",
            )
            .bind(&device_id)
            .bind(&branch_id)
            .bind(&device_code)
            .bind(&name)
            .bind(is_active as i64)
            .bind(&updated_at)
            .execute(pool)
            .await?;
        }

        // ── Store-wide app config (business flags, benefit number) ───────────────
        "app_config" => {
            let key   = str_field(p, "key")?;
            let value = opt_str(p, "value").unwrap_or_default();

            // Whitelist: sync only store-wide settings.
            // Device-specific keys (supabase creds, printer port, grace deadline) are excluded.
            const SYNCABLE: &[&str] = &[
                "flag_allow_negative_stock",
                "flag_require_discount_reason",
                "flag_cashier_can_discount",
                "flag_auto_print_receipt",
                "whatsapp_benefit_number",
                "reports_device_scope",
            ];
            if SYNCABLE.contains(&key.as_str()) {
                let now = chrono::Utc::now().to_rfc3339();
                sqlx::query(
                    "INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)
                     ON CONFLICT(key) DO UPDATE SET
                       value      = excluded.value,
                       updated_at = excluded.updated_at",
                )
                .bind(&key)
                .bind(&value)
                .bind(&now)
                .execute(pool)
                .await?;
            }
        }

        // ── Customers (last-write-wins on name/phone/email/notes) ────────────────
        "customer" => {
            let customer_id    = str_field(p, "customer_id")?;
            let branch_id      = str_field(p, "branch_id")?;
            let name           = str_field(p, "name")?;
            let phone          = opt_str(p, "phone");
            let email          = opt_str(p, "email");
            let loyalty_points = i64_field(p, "loyalty_points").unwrap_or(0);
            let notes          = opt_str(p, "notes");
            let created_at     = str_field(p, "created_at")?;
            let updated_at     = opt_str(p, "updated_at").unwrap_or_else(|| created_at.clone());

            sqlx::query(
                "INSERT INTO customers
                   (customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes, updated_at)
                 VALUES (?,?,?,?,?,?,?,?,?)
                 ON CONFLICT(customer_id) DO UPDATE SET
                   name           = excluded.name,
                   phone          = excluded.phone,
                   email          = excluded.email,
                   loyalty_points = MAX(customers.loyalty_points, excluded.loyalty_points),
                   notes          = excluded.notes,
                   updated_at     = excluded.updated_at
                 WHERE datetime(customers.updated_at) < datetime(excluded.updated_at)",
            )
            .bind(&customer_id)
            .bind(&branch_id)
            .bind(&name)
            .bind(phone)
            .bind(email)
            .bind(loyalty_points)
            .bind(&created_at)
            .bind(notes)
            .bind(&updated_at)
            .execute(pool)
            .await?;
        }

        // F-HIGH-02: delivery_order events from other terminals — apply locally so all
        // terminals can see and manage deliveries regardless of which terminal created them.
        // Column names match the local SQLite schema in 0014_delivery.sql.
        "delivery_order" => {
            let delivery_id          = str_field(p, "delivery_id")?;
            let sale_id              = str_field(p, "sale_id")?;
            let receipt_number       = str_field(p, "receipt_number")?;
            let branch_id_field      = str_field(p, "branch_id")?;
            let device_id_field      = str_field(p, "device_id")?;
            let origin_device_id     = opt_str(p, "origin_device_id")
                .unwrap_or_else(|| device_id_field.clone());
            let customer_id          = opt_str(p, "customer_id");
            let customer_name        = opt_str(p, "customer_name");
            let contact_number       = str_field(p, "contact_number").unwrap_or_default();
            let address_text         = str_field(p, "address_text").unwrap_or_default();
            let house_number         = opt_str(p, "house_number");
            let area                 = opt_str(p, "area");
            let delivery_note        = opt_str(p, "delivery_note");
            let delivery_staff_name  = opt_str(p, "delivery_staff_name");
            let expected_payment_method = str_field(p, "expected_payment_method")
                .unwrap_or_else(|_| "cash".into());
            let payment_status       = str_field(p, "payment_status")
                .unwrap_or_else(|_| "unpaid".into());
            let delivery_status      = str_field(p, "delivery_status")
                .unwrap_or_else(|_| "pending".into());
            let amount_minor         = i64_field(p, "amount_minor").unwrap_or(0);
            let currency             = str_field(p, "currency").unwrap_or_else(|_| "BHD".into());
            let paid_confirmed_at    = opt_str(p, "paid_confirmed_at");
            let created_by_user_id   = str_field(p, "created_by_user_id").unwrap_or_default();
            let created_at           = str_field(p, "created_at")?;
            let updated_at           = str_field(p, "updated_at")
                .unwrap_or_else(|_| created_at.clone());

            // Insert if new, or update status/payment fields if newer timestamp (LWW)
            sqlx::query(
                "INSERT INTO delivery_orders
                   (delivery_id, sale_id, receipt_number, branch_id, device_id, origin_device_id,
                    customer_id, customer_name, contact_number, address_text,
                    house_number, area, delivery_note, delivery_staff_name,
                    expected_payment_method, payment_status, delivery_status,
                    amount_minor, currency, paid_confirmed_at,
                    created_by_user_id, created_at, updated_at, sync_status, version)
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'synced',1)
                 ON CONFLICT(delivery_id) DO UPDATE SET
                   delivery_status     = excluded.delivery_status,
                   payment_status      = excluded.payment_status,
                   delivery_staff_name = excluded.delivery_staff_name,
                   paid_confirmed_at   = excluded.paid_confirmed_at,
                   updated_at          = excluded.updated_at
                 WHERE datetime(delivery_orders.updated_at) < datetime(excluded.updated_at)"
            )
            .bind(&delivery_id)
            .bind(&sale_id)
            .bind(&receipt_number)
            .bind(&branch_id_field)
            .bind(&device_id_field)
            .bind(&origin_device_id)
            .bind(customer_id)
            .bind(customer_name)
            .bind(&contact_number)
            .bind(&address_text)
            .bind(house_number)
            .bind(area)
            .bind(delivery_note)
            .bind(delivery_staff_name)
            .bind(&expected_payment_method)
            .bind(&payment_status)
            .bind(&delivery_status)
            .bind(amount_minor)
            .bind(&currency)
            .bind(paid_confirmed_at)
            .bind(&created_by_user_id)
            .bind(&created_at)
            .bind(&updated_at)
            .execute(pool)
            .await?;
        }

        // ── Cross-device transactional events (Phase C) ──────────────────────
        // Each device creates its own copy of the entity in sync_events; the
        // central DB is the union. For cross-device reports and stock drift
        // detection, every device must have every other device's events. Each
        // branch is idempotent on PK (UNIQUE constraint absorbs replays).
        "sale" => {
            let sale_id              = str_field(p, "sale_id")?;
            let receipt_number       = str_field(p, "receipt_number")?;
            let branch_id_field      = str_field(p, "branch_id")?;
            let device_id_field      = str_field(p, "device_id")?;
            let origin_device_id     = opt_str(p, "origin_device_id")
                .unwrap_or_else(|| device_id_field.clone());
            let shift_id             = str_field(p, "shift_id")?;
            let cashier_user_id      = str_field(p, "cashier_user_id")?;
            let status               = opt_str(p, "status").unwrap_or_else(|| "completed".into());
            let gross_total_minor    = i64_field(p, "gross_total_minor").unwrap_or(0);
            let discount_total_minor = i64_field(p, "discount_total_minor").unwrap_or(0);
            let tax_total_minor      = i64_field(p, "tax_total_minor").unwrap_or(0);
            let net_total_minor      = i64_field(p, "net_total_minor").unwrap_or(0);
            let currency             = opt_str(p, "currency").unwrap_or_else(|| "BHD".into());
            let business_date        = str_field(p, "business_date")?;
            let sold_at              = str_field(p, "sold_at")?;
            let created_offline      = bool_field(p, "created_offline");
            let idempotency_key      = str_field(p, "idempotency_key").unwrap_or_else(|_| sale_id.clone());

            sqlx::query(
                "INSERT OR IGNORE INTO sales
                 (sale_id, receipt_number, branch_id, device_id, origin_device_id, shift_id,
                  cashier_user_id, status, gross_total_minor, discount_total_minor,
                  tax_total_minor, net_total_minor, currency, business_date,
                  sold_at, created_offline, idempotency_key, sync_status)
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'synced')"
            )
            .bind(&sale_id)
            .bind(&receipt_number)
            .bind(&branch_id_field)
            .bind(&device_id_field)
            .bind(&origin_device_id)
            .bind(&shift_id)
            .bind(&cashier_user_id)
            .bind(&status)
            .bind(gross_total_minor)
            .bind(discount_total_minor)
            .bind(tax_total_minor)
            .bind(net_total_minor)
            .bind(&currency)
            .bind(&business_date)
            .bind(&sold_at)
            .bind(created_offline as i64)
            .bind(&idempotency_key)
            .execute(pool)
            .await?;
        }

        "sale_item" => {
            let sale_item_id          = str_field(p, "sale_item_id")?;
            let sale_id               = str_field(p, "sale_id")?;
            let product_id            = opt_str(p, "product_id");
            let product_name_snapshot = str_field(p, "product_name_snapshot")?;
            let sku_snapshot          = opt_str(p, "sku_snapshot");
            let barcode_snapshot      = opt_str(p, "barcode_snapshot");
            let quantity              = str_field(p, "quantity")?;
            let unit_price_minor      = i64_field(p, "unit_price_minor").unwrap_or(0);
            let line_discount_minor   = i64_field(p, "line_discount_minor").unwrap_or(0);
            let tax_rule_snapshot     = opt_str(p, "tax_rule_snapshot").unwrap_or_else(|| "{}".into());
            let tax_amount_minor      = i64_field(p, "tax_amount_minor").unwrap_or(0);
            let line_total_minor      = i64_field(p, "line_total_minor").unwrap_or(0);
            let note                  = opt_str(p, "note");
            let voided                = bool_field(p, "voided");

            let origin_device_id = opt_str(p, "origin_device_id").unwrap_or_default();

            if let Some(pid) = &product_id { ensure_product(pool, pid).await?; }

            sqlx::query(
                "INSERT OR IGNORE INTO sale_items
                 (sale_item_id, sale_id, origin_device_id, product_id, product_name_snapshot, sku_snapshot,
                  barcode_snapshot, quantity, unit_price_minor, line_discount_minor,
                  tax_rule_snapshot, tax_amount_minor, line_total_minor, note, voided)
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
            )
            .bind(&sale_item_id)
            .bind(&sale_id)
            .bind(&origin_device_id)
            .bind(product_id)
            .bind(&product_name_snapshot)
            .bind(sku_snapshot)
            .bind(barcode_snapshot)
            .bind(&quantity)
            .bind(unit_price_minor)
            .bind(line_discount_minor)
            .bind(&tax_rule_snapshot)
            .bind(tax_amount_minor)
            .bind(line_total_minor)
            .bind(note)
            .bind(voided as i64)
            .execute(pool)
            .await?;
        }

        "payment" => {
            let payment_id           = str_field(p, "payment_id")?;
            let sale_id              = str_field(p, "sale_id")?;
            let device_id_field      = opt_str(p, "device_id").unwrap_or_default();
            let origin_device_id     = opt_str(p, "origin_device_id")
                .unwrap_or_else(|| device_id_field.clone());
            let payment_method       = str_field(p, "payment_method")?;
            let amount_minor         = i64_field(p, "amount_minor").unwrap_or(0);
            let currency             = opt_str(p, "currency").unwrap_or_else(|| "BHD".into());
            let status               = opt_str(p, "status").unwrap_or_else(|| "approved".into());
            let external_reference   = opt_str(p, "external_reference");
            let tendered_minor       = opt_i64(p, "tendered_minor");
            let change_minor         = opt_i64(p, "change_minor");
            let recorded_by_user_id  = str_field(p, "recorded_by_user_id")?;
            let recorded_at          = str_field(p, "recorded_at")?;

            sqlx::query(
                "INSERT OR IGNORE INTO payments
                 (payment_id, sale_id, origin_device_id, payment_method, amount_minor, currency,
                  status, external_reference, tendered_minor, change_minor,
                  recorded_by_user_id, recorded_at, sync_status)
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,'synced')"
            )
            .bind(&payment_id)
            .bind(&sale_id)
            .bind(&origin_device_id)
            .bind(&payment_method)
            .bind(amount_minor)
            .bind(&currency)
            .bind(&status)
            .bind(external_reference)
            .bind(tendered_minor)
            .bind(change_minor)
            .bind(&recorded_by_user_id)
            .bind(&recorded_at)
            .execute(pool)
            .await?;
        }

        "refund" => {
            let refund_id              = str_field(p, "refund_id")?;
            let original_sale_id       = str_field(p, "original_sale_id")?;
            let device_id_field        = opt_str(p, "device_id").unwrap_or_default();
            let origin_device_id       = opt_str(p, "origin_device_id")
                .unwrap_or_else(|| device_id_field.clone());
            let refund_receipt_number  = str_field(p, "refund_receipt_number")?;
            let reason                 = opt_str(p, "reason").unwrap_or_default();
            let return_reason_code     = opt_str(p, "return_reason_code").unwrap_or_else(|| "other".into());
            let refund_total_minor     = i64_field(p, "refund_total_minor").unwrap_or(0);
            let currency               = opt_str(p, "currency").unwrap_or_else(|| "BHD".into());
            let created_by_user_id     = str_field(p, "created_by_user_id")?;
            let created_at             = str_field(p, "created_at")?;
            let idempotency_key        = str_field(p, "idempotency_key").unwrap_or_else(|_| refund_id.clone());

            sqlx::query(
                "INSERT OR IGNORE INTO refunds
                 (refund_id, original_sale_id, origin_device_id, refund_receipt_number, reason,
                  return_reason_code, refund_total_minor, currency, created_by_user_id, created_at,
                  sync_status, idempotency_key)
                 VALUES (?,?,?,?,?,?,?,?,?,?, 'synced', ?)"
            )
            .bind(&refund_id)
            .bind(&original_sale_id)
            .bind(&origin_device_id)
            .bind(&refund_receipt_number)
            .bind(&reason)
            .bind(&return_reason_code)
            .bind(refund_total_minor)
            .bind(&currency)
            .bind(&created_by_user_id)
            .bind(&created_at)
            .bind(&idempotency_key)
            .execute(pool)
            .await?;
        }

        "refund_item" => {
            let refund_item_id        = str_field(p, "refund_item_id")?;
            let refund_id             = str_field(p, "refund_id")?;
            let device_id_field       = opt_str(p, "device_id").unwrap_or_default();
            let origin_device_id      = opt_str(p, "origin_device_id")
                .unwrap_or_else(|| device_id_field.clone());
            let sale_item_id          = str_field(p, "sale_item_id")?;
            let product_name_snapshot = str_field(p, "product_name_snapshot")?;
            let quantity              = str_field(p, "quantity")?;
            let unit_price_minor      = i64_field(p, "unit_price_minor").unwrap_or(0);
            let refund_amount_minor   = i64_field(p, "refund_amount_minor").unwrap_or(0);

            sqlx::query(
                "INSERT OR IGNORE INTO refund_items
                 (refund_item_id, refund_id, origin_device_id, sale_item_id, product_name_snapshot,
                  quantity, unit_price_minor, refund_amount_minor)
                 VALUES (?,?,?,?,?,?,?,?)"
            )
            .bind(&refund_item_id)
            .bind(&refund_id)
            .bind(&origin_device_id)
            .bind(&sale_item_id)
            .bind(&product_name_snapshot)
            .bind(&quantity)
            .bind(unit_price_minor)
            .bind(refund_amount_minor)
            .execute(pool)
            .await?;
        }

        "shift" => {
            let shift_id            = str_field(p, "shift_id")?;
            let branch_id_field     = str_field(p, "branch_id")?;
            let device_id_field     = str_field(p, "device_id")?;
            let origin_device_id    = opt_str(p, "origin_device_id")
                .unwrap_or_else(|| device_id_field.clone());
            let cashier_user_id     = str_field(p, "cashier_user_id")?;
            let opened_at           = str_field(p, "opened_at")?;
            let closed_at           = opt_str(p, "closed_at");
            let opening_cash_minor  = i64_field(p, "opening_cash_minor").unwrap_or(0);
            let counted_cash_minor  = opt_i64(p, "counted_cash_minor");
            let status              = opt_str(p, "status").unwrap_or_else(|| "open".into());
            let close_notes         = opt_str(p, "close_notes");
            let updated_at          = opt_str(p, "updated_at").unwrap_or_else(|| opened_at.clone());

            sqlx::query(
                "INSERT INTO shifts
                 (shift_id, branch_id, device_id, origin_device_id, cashier_user_id,
                  opened_at, closed_at, opening_cash_minor, counted_cash_minor,
                  status, close_notes, sync_status, updated_at)
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,'synced',?)
                 ON CONFLICT(shift_id) DO UPDATE SET
                   closed_at          = excluded.closed_at,
                   counted_cash_minor = COALESCE(excluded.counted_cash_minor, shifts.counted_cash_minor),
                   status             = excluded.status,
                   close_notes        = excluded.close_notes,
                   updated_at         = excluded.updated_at
                 WHERE shifts.updated_at IS NULL OR datetime(shifts.updated_at) < datetime(excluded.updated_at)"
            )
            .bind(&shift_id)
            .bind(&branch_id_field)
            .bind(&device_id_field)
            .bind(&origin_device_id)
            .bind(&cashier_user_id)
            .bind(&opened_at)
            .bind(closed_at)
            .bind(opening_cash_minor)
            .bind(counted_cash_minor)
            .bind(&status)
            .bind(close_notes)
            .bind(&updated_at)
            .execute(pool)
            .await?;
        }

        "audit_log" => {
            let audit_log_id    = str_field(p, "audit_log_id")?;
            let event_type      = str_field(p, "event_type")?;
            let entity_type     = str_field(p, "entity_type")?;
            let entity_id       = str_field(p, "entity_id")?;
            let actor_user_id   = opt_str(p, "actor_user_id");
            let actor_type      = opt_str(p, "actor_type").unwrap_or_else(|| "user".into());
            let ai_action_id    = opt_str(p, "ai_action_id");
            let device_id_field = opt_str(p, "device_id").unwrap_or_default();
            let origin_device_id = opt_str(p, "origin_device_id")
                .unwrap_or_else(|| device_id_field.clone());
            let branch_id_field = opt_str(p, "branch_id");
            let before_json     = opt_str(p, "before_json");
            let after_json      = opt_str(p, "after_json");
            let reason          = opt_str(p, "reason");
            let created_at      = str_field(p, "created_at")?;
            let hash            = opt_str(p, "hash").unwrap_or_default();
            let previous_hash   = opt_str(p, "previous_hash");

            sqlx::query(
                "INSERT OR IGNORE INTO audit_logs
                 (audit_log_id, event_type, entity_type, entity_id,
                  actor_user_id, actor_type, ai_action_id, device_id, origin_device_id, branch_id,
                  before_json, after_json, reason, created_at, hash, previous_hash)
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
            )
            .bind(&audit_log_id)
            .bind(&event_type)
            .bind(&entity_type)
            .bind(&entity_id)
            .bind(actor_user_id)
            .bind(&actor_type)
            .bind(ai_action_id)
            .bind(&device_id_field)
            .bind(&origin_device_id)
            .bind(branch_id_field)
            .bind(before_json)
            .bind(after_json)
            .bind(reason)
            .bind(&created_at)
            .bind(&hash)
            .bind(previous_hash)
            .execute(pool)
            .await?;
        }

        other => {
            tracing::warn!("inbox: unknown entity_type '{}', skipping", other);
        }
    }

    Ok(())
}

/// Recompute `stock_levels.quantity_on_hand` for a (product, branch) by
/// summing the `stock_movements` ledger. The ledger is the source of truth;
/// the cache is a derived projection. After every remote movement apply,
/// every device converges to the same value.
///
/// Drift is detected when the local cached value differs from the ledger sum
/// by more than `STOCK_DRIFT_TOLERANCE`. The drift is recorded in sync_queue
/// as a `stock_drift_detected` event so the manager dashboard can surface it.
const STOCK_DRIFT_TOLERANCE: f64 = 0.001;

async fn recompute_stock_level(
    pool: &SqlitePool,
    product_id: &str,
    branch_id: &str,
    applied_at: &str,
) -> AppResult<()> {
    // Sum quantity_delta from the ledger. quantity_delta is stored as text
    // to preserve decimal precision (BHD uses 3 decimals, but stock may use
    // 1-3 depending on product unit). CAST to REAL for SUM.
    let ledger_sum: Option<f64> = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(CAST(quantity_delta AS REAL)), 0) AS REAL)
         FROM stock_movements
         WHERE product_id = ? AND branch_id = ?",
    )
    .bind(product_id)
    .bind(branch_id)
    .fetch_one(pool)
    .await?;

    // Read current cached level
    let cached: Option<f64> = sqlx::query_scalar(
        "SELECT CAST(quantity_on_hand AS REAL) FROM stock_levels
         WHERE product_id = ? AND branch_id = ?",
    )
    .bind(product_id)
    .bind(branch_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    let ledger = ledger_sum.unwrap_or(0.0);
    let drift = match cached {
        Some(c) => (ledger - c).abs() > STOCK_DRIFT_TOLERANCE,
        None    => false, // first time we see this row — no drift, just init
    };

    // Upsert the cached level to match the ledger
    let stock_level_id = format!("SL-{}-{}", product_id, branch_id);
    sqlx::query(
        "INSERT INTO stock_levels
            (stock_level_id, product_id, branch_id, quantity_on_hand, last_movement_at, updated_at)
         VALUES (?,?,?,?,?,?)
         ON CONFLICT(product_id, branch_id) DO UPDATE SET
            quantity_on_hand = excluded.quantity_on_hand,
            last_movement_at = excluded.last_movement_at,
            updated_at       = excluded.updated_at"
    )
    .bind(&stock_level_id)
    .bind(product_id)
    .bind(branch_id)
    .bind(format_qty(ledger))
    .bind(applied_at)
    .bind(applied_at)
    .execute(pool)
    .await?;

    if drift {
        // Emit a drift event into the outbox. The manager dashboard surfaces
        // these for review. The drift is a fact (ledger is the source of
        // truth); the manager can choose to keep the ledger sum.
        let _ = outbox_drift_event(pool, product_id, branch_id, cached, ledger, applied_at).await;
    }

    Ok(())
}

fn format_qty(q: f64) -> String {
    // Match the local convention: trim trailing zeros, keep at least 0.001 precision
    format!("{:.3}", q).trim_end_matches('0').trim_end_matches('.').to_string()
}

async fn outbox_drift_event(
    pool: &SqlitePool,
    product_id: &str,
    branch_id: &str,
    cached: Option<f64>,
    ledger: f64,
    applied_at: &str,
) -> AppResult<()> {
    use serde_json::json;
    use ulid::Ulid;

    let device_id: String = sqlx::query_scalar(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .unwrap_or_default();
    let branch_id_field: String = sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .unwrap_or_default();

    if device_id.is_empty() || branch_id_field.is_empty() {
        return Ok(()); // no active device/branch yet — skip
    }

    let drift_id = Ulid::new().to_string();
    let payload = json!({
        "drift_id":    drift_id,
        "product_id":  product_id,
        "branch_id":   branch_id,
        "cached_qty":  cached,
        "ledger_qty":  ledger,
        "delta":       ledger - cached.unwrap_or(0.0),
        "tolerance":   STOCK_DRIFT_TOLERANCE,
        "detected_at": applied_at,
        "device_id":   device_id,
    });
    let now = chrono::Utc::now().to_rfc3339();

    let mut tx = pool.begin().await?;
    let seq: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(local_sequence), 0) + 1 FROM sync_queue WHERE device_id = ?",
    )
    .bind(&device_id)
    .fetch_one(&mut *tx)
    .await?;
    let idem = format!("stock_drift-{}", drift_id);
    let payload_str = serde_json::to_string(&payload).unwrap_or_default();
    let payload_hash = hex::encode(Sha256::digest(payload_str.as_bytes()));
    sqlx::query(
        "INSERT OR IGNORE INTO sync_queue
         (sync_event_id, device_id, branch_id, entity_type, entity_id, operation,
          payload_json, payload_hash, idempotency_key, local_sequence, created_at, status)
         VALUES (?,?,?, 'stock_drift_detected', ?, 'detected', ?, ?, ?, ?, ?, 'pending')"
    )
    .bind(Ulid::new().to_string())
    .bind(&device_id)
    .bind(&branch_id_field)
    .bind(&drift_id)
    .bind(&payload_str)
    .bind(&payload_hash)
    .bind(&idem)
    .bind(seq)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

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
        .map(|v| {
            v.as_bool()
                .unwrap_or_else(|| v.as_i64().map(|n| n != 0).unwrap_or(false))
        })
        .unwrap_or(false)
}

fn i64_field(p: &serde_json::Value, key: &str) -> Result<i64, crate::errors::AppError> {
    p.get(key).and_then(|v| v.as_i64()).ok_or_else(|| {
        crate::errors::AppError::Internal(format!("inbox: missing i64 field '{key}'"))
    })
}

fn opt_i64(p: &serde_json::Value, key: &str) -> Option<i64> {
    p.get(key).and_then(|v| v.as_i64())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests — multi-terminal apply correctness
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::supabase_client::SyncEventRow;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn make_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        sqlx::migrate!("./migrations").run(&pool).await.expect("migrations");
        pool
    }

    fn user_event(seq: i64, user_id: &str, username: &str, updated_at: &str) -> SyncEventRow {
        SyncEventRow {
            global_sequence: seq,
            device_id: "01JDEVICE0000000000000002".into(),
            branch_id: "01JBRANCH0000000000000001".into(),
            entity_type: "user".into(),
            entity_id: user_id.into(),
            operation: "update".into(),
            payload_json: serde_json::json!({
                "user_id": user_id,
                "display_name": "Remote Owner",
                "username": username,
                "role_id": "01JROLE00000000000OWNER001",
                "branch_scope": "[]",
                "is_active": true,
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": updated_at,
                "version": 2,
            }),
            idempotency_key: format!("user-{user_id}-{updated_at}"),
            local_sequence: seq,
            created_at: updated_at.into(),
        }
    }

    /// The bug that left joining terminals empty: a remote owner whose username
    /// ('admin') collides with the local seeded 'admin' (a DIFFERENT user_id) must
    /// still apply — the username UNIQUE index must not make apply_event fail.
    #[tokio::test]
    async fn remote_user_with_colliding_username_applies() {
        let pool = make_pool().await;

        // Resolve the seeded admin's user_id + a valid role for a realistic remote id.
        let seed_admin: Option<String> =
            sqlx::query_scalar("SELECT user_id FROM users WHERE username = 'admin'")
                .fetch_optional(&pool).await.unwrap();
        // If the seed exists it owns 'admin'. Simulate device 1's owner being a
        // DIFFERENT user_id that also claims username 'admin'.
        let remote_id = "01JUSER0000000000REMOTE01";
        assert_ne!(seed_admin.as_deref(), Some(remote_id));

        let ev = user_event(10, remote_id, "admin", "2026-06-02T12:00:00Z");
        // Must NOT error despite the username collision with the local seed.
        apply_event(&pool, &ev).await.expect("apply must succeed despite username collision");

        // The remote owner now exists, is active, and uniquely owns 'admin'.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE username = 'admin'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 1, "exactly one user owns 'admin' after reconcile");

        let active_remote: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE user_id = ? AND is_active = 1",
        )
        .bind(remote_id)
        .fetch_one(&pool).await.unwrap();
        assert_eq!(active_remote, 1, "remote owner must be present and active");
    }

    /// Applying the same user event twice is idempotent (no error, no duplicate).
    #[tokio::test]
    async fn remote_user_apply_is_idempotent() {
        let pool = make_pool().await;
        let ev = user_event(11, "01JUSER0000000000REMOTE02", "cashier_b", "2026-06-02T12:00:00Z");
        apply_event(&pool, &ev).await.expect("first apply");
        apply_event(&pool, &ev).await.expect("second apply (idempotent)");
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE user_id = '01JUSER0000000000REMOTE02'",
        )
        .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 1);
    }

    /// Regression (the seq-355 poison): a `product_price` whose product hasn't been
    /// pulled yet — and a `product` whose category hasn't been pulled yet — must
    /// BOTH apply (against hidden stubs) instead of throwing a FOREIGN KEY error
    /// that halts the inbox watermark and strands the rest of the catalog. The real
    /// parent events then overwrite the stubs via last-write-wins.
    #[tokio::test]
    async fn out_of_order_catalog_does_not_halt_on_fk() {
        let pool = make_pool().await;
        let product_id = "01JPRODOUTOFORDER00000001";
        let price_id = "PRC-01JOUTOFORDER000000001";

        // 1) Price lands BEFORE its product (the real-world poison at seq 355).
        let price_ev = SyncEventRow {
            global_sequence: 355,
            device_id: "01JDEVICE0000000000000001".into(),
            branch_id: "01JBRANCH0000000000000001".into(),
            entity_type: "product_price".into(),
            entity_id: price_id.into(),
            operation: "insert".into(),
            payload_json: serde_json::json!({
                "price_id": price_id,
                "product_id": product_id,
                "price_type": "selling",
                "price_minor": 1500,
                "currency": "BHD",
                "effective_from": "2026-01-01T00:00:00Z",
                "created_by_user_id": "01JUSER000000000000ADMIN1",
                "created_at": "2026-01-01T00:00:00Z",
            }),
            idempotency_key: format!("price-{price_id}"),
            local_sequence: 355,
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        apply_event(&pool, &price_ev).await.expect("price before product must NOT FK-halt");

        let price_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM product_prices WHERE price_id = ?")
                .bind(price_id).fetch_one(&pool).await.unwrap();
        assert_eq!(price_count, 1, "price stored against a stub product");
        let stub_active: Option<i64> =
            sqlx::query_scalar("SELECT is_active FROM products WHERE product_id = ?")
                .bind(product_id).fetch_optional(&pool).await.unwrap();
        assert_eq!(stub_active, Some(0), "stub product exists and is hidden");

        // 2) The real product arrives later, itself referencing an unseen category.
        let prod_ev = SyncEventRow {
            global_sequence: 400,
            device_id: "01JDEVICE0000000000000001".into(),
            branch_id: "01JBRANCH0000000000000001".into(),
            entity_type: "product".into(),
            entity_id: product_id.into(),
            operation: "insert".into(),
            payload_json: serde_json::json!({
                "product_id": product_id,
                "category_id": "01JCATOUTOFORDER000000001",
                "name": "Real Product",
                "track_inventory": true,
                "allow_decimal_quantity": false,
                "is_active": true,
                "currency": "BHD",
                "created_at": "2026-06-01T00:00:00Z",
                "updated_at": "2026-06-01T00:00:00Z",
                "version": 1,
            }),
            idempotency_key: format!("prod-{product_id}"),
            local_sequence: 400,
            created_at: "2026-06-01T00:00:00Z".into(),
        };
        apply_event(&pool, &prod_ev).await.expect("product before category must NOT FK-halt");

        let (name, active): (String, i64) =
            sqlx::query_as("SELECT name, is_active FROM products WHERE product_id = ?")
                .bind(product_id).fetch_one(&pool).await.unwrap();
        assert_eq!(name, "Real Product", "real product overwrote the stub via LWW");
        assert_eq!(active, 1, "real product is active");
    }
}
