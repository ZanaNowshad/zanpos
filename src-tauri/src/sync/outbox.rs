use crate::errors::AppResult;
use serde_json::Value;
/// Outbox helpers — write full-payload sync_queue entries.
/// Called from repos after every entity write that must sync.
/// Each function returns the next local_sequence used, for callers that chain entries.
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use ulid::Ulid;

// ── Low-level enqueue ──────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_raw(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    entity_type: &str,
    entity_id: &str,
    operation: &str,
    payload: Value,
    idem_suffix: &str, // appended to entity_id to form idempotency_key
) -> AppResult<i64> {
    let sync_event_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // Wrap read-then-write in a transaction to prevent sequence races.
    // Without a transaction, two concurrent enqueue calls can read the same
    // MAX(local_sequence) and both attempt the same sequence number, causing
    // one event to be silently dropped by INSERT OR IGNORE.
    let mut tx = pool.begin().await?;

    let seq: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(local_sequence), 0) + 1 FROM sync_queue WHERE device_id = ?",
    )
    .bind(device_id)
    .fetch_one(&mut *tx)
    .await?;

    let payload_str = payload.to_string();
    // M10: SHA-256 replaces CRC32 (32-bit collision space too small for 100K+ events)
    let payload_hash = sha256_hex(&payload_str);
    let idempotency_key = format!("{}-{}-{}", entity_type, entity_id, idem_suffix);

    sqlx::query(
        "INSERT OR IGNORE INTO sync_queue
         (sync_event_id, device_id, branch_id, entity_type, entity_id, operation,
          payload_json, payload_hash, idempotency_key, local_sequence, created_at, status)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,'pending')",
    )
    .bind(&sync_event_id)
    .bind(device_id)
    .bind(branch_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(operation)
    .bind(&payload_str)
    .bind(&payload_hash)
    .bind(&idempotency_key)
    .bind(seq)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(seq)
}

/// Transaction-aware variant of enqueue_raw.
/// Call this when you need the sync_queue INSERT to be in the SAME
/// BEGIN/COMMIT as the parent mutation. The caller owns the transaction
/// and is responsible for commit/rollback.
#[allow(clippy::too_many_arguments)]
pub async fn enqueue_raw_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    entity_type: &str,
    entity_id: &str,
    operation: &str,
    payload: Value,
    idem_suffix: &str,
) -> AppResult<i64> {
    let sync_event_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let seq: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(local_sequence), 0) + 1 FROM sync_queue WHERE device_id = ?",
    )
    .bind(device_id)
    .fetch_one(&mut **tx)
    .await?;

    let payload_str = payload.to_string();
    let payload_hash = sha256_hex(&payload_str);
    let idempotency_key = format!("{}-{}-{}", entity_type, entity_id, idem_suffix);

    sqlx::query(
        "INSERT OR IGNORE INTO sync_queue
         (sync_event_id, device_id, branch_id, entity_type, entity_id, operation,
          payload_json, payload_hash, idempotency_key, local_sequence, created_at, status)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,'pending')",
    )
    .bind(&sync_event_id)
    .bind(device_id)
    .bind(branch_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(operation)
    .bind(&payload_str)
    .bind(&payload_hash)
    .bind(&idempotency_key)
    .bind(seq)
    .bind(&now)
    .execute(&mut **tx)
    .await?;

    Ok(seq)
}

/// SHA-256 hex digest of a string — replaces the old CRC32 payload hash.
/// sha2 is already a dependency (used by audit_hash), so no new crate needed.
fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

