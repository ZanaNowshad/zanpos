use crate::db::repositories::audit_hash;
use crate::domain::delivery::{
    CancelDeliveryInput, ConfirmPaymentInput, DeliveryInput, DeliveryListFilter, DeliveryRow,
    RevertPaymentInput, UpdateDeliveryStatusInput,
};
use crate::errors::{AppError, AppResult};
use sqlx::{Row, SqliteConnection, SqlitePool};
use ulid::Ulid;

/// Valid delivery status transitions.
/// - pending   → dispatched | cancelled
/// - dispatched → out_for_delivery | cancelled
/// - out_for_delivery → delivered | cancelled
/// - delivered  → (terminal — no further transitions)
/// - cancelled  → (terminal — no further transitions)
fn validate_delivery_transition(current: &str, next: &str) -> AppResult<()> {
    let allowed: &[&str] = match current {
        "pending" => &["dispatched", "cancelled"],
        "dispatched" => &["out_for_delivery", "cancelled"],
        "out_for_delivery" => &["delivered", "cancelled"],
        "delivered" => &[],
        "cancelled" => &[],
        _ => &[],
    };

    if current == next {
        // No-op re-apply of the same status is fine
        return Ok(());
    }

    if allowed.contains(&next) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "Cannot transition delivery from '{}' to '{}'",
            current, next
        )))
    }
}

/// Called from within sale_repo::finalize_sale's transaction.
/// Creates the delivery_order row atomically with the sale.
pub async fn create_delivery_in_tx(
    tx: &mut SqliteConnection,
    sale_id: &str,
    receipt_number: &str,
    amount_minor: i64,
    currency: &str,
    input: &DeliveryInput,
    created_by_user_id: &str,
    branch_id: &str,
    device_id: &str,
    now: &str,
) -> AppResult<DeliveryRow> {
    // Validate amount_minor — deliveries must have a positive value
    if amount_minor <= 0 {
        return Err(AppError::Validation(
            "Delivery amount must be greater than zero".into(),
        ));
    }
    // Validate expected_payment_method — exhaustive match against allowed values
    if !["cash", "card", "wallet"].contains(&input.expected_payment_method.as_str()) {
        return Err(AppError::Validation(format!(
            "Invalid expected_payment_method '{}': must be one of cash, card, wallet",
            input.expected_payment_method
        )));
    }
    // contact_number and address_text are required
    if input.contact_number.trim().is_empty() {
        return Err(AppError::Validation(
            "contact_number is required for delivery".into(),
        ));
    }
    if input.address_text.trim().is_empty() {
        return Err(AppError::Validation(
            "address_text is required for delivery".into(),
        ));
    }
    // customer_id is optional — a walk-in delivery can be placed without a registered customer.
    // The DB column is TEXT (nullable) so NULL is safe here.

    let delivery_id = Ulid::new().to_string();

    sqlx::query(
        "INSERT INTO delivery_orders
         (delivery_id, sale_id, receipt_number, customer_id, customer_name,
          contact_number, house_number, area, address_text, delivery_note,
          delivery_staff_name, expected_payment_method, payment_status,
          amount_minor, currency, delivery_status,
          created_by_user_id, branch_id, device_id, origin_device_id, created_at, updated_at,
          sync_status, sync_attempts)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,'unpaid',?,?,'pending',?,?,?,?,?,?,'pending',0)",
    )
    .bind(&delivery_id)
    .bind(sale_id)
    .bind(receipt_number)
    .bind(&input.customer_id)
    .bind(&input.customer_name)
    .bind(&input.contact_number)
    .bind(&input.house_number)
    .bind(&input.area)
    .bind(&input.address_text)
    .bind(&input.delivery_note)
    .bind(&input.delivery_staff_name)
    .bind(&input.expected_payment_method)
    .bind(amount_minor)
    .bind(currency)
    .bind(created_by_user_id)
    .bind(branch_id)
    .bind(device_id)
    .bind(device_id)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    // Mark the sale as a delivery
    sqlx::query("UPDATE sales SET is_delivery = 1 WHERE sale_id = ?")
        .bind(sale_id)
        .execute(&mut *tx)
        .await?;

    // Stock note: no stock_movements INSERT here. Stock is decremented by
    // sale_repo::finalize_sale (movement_type='sale') before this function is
    // called. Deliveries are pure logistics tracking for already-processed
    // POS sales; recording a second movement would double-count the outflow.

    Ok(DeliveryRow {
        delivery_id,
        sale_id: sale_id.to_string(),
        receipt_number: receipt_number.to_string(),
        customer_id: input.customer_id.clone(),
        customer_name: input.customer_name.clone(),
        contact_number: input.contact_number.clone(),
        house_number: input.house_number.clone(),
        area: input.area.clone(),
        address_text: input.address_text.clone(),
        delivery_note: input.delivery_note.clone(),
        delivery_staff_name: input.delivery_staff_name.clone(),
        expected_payment_method: input.expected_payment_method.clone(),
        payment_status: "unpaid".into(),
        delivery_status: "pending".into(),
        amount_minor,
        currency: currency.to_string(),
        paid_confirmed_by_user_id: None,
        paid_confirmed_at: None,
        payment_reference: None,
        payment_note: None,
        created_by_user_id: created_by_user_id.to_string(),
        branch_id: branch_id.to_string(),
        device_id: device_id.to_string(),
        created_at: now.to_string(),
        updated_at: now.to_string(),
    })
}

