/// Outbox helpers — write full-payload sync_queue entries.
/// Called from repos after every entity write that must sync.
/// Each function returns the next local_sequence used, for callers that chain entries.
use sqlx::SqlitePool;
use ulid::Ulid;
use serde_json::Value;
use crate::errors::AppResult;

// ── Low-level enqueue ──────────────────────────────────────────────────────────

pub async fn enqueue_raw(
    pool:        &SqlitePool,
    device_id:   &str,
    branch_id:   &str,
    entity_type: &str,
    entity_id:   &str,
    operation:   &str,
    payload:     Value,
    idem_suffix: &str,   // appended to entity_id to form idempotency_key
) -> AppResult<i64> {
    let sync_event_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let seq: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(local_sequence), 0) + 1 FROM sync_queue WHERE device_id = ?"
    )
    .bind(device_id)
    .fetch_one(pool)
    .await?;

    let payload_str = payload.to_string();
    let payload_hash = format!("{:016x}", crc32_hash(&payload_str));
    let idempotency_key = format!("{}-{}-{}", entity_type, entity_id, idem_suffix);

    sqlx::query(
        "INSERT OR IGNORE INTO sync_queue
         (sync_event_id, device_id, branch_id, entity_type, entity_id, operation,
          payload_json, payload_hash, idempotency_key, local_sequence, created_at, status)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,'pending')"
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
    .execute(pool)
    .await?;

    Ok(seq)
}

fn crc32_hash(s: &str) -> u32 {
    let mut h: u32 = 0xFFFF_FFFF;
    for byte in s.bytes() {
        h ^= byte as u32;
        for _ in 0..8 {
            if h & 1 != 0 { h = (h >> 1) ^ 0xEDB8_8320; }
            else           { h >>= 1; }
        }
    }
    !h
}

// ── High-level enqueue helpers ─────────────────────────────────────────────────

pub async fn enqueue_sale(
    pool:          &SqlitePool,
    device_id:     &str,
    branch_id:     &str,
    sale_id:       &str,
    receipt_number: &str,
    shift_id:      &str,
    cashier_user_id: &str,
    status:        &str,
    gross_total_minor: i64,
    discount_total_minor: i64,
    tax_total_minor: i64,
    net_total_minor: i64,
    currency:      &str,
    business_date: &str,
    sold_at:       &str,
    created_offline: bool,
    idempotency_key: &str,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "sale_id":              sale_id,
        "receipt_number":       receipt_number,
        "branch_id":            branch_id,
        "device_id":            device_id,
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
    });
    enqueue_raw(pool, device_id, branch_id, "sale", sale_id, "create", payload, "v1").await?;
    Ok(())
}

pub async fn enqueue_sale_item(
    pool:          &SqlitePool,
    device_id:     &str,
    branch_id:     &str,
    sale_item_id:  &str,
    sale_id:       &str,
    product_id:    Option<&str>,
    product_name_snapshot: &str,
    sku_snapshot:  Option<&str>,
    barcode_snapshot: Option<&str>,
    quantity:      &str,
    unit_price_minor: i64,
    line_discount_minor: i64,
    tax_rule_snapshot: &str,
    tax_amount_minor: i64,
    line_total_minor: i64,
    note:          Option<&str>,
    voided:        bool,
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
        "branch_id":             branch_id,
    });
    enqueue_raw(pool, device_id, branch_id, "sale_item", sale_item_id, "append", payload, "v1").await?;
    Ok(())
}

pub async fn enqueue_payment(
    pool:                &SqlitePool,
    device_id:           &str,
    branch_id:           &str,
    payment_id:          &str,
    sale_id:             &str,
    payment_method:      &str,
    amount_minor:        i64,
    currency:            &str,
    status:              &str,
    external_reference:  Option<&str>,
    tendered_minor:      Option<i64>,
    change_minor:        Option<i64>,
    recorded_by_user_id: &str,
    recorded_at:         &str,
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
        "branch_id":            branch_id,
    });
    enqueue_raw(pool, device_id, branch_id, "payment", payment_id, "append", payload, "v1").await?;
    Ok(())
}

pub async fn enqueue_shift(
    pool:               &SqlitePool,
    device_id:          &str,
    branch_id:          &str,
    shift_id:           &str,
    cashier_user_id:    &str,
    opened_at:          &str,
    closed_at:          Option<&str>,
    opening_cash_minor: i64,
    counted_cash_minor: Option<i64>,
    status:             &str,
    close_notes:        Option<&str>,
    updated_at:         &str,
    operation:          &str,   // "create" or "update"
) -> AppResult<()> {
    let payload = serde_json::json!({
        "shift_id":             shift_id,
        "branch_id":            branch_id,
        "device_id":            device_id,
        "cashier_user_id":      cashier_user_id,
        "opened_at":            opened_at,
        "closed_at":            closed_at,
        "opening_cash_minor":   opening_cash_minor,
        "counted_cash_minor":   counted_cash_minor,
        "status":               status,
        "close_notes":          close_notes,
        "updated_at":           updated_at,
    });
    enqueue_raw(pool, device_id, branch_id, "shift", shift_id, operation, payload, updated_at).await?;
    Ok(())
}