// ── High-level enqueue helpers ─────────────────────────────────────────────────
// These helpers have many arguments by design (they map 1:1 to DB columns for
// sync payloads). Clippy's too_many_arguments lint is suppressed intentionally.
// enqueue_audit_log is prepared for Phase 3 audit-trail sync; unused until then.

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_sale(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    sale_id: &str,
    receipt_number: &str,
    shift_id: &str,
    cashier_user_id: &str,
    status: &str,
    gross_total_minor: i64,
    discount_total_minor: i64,
    tax_total_minor: i64,
    net_total_minor: i64,
    currency: &str,
    business_date: &str,
    sold_at: &str,
    created_offline: bool,
    idempotency_key: &str,
    customer_id: Option<&str>,
    is_delivery: bool,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "sale_id":              sale_id,
        "receipt_number":       receipt_number,
        "branch_id":            branch_id,
        "device_id":            device_id,
        "origin_device_id":     device_id,
        "shift_id":             shift_id,
        "cashier_user_id":      cashier_user_id,
        "status":               status,
        "gross_total_minor":    gross_total_minor,
        "discount_total_minor": discount_total_minor,
        "tax_total_minor":      tax_total_minor,
        "net_total_minor":      net_total_minor,
        "currency":             currency,
        "business_date":        business_date,
        "sold_at":              sold_at,
        "created_offline":      created_offline,
        "idempotency_key":      idempotency_key,
        "customer_id":          customer_id,
        "is_delivery":          is_delivery,
    });
    enqueue_raw(
        pool, device_id, branch_id, "sale", sale_id, "create", payload, "v1",
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_sale_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    sale_id: &str,
    receipt_number: &str,
    shift_id: &str,
    cashier_user_id: &str,
    status: &str,
    gross_total_minor: i64,
    discount_total_minor: i64,
    tax_total_minor: i64,
    net_total_minor: i64,
    currency: &str,
    business_date: &str,
    sold_at: &str,
    created_offline: bool,
    idempotency_key: &str,
    customer_id: Option<&str>,
    is_delivery: bool,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "sale_id":              sale_id,
        "receipt_number":       receipt_number,
        "branch_id":            branch_id,
        "device_id":            device_id,
        "origin_device_id":     device_id,
        "shift_id":             shift_id,
        "cashier_user_id":      cashier_user_id,
        "status":               status,
        "gross_total_minor":    gross_total_minor,
        "discount_total_minor": discount_total_minor,
        "tax_total_minor":      tax_total_minor,
        "net_total_minor":      net_total_minor,
        "currency":             currency,
        "business_date":        business_date,
        "sold_at":              sold_at,
        "created_offline":      created_offline,
        "idempotency_key":      idempotency_key,
        "customer_id":          customer_id,
        "is_delivery":          is_delivery,
    });
    enqueue_raw_in_tx(
        tx, device_id, branch_id, "sale", sale_id, "create", payload, "v1",
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_sale_item(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    sale_item_id: &str,
    sale_id: &str,
    product_id: Option<&str>,
    product_name_snapshot: &str,
    sku_snapshot: Option<&str>,
    barcode_snapshot: Option<&str>,
    quantity: &str,
    unit_price_minor: i64,
    line_discount_minor: i64,
    tax_rule_snapshot: &str,
    tax_amount_minor: i64,
    line_total_minor: i64,
    note: Option<&str>,
    voided: bool,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "sale_item_id":          sale_item_id,
        "sale_id":               sale_id,
        "product_id":            product_id,
        "product_name_snapshot": product_name_snapshot,
        "sku_snapshot":          sku_snapshot,
        "barcode_snapshot":      barcode_snapshot,
        "quantity":              quantity,
        "unit_price_minor":      unit_price_minor,
        "line_discount_minor":   line_discount_minor,
        "tax_rule_snapshot":     tax_rule_snapshot,
        "tax_amount_minor":      tax_amount_minor,
        "line_total_minor":      line_total_minor,
        "note":                  note,
        "voided":                voided,
        "device_id":             device_id,
        "origin_device_id":      device_id,
        "branch_id":             branch_id,
    });
    enqueue_raw(
        pool,
        device_id,
        branch_id,
        "sale_item",
        sale_item_id,
        "append",
        payload,
        "v1",
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_sale_item_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    sale_item_id: &str,
    sale_id: &str,
    product_id: Option<&str>,
    product_name_snapshot: &str,
    sku_snapshot: Option<&str>,
    barcode_snapshot: Option<&str>,
    quantity: &str,
    unit_price_minor: i64,
    line_discount_minor: i64,
    tax_rule_snapshot: &str,
    tax_amount_minor: i64,
    line_total_minor: i64,
    note: Option<&str>,
    voided: bool,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "sale_item_id":          sale_item_id,
        "sale_id":               sale_id,
        "product_id":            product_id,
        "product_name_snapshot": product_name_snapshot,
        "sku_snapshot":          sku_snapshot,
        "barcode_snapshot":      barcode_snapshot,
        "quantity":              quantity,
        "unit_price_minor":      unit_price_minor,
        "line_discount_minor":   line_discount_minor,
        "tax_rule_snapshot":     tax_rule_snapshot,
        "tax_amount_minor":      tax_amount_minor,
        "line_total_minor":      line_total_minor,
        "note":                  note,
        "voided":                voided,
        "device_id":             device_id,
        "origin_device_id":      device_id,
        "branch_id":             branch_id,
    });
    enqueue_raw_in_tx(
        tx,
        device_id,
        branch_id,
        "sale_item",
        sale_item_id,
        "append",
        payload,
        "v1",
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_payment(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    payment_id: &str,
    sale_id: &str,
    payment_method: &str,
    amount_minor: i64,
    currency: &str,
    status: &str,
    external_reference: Option<&str>,
    tendered_minor: Option<i64>,
    change_minor: Option<i64>,
    recorded_by_user_id: &str,
    recorded_at: &str,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "payment_id":           payment_id,
        "sale_id":              sale_id,
        "payment_method":       payment_method,
        "amount_minor":         amount_minor,
        "currency":             currency,
        "status":               status,
        "external_reference":   external_reference,
        "tendered_minor":       tendered_minor,
        "change_minor":         change_minor,
        "recorded_by_user_id":  recorded_by_user_id,
        "recorded_at":          recorded_at,
        "device_id":            device_id,
        "origin_device_id":     device_id,
        "branch_id":            branch_id,
    });
    enqueue_raw(
        pool, device_id, branch_id, "payment", payment_id, "append", payload, "v1",
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_payment_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    payment_id: &str,
    sale_id: &str,
    payment_method: &str,
    amount_minor: i64,
    currency: &str,
    status: &str,
    external_reference: Option<&str>,
    tendered_minor: Option<i64>,
    change_minor: Option<i64>,
    recorded_by_user_id: &str,
    recorded_at: &str,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "payment_id":           payment_id,
        "sale_id":              sale_id,
        "payment_method":       payment_method,
        "amount_minor":         amount_minor,
        "currency":             currency,
        "status":               status,
        "external_reference":   external_reference,
        "tendered_minor":       tendered_minor,
        "change_minor":         change_minor,
        "recorded_by_user_id":  recorded_by_user_id,
        "recorded_at":          recorded_at,
        "device_id":            device_id,
        "origin_device_id":     device_id,
        "branch_id":            branch_id,
    });
    enqueue_raw_in_tx(
        tx, device_id, branch_id, "payment", payment_id, "append", payload, "v1",
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_shift(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    shift_id: &str,
    cashier_user_id: &str,
    opened_at: &str,
    closed_at: Option<&str>,
    opening_cash_minor: i64,
    counted_cash_minor: Option<i64>,
    status: &str,
    close_notes: Option<&str>,
    updated_at: &str,
    operation: &str, // "create" or "update"
) -> AppResult<()> {
    let payload = serde_json::json!({
        "shift_id":             shift_id,
        "branch_id":            branch_id,
        "device_id":            device_id,
        "origin_device_id":     device_id,
        "cashier_user_id":      cashier_user_id,
        "opened_at":            opened_at,
        "closed_at":            closed_at,
        "opening_cash_minor":   opening_cash_minor,
        "counted_cash_minor":   counted_cash_minor,
        "status":               status,
        "close_notes":          close_notes,
        "updated_at":           updated_at,
    });
    enqueue_raw(
        pool, device_id, branch_id, "shift", shift_id, operation, payload, updated_at,
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_shift_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    shift_id: &str,
    cashier_user_id: &str,
    opened_at: &str,
    closed_at: Option<&str>,
    opening_cash_minor: i64,
    counted_cash_minor: Option<i64>,
    status: &str,
    close_notes: Option<&str>,
    updated_at: &str,
    operation: &str,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "shift_id":             shift_id,
        "branch_id":            branch_id,
        "device_id":            device_id,
        "origin_device_id":     device_id,
        "cashier_user_id":      cashier_user_id,
        "opened_at":            opened_at,
        "closed_at":            closed_at,
        "opening_cash_minor":   opening_cash_minor,
        "counted_cash_minor":   counted_cash_minor,
        "status":               status,
        "close_notes":          close_notes,
        "updated_at":           updated_at,
    });
    enqueue_raw_in_tx(
        tx, device_id, branch_id, "shift", shift_id, operation, payload, updated_at,
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_refund(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    refund_id: &str,
    original_sale_id: &str,
    refund_receipt_number: &str,
    reason: &str,
    refund_total_minor: i64,
    currency: &str,
    created_by_user_id: &str,
    created_at: &str,
    idempotency_key: &str,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "refund_id":              refund_id,
        "original_sale_id":       original_sale_id,
        "refund_receipt_number":  refund_receipt_number,
        "reason":                 reason,
        "refund_total_minor":     refund_total_minor,
        "currency":               currency,
        "created_by_user_id":     created_by_user_id,
        "created_at":             created_at,
        "idempotency_key":        idempotency_key,
        "device_id":              device_id,
        "origin_device_id":       device_id,
        "branch_id":              branch_id,
    });
    enqueue_raw(
        pool, device_id, branch_id, "refund", refund_id, "create", payload, "v1",
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_refund_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    refund_id: &str,
    original_sale_id: &str,
    refund_receipt_number: &str,
    reason: &str,
    refund_total_minor: i64,
    currency: &str,
    created_by_user_id: &str,
    created_at: &str,
    idempotency_key: &str,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "refund_id":              refund_id,
        "original_sale_id":       original_sale_id,
        "refund_receipt_number":  refund_receipt_number,
        "reason":                 reason,
        "refund_total_minor":     refund_total_minor,
        "currency":               currency,
        "created_by_user_id":     created_by_user_id,
        "created_at":             created_at,
        "idempotency_key":        idempotency_key,
        "device_id":              device_id,
        "origin_device_id":       device_id,
        "branch_id":              branch_id,
    });
    enqueue_raw_in_tx(
        tx, device_id, branch_id, "refund", refund_id, "create", payload, "v1",
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_refund_item(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    refund_item_id: &str,
    refund_id: &str,
    sale_item_id: &str,
    product_name_snapshot: &str,
    quantity: &str,
    unit_price_minor: i64,
    refund_amount_minor: i64,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "refund_item_id":        refund_item_id,
        "refund_id":             refund_id,
        "sale_item_id":          sale_item_id,
        "product_name_snapshot": product_name_snapshot,
        "quantity":              quantity,
        "unit_price_minor":      unit_price_minor,
        "refund_amount_minor":   refund_amount_minor,
        "device_id":             device_id,
        "origin_device_id":      device_id,
        "branch_id":             branch_id,
    });
    enqueue_raw(
        pool,
        device_id,
        branch_id,
        "refund_item",
        refund_item_id,
        "append",
        payload,
        "v1",
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_refund_item_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    refund_item_id: &str,
    refund_id: &str,
    sale_item_id: &str,
    product_name_snapshot: &str,
    quantity: &str,
    unit_price_minor: i64,
    refund_amount_minor: i64,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "refund_item_id":        refund_item_id,
        "refund_id":             refund_id,
        "sale_item_id":          sale_item_id,
        "product_name_snapshot": product_name_snapshot,
        "quantity":              quantity,
        "unit_price_minor":      unit_price_minor,
        "refund_amount_minor":   refund_amount_minor,
        "device_id":             device_id,
        "origin_device_id":      device_id,
        "branch_id":             branch_id,
    });
    enqueue_raw_in_tx(
        tx,
        device_id,
        branch_id,
        "refund_item",
        refund_item_id,
        "append",
        payload,
        "v1",
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_product(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    product_id: &str,
    category_id: &str,
    name: &str,
    sku: Option<&str>,
    barcode: Option<&str>,
    description: Option<&str>,
    track_inventory: bool,
    allow_decimal_quantity: bool,
    is_active: bool,
    tax_rule_id: Option<&str>,
    cost_minor: Option<i64>,
    currency: &str,
    reorder_point: i64,
    image_path: Option<&str>,
    default_supplier_id: Option<&str>,
    created_at: &str,
    updated_at: &str,
    version: i64,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "product_id":              product_id,
        "category_id":             category_id,
        "name":                    name,
        "sku":                     sku,
        "barcode":                 barcode,
        "description":             description,
        "track_inventory":         track_inventory,
        "allow_decimal_quantity":  allow_decimal_quantity,
        "is_active":               is_active,
        "tax_rule_id":             tax_rule_id,
        "cost_minor":              cost_minor,
        "currency":                currency,
        "reorder_point":           reorder_point,
        "image_path":              image_path,
        "default_supplier_id":     default_supplier_id,
        "created_at":              created_at,
        "updated_at":              updated_at,
        "version":                 version,
        "device_id":               device_id,
        "branch_id":               branch_id,
    });
    enqueue_raw(
        pool, device_id, branch_id, "product", product_id, "update", payload, updated_at,
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_product_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    product_id: &str,
    category_id: &str,
    name: &str,
    sku: Option<&str>,
    barcode: Option<&str>,
    description: Option<&str>,
    track_inventory: bool,
    allow_decimal_quantity: bool,
    is_active: bool,
    tax_rule_id: Option<&str>,
    cost_minor: Option<i64>,
    currency: &str,
    reorder_point: i64,
    image_path: Option<&str>,
    default_supplier_id: Option<&str>,
    created_at: &str,
    updated_at: &str,
    version: i64,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "product_id":              product_id,
        "category_id":             category_id,
        "name":                    name,
        "sku":                     sku,
        "barcode":                 barcode,
        "description":             description,
        "track_inventory":         track_inventory,
        "allow_decimal_quantity":  allow_decimal_quantity,
        "is_active":               is_active,
        "tax_rule_id":             tax_rule_id,
        "cost_minor":              cost_minor,
        "currency":                currency,
        "reorder_point":           reorder_point,
        "image_path":              image_path,
        "default_supplier_id":     default_supplier_id,
        "created_at":              created_at,
        "updated_at":              updated_at,
        "version":                 version,
        "device_id":               device_id,
        "branch_id":               branch_id,
    });
    enqueue_raw_in_tx(
        tx, device_id, branch_id, "product", product_id, "update", payload, updated_at,
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_product_price(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    price_id: &str,
    product_id: &str,
    price_minor: i64,
    currency: &str,
    effective_from: &str,
    effective_to: Option<&str>,
    created_by_user_id: &str,
    created_by_ai_action_id: Option<&str>,
    created_at: &str,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "price_id":               price_id,
        "product_id":             product_id,
        "branch_id":              branch_id,
        "price_type":             "selling",
        "price_minor":            price_minor,
        "currency":               currency,
        "effective_from":         effective_from,
        "effective_to":           effective_to,
        "created_by_user_id":     created_by_user_id,
        "created_by_ai_action_id": created_by_ai_action_id,
        "created_at":             created_at,
        "device_id":              device_id,
    });
    enqueue_raw(
        pool,
        device_id,
        branch_id,
        "product_price",
        price_id,
        "append",
        payload,
        "v1",
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_product_price_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    price_id: &str,
    product_id: &str,
    price_minor: i64,
    currency: &str,
    effective_from: &str,
    effective_to: Option<&str>,
    created_by_user_id: &str,
    created_by_ai_action_id: Option<&str>,
    created_at: &str,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "price_id":               price_id,
        "product_id":             product_id,
        "branch_id":              branch_id,
        "price_type":             "selling",
        "price_minor":            price_minor,
        "currency":               currency,
        "effective_from":         effective_from,
        "effective_to":           effective_to,
        "created_by_user_id":     created_by_user_id,
        "created_by_ai_action_id": created_by_ai_action_id,
        "created_at":             created_at,
        "device_id":              device_id,
    });
    enqueue_raw_in_tx(
        tx,
        device_id,
        branch_id,
        "product_price",
        price_id,
        "append",
        payload,
        "v1",
    )
    .await?;
    Ok(())
}