/// Fetch a single delivery by ID.
pub async fn get_delivery(pool: &SqlitePool, delivery_id: &str) -> AppResult<DeliveryRow> {
    sqlx::query_as::<_, DeliveryRow>(
        "SELECT delivery_id, sale_id, receipt_number, customer_id, customer_name,
                contact_number, house_number, area, address_text, delivery_note,
                delivery_staff_name, expected_payment_method, payment_status,
                delivery_status, amount_minor, currency,
                paid_confirmed_by_user_id, paid_confirmed_at,
                payment_reference, payment_note,
                created_by_user_id, branch_id, device_id, created_at, updated_at
         FROM delivery_orders WHERE delivery_id = ?",
    )
    .bind(delivery_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Delivery {} not found", delivery_id)))
}

/// List deliveries for a specific branch with optional filters.
pub async fn list_deliveries(
    pool: &SqlitePool,
    branch_id: &str,
    filter: &DeliveryListFilter,
) -> AppResult<Vec<DeliveryRow>> {
    // Build dynamic query — branch_id is always the first filter
    let mut conditions: Vec<&str> = vec!["d.branch_id = ?"];
    if filter.payment_status.is_some() {
        conditions.push("payment_status = ?");
    }
    if filter.delivery_status.is_some() {
        conditions.push("delivery_status = ?");
    }
    if filter.date_from.is_some() {
        conditions.push("created_at >= ?");
    }
    if filter.date_to.is_some() {
        conditions.push("created_at <= ?");
    }
    if filter.staff_name.is_some() {
        conditions.push("delivery_staff_name LIKE ?");
    }
    if filter.contact_search.is_some() {
        conditions.push("(contact_number LIKE ? OR customer_name LIKE ?)");
    }

    let where_clause = format!("WHERE {}", conditions.join(" AND "));

    let limit = filter.limit.unwrap_or(50);
    let offset = filter.offset.unwrap_or(0);

    let sql = format!(
        "SELECT delivery_id, sale_id, receipt_number, customer_id, customer_name,
                contact_number, house_number, area, address_text, delivery_note,
                delivery_staff_name, expected_payment_method, payment_status,
                delivery_status, amount_minor, currency,
                paid_confirmed_by_user_id, paid_confirmed_at,
                payment_reference, payment_note,
                created_by_user_id, branch_id, device_id, created_at, updated_at
         FROM delivery_orders d
         {} ORDER BY created_at DESC LIMIT ? OFFSET ?",
        where_clause
    );

    let mut q = sqlx::query_as::<_, DeliveryRow>(&sql);
    q = q.bind(branch_id);
    if let Some(v) = &filter.payment_status {
        q = q.bind(v);
    }
    if let Some(v) = &filter.delivery_status {
        q = q.bind(v);
    }
    if let Some(v) = &filter.date_from {
        q = q.bind(v);
    }
    if let Some(v) = &filter.date_to {
        q = q.bind(v);
    }
    if let Some(v) = &filter.staff_name {
        q = q.bind(format!("%{}%", v));
    }
    if let Some(v) = &filter.contact_search {
        let pattern = format!("%{}%", v);
        q = q.bind(pattern.clone()).bind(pattern);
    }
    q = q.bind(limit).bind(offset);

    Ok(q.fetch_all(pool).await?)
}

/// Update delivery_status. Any authenticated user can call this.
/// Cannot transition out of 'cancelled'. Cannot transition 'delivered' backwards.
pub async fn update_delivery_status(
    pool: &SqlitePool,
    input: &UpdateDeliveryStatusInput,
) -> AppResult<DeliveryRow> {
    let valid = [
        "pending",
        "dispatched",
        "out_for_delivery",
        "delivered",
        "cancelled",
    ];
    if !valid.contains(&input.delivery_status.as_str()) {
        return Err(AppError::Validation(format!(
            "Invalid delivery_status: {}",
            input.delivery_status
        )));
    }

    let existing = get_delivery(pool, &input.delivery_id).await?;

    // State machine: enforce valid transitions before touching the DB
    validate_delivery_transition(&existing.delivery_status, &input.delivery_status)?;

    // Guard: cannot cancel a paid delivery via status update — use cancel_delivery for dual-field update
    if input.delivery_status == "cancelled" && existing.payment_status == "paid" {
        return Err(AppError::Validation(
            "Cannot cancel a delivery that has already been paid".into(),
        ));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let before_json =
        serde_json::json!({ "delivery_status": &existing.delivery_status }).to_string();
    let after_json = serde_json::json!({ "delivery_status": &input.delivery_status }).to_string();

    sqlx::query(
        "UPDATE delivery_orders
         SET delivery_status = ?, payment_status = CASE WHEN ? = 'cancelled' THEN 'cancelled' ELSE payment_status END,
             updated_at = ?, version = version + 1, sync_status = 'pending'
         WHERE delivery_id = ?",
    )
    .bind(&input.delivery_status)
    .bind(&input.delivery_status)
    .bind(&now)
    .bind(&input.delivery_id)
    .execute(pool)
    .await?;

    // Audit log
    let audit_id = Ulid::new().to_string();
    let prev_hash = audit_hash::fetch_last_hash(pool, &existing.device_id)
        .await
        .unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &audit_id,
        event_type: "delivery.status_changed",
        entity_type: "delivery_order",
        entity_id: &input.delivery_id,
        actor_user_id: &input.actor_user_id,
        actor_type: "user",
        created_at: &now,
        before_json: Some(&before_json),
        after_json: Some(&after_json),
        reason: None,
        previous_hash: &prev_hash,
    });
    sqlx::query(
        "INSERT INTO audit_logs
         (audit_log_id, event_type, entity_type, entity_id, actor_user_id, actor_type,
          device_id, origin_device_id, branch_id, before_json, after_json, created_at, hash, previous_hash)
         VALUES (?,'delivery.status_changed','delivery_order',?,?,'user',?,?,?,?,?,?,?,?)",
    )
    .bind(&audit_id)
    .bind(&input.delivery_id)
    .bind(&input.actor_user_id)
    .bind(&existing.device_id)
    .bind(&existing.device_id)
    .bind(&existing.branch_id)
    .bind(&before_json)
    .bind(&after_json)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash)
    })
    .execute(pool)
    .await?;

    get_delivery(pool, &input.delivery_id).await
}