pub async fn enqueue_refund(
    pool:                 &SqlitePool,
    device_id:            &str,
    branch_id:            &str,
    refund_id:            &str,
    original_sale_id:     &str,
    refund_receipt_number: &str,
    reason:               &str,
    refund_total_minor:   i64,
    currency:             &str,
    created_by_user_id:   &str,
    created_at:           &str,
    idempotency_key:      &str,
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
        "branch_id":              branch_id,
    });
    enqueue_raw(pool, device_id, branch_id, "refund", refund_id, "create", payload, "v1").await?;
    Ok(())
}

pub async fn enqueue_refund_item(
    pool:                 &SqlitePool,
    device_id:            &str,
    branch_id:            &str,
    refund_item_id:       &str,
    refund_id:            &str,
    sale_item_id:         &str,
    product_name_snapshot: &str,
    quantity:             &str,
    unit_price_minor:     i64,
    refund_amount_minor:  i64,
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
        "branch_id":             branch_id,
    });
    enqueue_raw(pool, device_id, branch_id, "refund_item", refund_item_id, "append", payload, "v1").await?;
    Ok(())
}

pub async fn enqueue_product(
    pool:                   &SqlitePool,
    device_id:              &str,
    branch_id:              &str,
    product_id:             &str,
    category_id:            &str,
    name:                   &str,
    sku:                    Option<&str>,
    barcode:                Option<&str>,
    description:            Option<&str>,
    track_inventory:        bool,
    allow_decimal_quantity: bool,
    is_active:              bool,
    tax_rule_id:            Option<&str>,
    cost_minor:             Option<i64>,
    currency:               &str,
    created_at:             &str,
    updated_at:             &str,
    version:                i64,
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
        "created_at":              created_at,
        "updated_at":              updated_at,
        "version":                 version,
        "device_id":               device_id,
        "branch_id":               branch_id,
    });
    enqueue_raw(pool, device_id, branch_id, "product", product_id, "update", payload, updated_at).await?;
    Ok(())
}

pub async fn enqueue_product_price(
    pool:                   &SqlitePool,
    device_id:              &str,
    branch_id:              &str,
    price_id:               &str,
    product_id:             &str,
    price_minor:            i64,
    currency:               &str,
    effective_from:         &str,
    created_by_user_id:     &str,
    created_by_ai_action_id: Option<&str>,
    created_at:             &str,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "price_id":               price_id,
        "product_id":             product_id,
        "branch_id":              branch_id,
        "price_type":             "selling",
        "price_minor":            price_minor,
        "currency":               currency,
        "effective_from":         effective_from,
        "created_by_user_id":     created_by_user_id,
        "created_by_ai_action_id": created_by_ai_action_id,
        "created_at":             created_at,
        "device_id":              device_id,
    });
    enqueue_raw(pool, device_id, branch_id, "product_price", price_id, "append", payload, "v1").await?;
    Ok(())
}

pub async fn enqueue_audit_log(
    pool:          &SqlitePool,
    device_id:     &str,
    branch_id:     &str,
    audit_log_id:  &str,
    event_type:    &str,
    entity_type:   &str,
    entity_id:     &str,
    actor_user_id: Option<&str>,
    actor_type:    &str,
    ai_action_id:  Option<&str>,
    before_json:   Option<&str>,
    after_json:    Option<&str>,
    reason:        Option<&str>,
    created_at:    &str,
    hash:          &str,
    previous_hash: Option<&str>,
) -> AppResult<()> {
    let payload = serde_json::json!({
        "audit_log_id":  audit_log_id,
        "event_type":    event_type,
        "entity_type":   entity_type,
        "entity_id":     entity_id,
        "actor_user_id": actor_user_id,
        "actor_type":    actor_type,
        "ai_action_id":  ai_action_id,
        "device_id":     device_id,
        "branch_id":     branch_id,
        "before_json":   before_json,
        "after_json":    after_json,
        "reason":        reason,
        "created_at":    created_at,
        "hash":          hash,
        "previous_hash": previous_hash,
    });
    enqueue_raw(pool, device_id, branch_id, "audit_log", audit_log_id, "append", payload, "v1").await?;
    Ok(())
}