pub async fn enqueue_category(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    category_id: &str,
    name: &str,
    sort_order: i64,
    is_active: bool,
    parent_category_id: Option<&str>,
    created_at: &str,
    updated_at: &str,
    version: i64,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "category_id":        category_id,
        "parent_category_id": parent_category_id,
        "name":               name,
        "sort_order":         sort_order,
        "is_active":          is_active,
        "created_at":         created_at,
        "updated_at":         updated_at,
        "version":            version,
        "device_id":          device_id,
        "branch_id":          branch_id,
    });
    enqueue_raw(pool, device_id, branch_id, "category", category_id, "update", payload, updated_at)
        .await?;
    Ok(())
}

pub async fn enqueue_category_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    category_id: &str,
    name: &str,
    sort_order: i64,
    is_active: bool,
    parent_category_id: Option<&str>,
    created_at: &str,
    updated_at: &str,
    version: i64,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "category_id":        category_id,
        "parent_category_id": parent_category_id,
        "name":               name,
        "sort_order":         sort_order,
        "is_active":          is_active,
        "created_at":         created_at,
        "updated_at":         updated_at,
        "version":            version,
        "device_id":          device_id,
        "branch_id":          branch_id,
    });
    enqueue_raw_in_tx(tx, device_id, branch_id, "category", category_id, "update", payload, updated_at)
        .await?;
    Ok(())
}