/// Confirm payment. Manager/owner only (RBAC checked in command layer).
/// Idempotent: if already paid, returns the existing row without error.
pub async fn confirm_payment(
    pool: &SqlitePool,
    input: &ConfirmPaymentInput,
) -> AppResult<DeliveryRow> {
    let existing = get_delivery(pool, &input.delivery_id).await?;

    // Idempotent: already paid — return as-is
    if existing.payment_status == "paid" {
        return Ok(existing);
    }

    // Guard: cannot confirm payment on a cancelled delivery
    if existing.payment_status == "cancelled" {
        return Err(AppError::Validation(
            "Cannot confirm payment on a cancelled delivery".into(),
        ));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let before_json = serde_json::json!({ "payment_status": "unpaid" }).to_string();
    let after_json = serde_json::json!({
        "payment_status": "paid",
        "paid_confirmed_by_user_id": &input.confirmed_by_user_id,
        "payment_reference": &input.payment_reference,
    })
    .to_string();

    sqlx::query(
        "UPDATE delivery_orders
         SET payment_status = 'paid',
             paid_confirmed_by_user_id = ?,
             paid_confirmed_at = ?,
             payment_reference = ?,
             payment_note = ?,
             updated_at = ?,
             version = version + 1,
             sync_status = 'pending'
         WHERE delivery_id = ? AND payment_status = 'unpaid'",
    )
    .bind(&input.confirmed_by_user_id)
    .bind(&now)
    .bind(&input.payment_reference)
    .bind(&input.payment_note)
    .bind(&now)
    .bind(&input.delivery_id)
    .execute(pool)
    .await?;

    // Audit log
    let audit_id = Ulid::new().to_string();
    let prev_hash = audit_hash::fetch_last_hash(pool, &existing.device_id)
        .await
        .unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &audit_id,
        event_type: "delivery.payment_confirmed",
        entity_type: "delivery_order",
        entity_id: &input.delivery_id,
        actor_user_id: &input.confirmed_by_user_id,
        actor_type: "user",
        created_at: &now,
        before_json: Some(&before_json),
        after_json: Some(&after_json),
        reason: None,
        previous_hash: &prev_hash,
    });
    sqlx::query(
        "INSERT INTO audit_logs
         (audit_log_id, event_type, entity_type, entity_id, actor_user_id, actor_type,
          device_id, origin_device_id, branch_id, before_json, after_json, created_at, hash, previous_hash)
         VALUES (?,'delivery.payment_confirmed','delivery_order',?,?,'user',?,?,?,?,?,?,?,?)",
    )
    .bind(&audit_id)
    .bind(&input.delivery_id)
    .bind(&input.confirmed_by_user_id)
    .bind(&existing.device_id)
    .bind(&existing.device_id)
    .bind(&existing.branch_id)
    .bind(&before_json)
    .bind(&after_json)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash)
    })
    .execute(pool)
    .await?;

    get_delivery(pool, &input.delivery_id).await
}