pub async fn enqueue_tax_rule(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    tax_rule_id: &str,
    name: &str,
    rate_basis_points: i64,
    inclusive: bool,
    is_active: bool,
    effective_from: &str,
    updated_at: &str, // used as idem_suffix so updates generate distinct keys from the initial insert
    version: i64,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "tax_rule_id":       tax_rule_id,
        "name":              name,
        "rate_basis_points": rate_basis_points,
        "inclusive":         inclusive,
        "is_active":         is_active,
        "effective_from":    effective_from,
        "updated_at":        updated_at,
        "version":           version,
        "device_id":         device_id,
        "branch_id":         branch_id,
    });
    enqueue_raw(pool, device_id, branch_id, "tax_rule", tax_rule_id, "update", payload, updated_at)
        .await?;
    Ok(())
}

pub async fn enqueue_tax_rule_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    tax_rule_id: &str,
    name: &str,
    rate_basis_points: i64,
    inclusive: bool,
    is_active: bool,
    effective_from: &str,
    updated_at: &str,
    version: i64,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "tax_rule_id":       tax_rule_id,
        "name":              name,
        "rate_basis_points": rate_basis_points,
        "inclusive":         inclusive,
        "is_active":         is_active,
        "effective_from":    effective_from,
        "updated_at":        updated_at,
        "version":           version,
        "device_id":         device_id,
        "branch_id":         branch_id,
    });
    enqueue_raw_in_tx(tx, device_id, branch_id, "tax_rule", tax_rule_id, "update", payload, updated_at)
        .await?;
    Ok(())
}

pub async fn enqueue_user(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    user_id: &str,
    display_name: &str,
    username: &str,
    role_id: &str,
    branch_scope: &str,
    is_active: bool,
    created_at: &str,
    updated_at: &str,
    version: i64,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "user_id":      user_id,
        "display_name": display_name,
        "username":     username,
        "role_id":      role_id,
        "branch_scope": branch_scope,
        "is_active":    is_active,
        "created_at":   created_at,
        "updated_at":   updated_at,
        "version":      version,
        "device_id":    device_id,
        "branch_id":    branch_id,
    });
    enqueue_raw(pool, device_id, branch_id, "user", user_id, "update", payload, updated_at)
        .await?;
    Ok(())
}

pub async fn enqueue_user_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    user_id: &str,
    display_name: &str,
    username: &str,
    role_id: &str,
    branch_scope: &str,
    is_active: bool,
    created_at: &str,
    updated_at: &str,
    version: i64,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "user_id":      user_id,
        "display_name": display_name,
        "username":     username,
        "role_id":      role_id,
        "branch_scope": branch_scope,
        "is_active":    is_active,
        "created_at":   created_at,
        "updated_at":   updated_at,
        "version":      version,
        "device_id":    device_id,
        "branch_id":    branch_id,
    });
    enqueue_raw_in_tx(tx, device_id, branch_id, "user", user_id, "update", payload, updated_at)
        .await?;
    Ok(())
}

/// Phase 3: enqueue audit log entries for central sync. Not yet called; kept for
/// forward-compatibility with the planned audit-trail sync in Phase 3.
#[allow(dead_code, clippy::too_many_arguments)]
pub async fn enqueue_audit_log(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    audit_log_id: &str,
    event_type: &str,
    entity_type: &str,
    entity_id: &str,
    actor_user_id: Option<&str>,
    actor_type: &str,
    ai_action_id: Option<&str>,
    before_json: Option<&str>,
    after_json: Option<&str>,
    reason: Option<&str>,
    created_at: &str,
    hash: &str,
    previous_hash: Option<&str>,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "audit_log_id":    audit_log_id,
        "event_type":      event_type,
        "entity_type":     entity_type,
        "entity_id":       entity_id,
        "actor_user_id":   actor_user_id,
        "actor_type":      actor_type,
        "ai_action_id":    ai_action_id,
        "device_id":       device_id,
        "origin_device_id": device_id,
        "branch_id":       branch_id,
        "before_json":     before_json,
        "after_json":      after_json,
        "reason":          reason,
        "created_at":      created_at,
        "hash":            hash,
        "previous_hash":   previous_hash,
    });
    enqueue_raw(
        pool,
        device_id,
        branch_id,
        "audit_log",
        audit_log_id,
        "append",
        payload,
        "v1",
    )
    .await?;
    Ok(())
}

/// Phase 3: transaction-aware variant of enqueue_audit_log.
#[allow(dead_code, clippy::too_many_arguments)]
pub async fn enqueue_audit_log_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    audit_log_id: &str,
    event_type: &str,
    entity_type: &str,
    entity_id: &str,
    actor_user_id: Option<&str>,
    actor_type: &str,
    ai_action_id: Option<&str>,
    before_json: Option<&str>,
    after_json: Option<&str>,
    reason: Option<&str>,
    created_at: &str,
    hash: &str,
    previous_hash: Option<&str>,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "audit_log_id":    audit_log_id,
        "event_type":      event_type,
        "entity_type":     entity_type,
        "entity_id":       entity_id,
        "actor_user_id":   actor_user_id,
        "actor_type":      actor_type,
        "ai_action_id":    ai_action_id,
        "device_id":       device_id,
        "origin_device_id": device_id,
        "branch_id":       branch_id,
        "before_json":     before_json,
        "after_json":      after_json,
        "reason":          reason,
        "created_at":      created_at,
        "hash":            hash,
        "previous_hash":   previous_hash,
    });
    enqueue_raw_in_tx(
        tx,
        device_id,
        branch_id,
        "audit_log",
        audit_log_id,
        "append",
        payload,
        "v1",
    )
    .await?;
    Ok(())
}