/// Cancel a delivery. Manager/owner only (RBAC in command layer).
pub async fn cancel_delivery(
    pool: &SqlitePool,
    input: &CancelDeliveryInput,
) -> AppResult<DeliveryRow> {
    let existing = get_delivery(pool, &input.delivery_id).await?;

    if existing.payment_status == "paid" {
        return Err(AppError::Validation(
            "Cannot cancel a delivery that has already been paid".into(),
        ));
    }
    if existing.delivery_status == "cancelled" {
        return Ok(existing); // idempotent
    }
    // Use the formal state machine for transition validation (replaces ad-hoc delivered guard)
    validate_delivery_transition(&existing.delivery_status, "cancelled")?;

    let now = chrono::Utc::now().to_rfc3339();
    let before_json = serde_json::json!({
        "delivery_status": &existing.delivery_status,
        "payment_status": &existing.payment_status,
    })
    .to_string();
    let after_json = serde_json::json!({
        "delivery_status": "cancelled",
        "payment_status": "cancelled",
    })
    .to_string();

    sqlx::query(
        "UPDATE delivery_orders
         SET delivery_status = 'cancelled', payment_status = 'cancelled',
             updated_at = ?, version = version + 1, sync_status = 'pending'
         WHERE delivery_id = ?",
    )
    .bind(&now)
    .bind(&input.delivery_id)
    .execute(pool)
    .await?;

    // Audit log
    let audit_id = Ulid::new().to_string();
    let prev_hash = audit_hash::fetch_last_hash(pool, &existing.device_id)
        .await
        .unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &audit_id,
        event_type: "delivery.cancelled",
        entity_type: "delivery_order",
        entity_id: &input.delivery_id,
        actor_user_id: &input.actor_user_id,
        actor_type: "user",
        created_at: &now,
        before_json: Some(&before_json),
        after_json: Some(&after_json),
        reason: None,
        previous_hash: &prev_hash,
    });
    sqlx::query(
        "INSERT INTO audit_logs
         (audit_log_id, event_type, entity_type, entity_id, actor_user_id, actor_type,
          device_id, origin_device_id, branch_id, before_json, after_json, created_at, hash, previous_hash)
         VALUES (?,'delivery.cancelled','delivery_order',?,?,'user',?,?,?,?,?,?,?,?)",
    )
    .bind(&audit_id)
    .bind(&input.delivery_id)
    .bind(&input.actor_user_id)
    .bind(&existing.device_id)
    .bind(&existing.device_id)
    .bind(&existing.branch_id)
    .bind(&before_json)
    .bind(&after_json)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash)
    })
    .execute(pool)
    .await?;

    get_delivery(pool, &input.delivery_id).await
}