// ── Inventory ─────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_stock_movement(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    movement_id: &str,
    product_id: &str,
    movement_type: &str,
    quantity_delta: &str,
    quantity_after: &str,
    reference_type: &str,
    reference_id: &str,
    notes: Option<&str>,
    created_by_user_id: Option<&str>,
    created_at: &str,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "movement_id":          movement_id,
        "product_id":           product_id,
        "branch_id":            branch_id,
        "device_id":            device_id,
        "origin_device_id":     device_id,
        "movement_type":        movement_type,
        "quantity_delta":       quantity_delta,
        "quantity_after":       quantity_after,
        "reference_type":       reference_type,
        "reference_id":         reference_id,
        "notes":                notes,
        "created_by_user_id":   created_by_user_id,
        "created_at":           created_at,
    });
    enqueue_raw(
        pool,
        device_id,
        branch_id,
        "stock_movement",
        movement_id,
        "append",
        payload,
        "v1",
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_stock_movement_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    movement_id: &str,
    product_id: &str,
    movement_type: &str,
    quantity_delta: &str,
    quantity_after: &str,
    reference_type: &str,
    reference_id: &str,
    notes: Option<&str>,
    created_by_user_id: Option<&str>,
    created_at: &str,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "movement_id":          movement_id,
        "product_id":           product_id,
        "branch_id":            branch_id,
        "device_id":            device_id,
        "origin_device_id":     device_id,
        "movement_type":        movement_type,
        "quantity_delta":       quantity_delta,
        "quantity_after":       quantity_after,
        "reference_type":       reference_type,
        "reference_id":         reference_id,
        "notes":                notes,
        "created_by_user_id":   created_by_user_id,
        "created_at":           created_at,
    });
    enqueue_raw_in_tx(
        tx,
        device_id,
        branch_id,
        "stock_movement",
        movement_id,
        "append",
        payload,
        "v1",
    )
    .await?;
    Ok(())
}

pub async fn enqueue_stock_level(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    product_id: &str,
    quantity_on_hand: &str,
    updated_at: &str,
    last_movement_at: &str,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "product_id":        product_id,
        "branch_id":         branch_id,
        "device_id":         device_id,
        "quantity_on_hand":  quantity_on_hand,
        "updated_at":        updated_at,
        "last_movement_at":  last_movement_at,
    });
    // Use updated_at as idem_suffix so a newer update overrides an older one
    enqueue_raw(
        pool,
        device_id,
        branch_id,
        "stock_level",
        product_id,
        "update",
        payload,
        updated_at,
    )
    .await?;
    Ok(())
}

pub async fn enqueue_stock_level_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    product_id: &str,
    quantity_on_hand: &str,
    updated_at: &str,
    last_movement_at: &str,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "product_id":        product_id,
        "branch_id":         branch_id,
        "device_id":         device_id,
        "quantity_on_hand":  quantity_on_hand,
        "updated_at":        updated_at,
        "last_movement_at":  last_movement_at,
    });
    enqueue_raw_in_tx(
        tx,
        device_id,
        branch_id,
        "stock_level",
        product_id,
        "update",
        payload,
        updated_at,
    )
    .await?;
    Ok(())
}

// ── App config (store-wide settings) ─────────────────────────────────────────

/// Enqueue a single app_config key/value for sync.
/// Only store-wide keys should be enqueued — device-specific ones (printer port,
/// supabase credentials, etc.) must NOT be passed here.
pub async fn enqueue_app_config(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    key: &str,
    value: &str,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let payload = serde_json::json!({
        "key":        key,
        "value":      value,
        "device_id":  device_id,
        "branch_id":  branch_id,
        "updated_at": now,
    });
    // Use key+now as idem_suffix so every save is a distinct event
    enqueue_raw(pool, device_id, branch_id, "app_config", key, "update", payload, &now)
        .await?;
    Ok(())
}

pub async fn enqueue_app_config_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    key: &str,
    value: &str,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let payload = serde_json::json!({
        "key":        key,
        "value":      value,
        "device_id":  device_id,
        "branch_id":  branch_id,
        "updated_at": now,
    });
    enqueue_raw_in_tx(tx, device_id, branch_id, "app_config", key, "update", payload, &now)
        .await?;
    Ok(())
}

// ── Device registration ───────────────────────────────────────────────────

/// Enqueue a device record so other terminals learn about this device.
/// Called on join and on startup so every terminal eventually sees all others.
/// Idempotency key is stable per (device_code + name) so renaming triggers
/// a new event while repeated startups with the same details are deduplicated.
pub async fn enqueue_device(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    device_code: &str,
    name: &str,
    is_active: bool,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let payload = serde_json::json!({
        "device_id":   device_id,
        "branch_id":   branch_id,
        "device_code": device_code,
        "name":        name,
        "status":      "online",
        "is_active":   is_active,
        "updated_at":  now,
    });
    // Stable idem key: device_code + name — same device with same details is idempotent.
    let idem = format!("{}-{}", device_code, name);
    enqueue_raw(pool, device_id, branch_id, "device", device_id, "upsert", payload, &idem)
        .await?;
    Ok(())
}

pub async fn enqueue_device_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    device_code: &str,
    name: &str,
    is_active: bool,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let payload = serde_json::json!({
        "device_id":   device_id,
        "branch_id":   branch_id,
        "device_code": device_code,
        "name":        name,
        "status":      "online",
        "is_active":   is_active,
        "updated_at":  now,
    });
    let idem = format!("{}-{}", device_code, name);
    enqueue_raw_in_tx(tx, device_id, branch_id, "device", device_id, "upsert", payload, &idem)
        .await?;
    Ok(())
}

// ── Customer ─────────────────────────────────────────────────────────────────

pub async fn enqueue_customer(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    customer_id: &str,
    name: &str,
    phone: Option<&str>,
    email: Option<&str>,
    loyalty_points: i64,
    notes: Option<&str>,
    created_at: &str,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let payload = serde_json::json!({
        "customer_id":    customer_id,
        "branch_id":      branch_id,
        "device_id":      device_id,
        "name":           name,
        "phone":          phone,
        "email":          email,
        "loyalty_points": loyalty_points,
        "notes":          notes,
        "created_at":     created_at,
        "updated_at":     now,
    });
    enqueue_raw(pool, device_id, branch_id, "customer", customer_id, "upsert", payload, &now)
        .await?;
    Ok(())
}

pub async fn enqueue_customer_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    customer_id: &str,
    name: &str,
    phone: Option<&str>,
    email: Option<&str>,
    loyalty_points: i64,
    notes: Option<&str>,
    created_at: &str,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let payload = serde_json::json!({
        "customer_id":    customer_id,
        "branch_id":      branch_id,
        "device_id":      device_id,
        "name":           name,
        "phone":          phone,
        "email":          email,
        "loyalty_points": loyalty_points,
        "notes":          notes,
        "created_at":     created_at,
        "updated_at":     now,
    });
    enqueue_raw_in_tx(tx, device_id, branch_id, "customer", customer_id, "upsert", payload, &now)
        .await?;
    Ok(())
}

// ── Delivery orders ───────────────────────────────────────────────────────────

/// F-HIGH-02: Enqueue a delivery_order so other terminals receive status updates.
#[allow(clippy::too_many_arguments)]
pub async fn enqueue_delivery_order(
    pool: &SqlitePool,
    device_id: &str,
    branch_id: &str,
    delivery_id: &str,
    sale_id: &str,
    receipt_number: &str,
    customer_id: Option<&str>,
    customer_name: Option<&str>,
    contact_number: &str,
    address_text: &str,
    house_number: Option<&str>,
    area: Option<&str>,
    delivery_note: Option<&str>,
    delivery_staff_name: Option<&str>,
    expected_payment_method: &str,
    payment_status: &str,
    delivery_status: &str,
    amount_minor: i64,
    currency: &str,
    paid_confirmed_at: Option<&str>,
    created_by_user_id: &str,
    created_at: &str,
    updated_at: &str,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "delivery_id":              delivery_id,
        "sale_id":                  sale_id,
        "receipt_number":           receipt_number,
        "branch_id":                branch_id,
        "device_id":                device_id,
        "origin_device_id":         device_id,
        "customer_id":              customer_id,
        "customer_name":            customer_name,
        "contact_number":           contact_number,
        "address_text":             address_text,
        "house_number":             house_number,
        "area":                     area,
        "delivery_note":            delivery_note,
        "delivery_staff_name":      delivery_staff_name,
        "expected_payment_method":  expected_payment_method,
        "payment_status":           payment_status,
        "delivery_status":          delivery_status,
        "amount_minor":             amount_minor,
        "currency":                 currency,
        "paid_confirmed_at":        paid_confirmed_at,
        "created_by_user_id":       created_by_user_id,
        "created_at":               created_at,
        "updated_at":               updated_at,
    });
    enqueue_raw(
        pool, device_id, branch_id, "delivery_order", delivery_id, "upsert", payload, updated_at,
    )
    .await?;
    Ok(())
}

/// Transaction-aware variant of enqueue_delivery_order.
#[allow(clippy::too_many_arguments)]
pub async fn enqueue_delivery_order_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    device_id: &str,
    branch_id: &str,
    delivery_id: &str,
    sale_id: &str,
    receipt_number: &str,
    customer_id: Option<&str>,
    customer_name: Option<&str>,
    contact_number: &str,
    address_text: &str,
    house_number: Option<&str>,
    area: Option<&str>,
    delivery_note: Option<&str>,
    delivery_staff_name: Option<&str>,
    expected_payment_method: &str,
    payment_status: &str,
    delivery_status: &str,
    amount_minor: i64,
    currency: &str,
    paid_confirmed_at: Option<&str>,
    created_by_user_id: &str,
    created_at: &str,
    updated_at: &str,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "delivery_id":              delivery_id,
        "sale_id":                  sale_id,
        "receipt_number":           receipt_number,
        "branch_id":                branch_id,
        "device_id":                device_id,
        "origin_device_id":         device_id,
        "customer_id":              customer_id,
        "customer_name":            customer_name,
        "contact_number":           contact_number,
        "address_text":             address_text,
        "house_number":             house_number,
        "area":                     area,
        "delivery_note":            delivery_note,
        "delivery_staff_name":      delivery_staff_name,
        "expected_payment_method":  expected_payment_method,
        "payment_status":           payment_status,
        "delivery_status":          delivery_status,
        "amount_minor":             amount_minor,
        "currency":                 currency,
        "paid_confirmed_at":        paid_confirmed_at,
        "created_by_user_id":       created_by_user_id,
        "created_at":               created_at,
        "updated_at":               updated_at,
    });
    enqueue_raw_in_tx(
        tx, device_id, branch_id, "delivery_order", delivery_id, "upsert", payload, updated_at,
    )
    .await?;
    Ok(())
}

// ── Full-catalog enqueue (multi-terminal bootstrap) ───────────────────────────