/// Revert a paid delivery back to unpaid. Manager/owner only (RBAC in command layer).
/// Clears payment confirmation fields and writes an audit log entry.
pub async fn revert_payment(
    pool: &SqlitePool,
    input: &RevertPaymentInput,
) -> AppResult<DeliveryRow> {
    let existing = get_delivery(pool, &input.delivery_id).await?;

    // Guard: must currently be paid
    if existing.payment_status != "paid" {
        return Err(AppError::Validation(
            "Delivery is not currently marked as paid".into(),
        ));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let before_json = serde_json::json!({
        "payment_status": "paid",
        "paid_confirmed_by_user_id": &existing.paid_confirmed_by_user_id,
        "paid_confirmed_at": &existing.paid_confirmed_at,
    })
    .to_string();
    let after_json = serde_json::json!({
        "payment_status": "unpaid",
        "reverted_by_user_id": &input.actor_user_id,
        "reason": &input.reason,
    })
    .to_string();

    sqlx::query(
        "UPDATE delivery_orders
         SET payment_status = 'unpaid',
             paid_confirmed_by_user_id = NULL,
             paid_confirmed_at = NULL,
             payment_reference = NULL,
             payment_note = NULL,
             updated_at = ?,
             version = version + 1,
             sync_status = 'pending'
         WHERE delivery_id = ? AND payment_status = 'paid'",
    )
    .bind(&now)
    .bind(&input.delivery_id)
    .execute(pool)
    .await?;

    // Audit log
    let audit_id = Ulid::new().to_string();
    let prev_hash = audit_hash::fetch_last_hash(pool, &existing.device_id)
        .await
        .unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &audit_id,
        event_type: "delivery.payment_reverted",
        entity_type: "delivery_order",
        entity_id: &input.delivery_id,
        actor_user_id: &input.actor_user_id,
        actor_type: "user",
        created_at: &now,
        before_json: Some(&before_json),
        after_json: Some(&after_json),
        reason: None,
        previous_hash: &prev_hash,
    });
    sqlx::query(
        "INSERT INTO audit_logs
         (audit_log_id, event_type, entity_type, entity_id, actor_user_id, actor_type,
          device_id, origin_device_id, branch_id, before_json, after_json, created_at, hash, previous_hash)
         VALUES (?,'delivery.payment_reverted','delivery_order',?,?,'user',?,?,?,?,?,?,?,?)",
    )
    .bind(&audit_id)
    .bind(&input.delivery_id)
    .bind(&input.actor_user_id)
    .bind(&existing.device_id)
    .bind(&existing.device_id)
    .bind(&existing.branch_id)
    .bind(&before_json)
    .bind(&after_json)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash)
    })
    .execute(pool)
    .await?;

    get_delivery(pool, &input.delivery_id).await
}

/// Distinct rider names for autocomplete, ordered by most recently used.
pub async fn rider_suggestions(pool: &SqlitePool, branch_id: &str) -> AppResult<Vec<String>> {
    let rows = sqlx::query(
        "SELECT delivery_staff_name FROM delivery_orders
         WHERE branch_id = ? AND delivery_staff_name IS NOT NULL AND delivery_staff_name != ''
         GROUP BY delivery_staff_name ORDER BY MAX(created_at) DESC LIMIT 20",
    )
    .bind(branch_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .filter_map(|r| r.get::<Option<String>, _>("delivery_staff_name"))
        .collect())
}