/// Enqueue the COMPLETE local store state (products, categories, tax rules, users,
/// prices, customers, and this device) to the outbox so a freshly-joined terminal
/// pulls everything on its first sync.
///
/// This MUST be called at the moment setup completes / Supabase is configured —
/// NOT as a one-shot boot task. The old boot-time `sync_bootstrap_v2` ran before
/// the wizard created the real owner and before Supabase was configured, enqueuing
/// stale seed data into a void and then marking itself done forever. The result was
/// that the real owner/users and the registered device were never pushed, so a
/// second terminal saw no users to log in with and never appeared in the device list.
///
/// Safe to call repeatedly: enqueue_raw is idempotent on (entity_type, entity_id,
/// idem_suffix), and the central apply uses last-write-wins on updated_at.
pub async fn enqueue_full_catalog(pool: &SqlitePool) -> AppResult<()> {
    use sqlx::Row;

    let device_id: String = sqlx::query_scalar(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .unwrap_or_default();
    let branch_id: String = sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .unwrap_or_default();

    if device_id.is_empty() || branch_id.is_empty() {
        tracing::warn!("enqueue_full_catalog: no active device/branch — skipped");
        return Ok(());
    }

    // ── Categories ──
    if let Ok(rows) = sqlx::query(
        "SELECT category_id, name, sort_order, is_active, parent_category_id, created_at, updated_at, version FROM categories",
    ).fetch_all(pool).await {
        for r in &rows {
            let _ = enqueue_category(
                pool, &device_id, &branch_id,
                r.get("category_id"), r.get("name"),
                r.get::<i64, _>("sort_order"),
                r.get::<i64, _>("is_active") != 0,
                r.get("parent_category_id"),
                r.get("created_at"), r.get("updated_at"),
                r.get::<i64, _>("version"),
            ).await;
        }
        tracing::info!("enqueue_full_catalog: {} categories", rows.len());
    }

    // ── Tax rules ──
    if let Ok(rows) = sqlx::query(
        "SELECT tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from, updated_at, version FROM tax_rules",
    ).fetch_all(pool).await {
        for r in &rows {
            let updated_at: String = r.get::<Option<String>, _>("updated_at")
                .unwrap_or_else(|| r.get::<String, _>("effective_from"));
            let _ = enqueue_tax_rule(
                pool, &device_id, &branch_id,
                r.get("tax_rule_id"), r.get("name"),
                r.get::<i64, _>("rate_basis_points"),
                r.get::<i64, _>("inclusive") != 0,
                r.get::<i64, _>("is_active") != 0,
                r.get("effective_from"), &updated_at,
                r.get::<i64, _>("version"),
            ).await;
        }
        tracing::info!("enqueue_full_catalog: {} tax_rules", rows.len());
    }

    // ── Products ──
    if let Ok(rows) = sqlx::query(
        "SELECT product_id, category_id, name, sku, barcode, description,
                track_inventory, allow_decimal_quantity, is_active,
                tax_rule_id, cost_minor, currency, reorder_point, image_path, default_supplier_id,
                created_at, updated_at, version
         FROM products",
    ).fetch_all(pool).await {
        for r in &rows {
            let _ = enqueue_product(
                pool, &device_id, &branch_id,
                r.get("product_id"), r.get("category_id"), r.get("name"),
                r.get("sku"), r.get("barcode"), r.get("description"),
                r.get::<i64, _>("track_inventory") != 0,
                r.get::<i64, _>("allow_decimal_quantity") != 0,
                r.get::<i64, _>("is_active") != 0,
                r.get("tax_rule_id"), r.get("cost_minor"),
                r.get::<String, _>("currency").as_str(),
                r.get::<i64, _>("reorder_point"),
                r.get("image_path"),
                r.get("default_supplier_id"),
                r.get("created_at"), r.get("updated_at"),
                r.get::<i64, _>("version"),
            ).await;
        }
        tracing::info!("enqueue_full_catalog: {} products", rows.len());
    }

    // ── Product prices ──
    if let Ok(rows) = sqlx::query(
        "SELECT price_id, product_id, price_minor, currency, effective_from,
                created_by_user_id, created_by_ai_action_id, created_at
         FROM product_prices WHERE branch_id IS NULL AND price_type = 'selling' AND effective_to IS NULL",
    ).fetch_all(pool).await {
        for r in &rows {
            let _ = enqueue_product_price(
                pool, &device_id, &branch_id,
                r.get("price_id"), r.get("product_id"),
                r.get::<i64, _>("price_minor"),
                r.get::<String, _>("currency").as_str(),
                r.get("effective_from"),
                r.get::<Option<String>, _>("effective_to").as_deref(),
                r.get::<Option<String>, _>("created_by_user_id").unwrap_or_default().as_str(),
                r.get("created_by_ai_action_id"), r.get("created_at"),
            ).await;
        }
        tracing::info!("enqueue_full_catalog: {} product_prices", rows.len());
    }

    // ── Users (PIN hash intentionally NOT synced) ──
    if let Ok(rows) = sqlx::query(
        "SELECT user_id, display_name, username, role_id, branch_scope, is_active, created_at, updated_at, version FROM users",
    ).fetch_all(pool).await {
        for r in &rows {
            let _ = enqueue_user(
                pool, &device_id, &branch_id,
                r.get("user_id"), r.get("display_name"), r.get("username"),
                r.get("role_id"),
                r.get::<Option<String>, _>("branch_scope").unwrap_or_else(|| "[]".into()).as_str(),
                r.get::<i64, _>("is_active") != 0,
                r.get("created_at"), r.get("updated_at"),
                r.get::<i64, _>("version"),
            ).await;
        }
        tracing::info!("enqueue_full_catalog: {} users", rows.len());
    }

    // ── Customers ──
    if let Ok(rows) = sqlx::query(
        "SELECT customer_id, name, phone, email, loyalty_points, notes, created_at FROM customers WHERE branch_id = ?",
    ).bind(&branch_id).fetch_all(pool).await {
        for r in &rows {
            let _ = enqueue_customer(
                pool, &device_id, &branch_id,
                r.get("customer_id"), r.get("name"),
                r.get("phone"), r.get("email"),
                r.get::<i64, _>("loyalty_points"),
                r.get("notes"), r.get("created_at"),
            ).await;
        }
        tracing::info!("enqueue_full_catalog: {} customers", rows.len());
    }

    // ── This device ──
    if let Ok(Some(dev)) = sqlx::query(
        "SELECT device_id, branch_id, device_code, name, is_active FROM devices WHERE device_id = ?",
    ).bind(&device_id).fetch_optional(pool).await {
        let _ = enqueue_device(
            pool,
            dev.get("device_id"), dev.get("branch_id"),
            dev.get("device_code"), dev.get("name"),
            dev.get::<i64, _>("is_active") != 0,
        ).await;
        tracing::info!("enqueue_full_catalog: device record enqueued");
    }

    Ok(())
}
