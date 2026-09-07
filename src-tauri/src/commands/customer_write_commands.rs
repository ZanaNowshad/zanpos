//! Creating and updating a customer.
//!
//! Split out of `customer_commands` when that file passed the 500-line limit.
//! The limit is a proxy for the fat-LTO discipline in CLAUDE.md — deep inline
//! chains through oversized code are what produced a release-only stack
//! overflow once — so the cut follows the read/write seam rather than a line
//! count: the directory and lookup stay put, the two writes move here.

use crate::commands::customer_commands::{
    clean_optional_text, map_row, CustomerInput, CustomerRow, CustomerUpdateInput,
};
use crate::commands::customer_scope::{actor_branch_id, customer_in_branch};
use crate::commands::{rbac, sync_commands};
use crate::db::repositories::audit_hash;
use crate::errors::AppError;
use crate::AppState;
use sqlx::Row;
use tauri::State;
use ulid::Ulid;

/// Branch-wide loyalty totals.
///
/// Aggregated in SQL rather than summed over a page: the directory is paginated,
/// so a client-side total would silently describe only the rows on screen. Every
/// figure here is a COUNT or SUM over the actor's whole branch.
#[tauri::command]
pub async fn customer_create(
    mut input: CustomerInput,
    session_token: String,
    state: State<'_, AppState>,
) -> Result<CustomerRow, AppError> {
    let actor =
        rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE).await?;
    input.actor_user_id = actor.user_id.clone();
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation("Customer name is required".into()));
    }
    if name.len() > 255 {
        return Err(AppError::Validation(
            "Customer name must not exceed 255 characters".into(),
        ));
    }
    if let Some(ref phone) = input.phone {
        let phone_trimmed = phone.trim();
        if !phone_trimmed.is_empty() {
            if phone_trimmed.len() > 30 {
                return Err(AppError::Validation(
                    "Phone number must not exceed 30 characters".into(),
                ));
            }
            if !phone_trimmed
                .chars()
                .all(|c| c.is_ascii_digit() || " +-()".contains(c))
            {
                return Err(AppError::Validation(
                    "Phone number contains invalid characters".into(),
                ));
            }
        }
    }
    let phone = clean_optional_text(input.phone.as_deref());
    let email = clean_optional_text(input.email.as_deref());
    let notes = clean_optional_text(input.notes.as_deref());

    // Stamped with the creator's branch so that create and list agree. These
    // resolve identically on a single-branch install (both originate from
    // `active_branch_id`), but on a multi-branch one a user must not create a
    // customer they would then be unable to see.
    let branch_id = actor_branch_id(&state.db, &input.actor_user_id).await?;
    let customer_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let device_id: String = sqlx::query_scalar(
        "SELECT device_id FROM devices WHERE is_active=1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .flatten()
    .unwrap_or_default();

    sqlx::query(
        "INSERT INTO customers
           (customer_id, branch_id, origin_device_id, name, phone, email, loyalty_points, created_at, updated_at, notes,
            sync_status, sync_attempts)
         VALUES (?,?,?,?,?,?,0,?,?,?,'pending',0)",
    )
    .bind(&customer_id)
    .bind(&branch_id)
    .bind(&device_id)
    .bind(input.name.trim())
    .bind(phone.as_deref())
    .bind(email.as_deref())
    .bind(&now)
    .bind(&now)
    .bind(notes.as_deref())
    .execute(&state.db)
    .await
    .map_err(|e| {
        // F-PHONE-UNIQUE: surface a user-friendly message instead of the raw sqlite error
        if let sqlx::Error::Database(ref db_err) = e {
            let msg = db_err.message();
            if msg.contains("UNIQUE constraint failed") && msg.contains("customers.phone") {
                return AppError::Conflict(
                    "Phone number is already registered to another customer".into(),
                );
            }
        }
        AppError::Database(e)
    })?;

    let row = sqlx::query(
        "SELECT customer_id, branch_id, name, whatsapp_name, phone, email, loyalty_points, created_at, notes
         FROM customers WHERE customer_id = ?",
    )
    .bind(&customer_id)
    .fetch_one(&state.db)
    .await?;

    // sync_status='pending', sync_attempts=0 are set explicitly in INSERT — sync worker picks it up

    // F-LOW-04: audit trail records who created the customer
    let after = serde_json::json!({
        "customer_id": customer_id, "name": input.name.trim(),
        "phone": phone, "email": email,
    })
    .to_string();
    if let Err(e) = audit_hash::insert_audit_entry(
        &state.db,
        "CUSTOMER_CREATED",
        "customer",
        &customer_id,
        &input.actor_user_id,
        "user",
        &device_id,
        &branch_id,
        None,
        Some(&after),
        None,
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [CUSTOMER_CREATED]: {:?}", e);
    }

    sync_commands::schedule_immediate_sync(&state);
    Ok(map_row(&row))
}

/// Update an existing customer.
#[tauri::command]
pub async fn customer_update(
    mut input: CustomerUpdateInput,
    session_token: String,
    state: State<'_, AppState>,
) -> Result<CustomerRow, AppError> {
    let actor =
        rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE).await?;
    input.actor_user_id = actor.user_id.clone();
    // A customer_id belonging to another branch must not become writable
    // simply by being sent; the scope check happens before any validation.
    let actor_branch = actor_branch_id(&state.db, &input.actor_user_id).await?;
    customer_in_branch(&state.db, &input.customer_id, &actor_branch).await?;
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation("Customer name is required".into()));
    }
    if name.len() > 255 {
        return Err(AppError::Validation(
            "Customer name must not exceed 255 characters".into(),
        ));
    }
    if let Some(ref phone) = input.phone {
        let phone_trimmed = phone.trim();
        if !phone_trimmed.is_empty() {
            if phone_trimmed.len() > 30 {
                return Err(AppError::Validation(
                    "Phone number must not exceed 30 characters".into(),
                ));
            }
            if !phone_trimmed
                .chars()
                .all(|c| c.is_ascii_digit() || " +-()".contains(c))
            {
                return Err(AppError::Validation(
                    "Phone number contains invalid characters".into(),
                ));
            }
        }
    }
    let phone = clean_optional_text(input.phone.as_deref());
    let email = clean_optional_text(input.email.as_deref());
    let notes = clean_optional_text(input.notes.as_deref());

    // Fetch existing customer for audit before-state and branch/device info
    let existing = sqlx::query(
        "SELECT name, phone, email, notes, branch_id, origin_device_id
         FROM customers WHERE customer_id = ?",
    )
    .bind(&input.customer_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Customer {} not found", input.customer_id)))?;

    let existing_branch: String = existing.get("branch_id");
    let existing_device: String = existing.get("origin_device_id");
    let before = serde_json::json!({
        "name": existing.get::<String, _>("name"),
        "phone": existing.get::<Option<String>, _>("phone"),
        "email": existing.get::<Option<String>, _>("email"),
        "notes": existing.get::<Option<String>, _>("notes"),
    })
    .to_string();

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE customers
         SET name=?, phone=?, email=?, notes=?, updated_at=?, sync_status = 'pending'
         WHERE customer_id=?",
    )
    .bind(&name)
    .bind(phone.as_deref())
    .bind(email.as_deref())
    .bind(notes.as_deref())
    .bind(&now)
    .bind(&input.customer_id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        // F-PHONE-UNIQUE: surface a user-friendly message instead of the raw sqlite error
        if let sqlx::Error::Database(ref db_err) = e {
            let msg = db_err.message();
            if msg.contains("UNIQUE constraint failed") && msg.contains("customers.phone") {
                return AppError::Conflict(
                    "Phone number is already registered to another customer".into(),
                );
            }
        }
        AppError::Database(e)
    })?;

    let row = sqlx::query(
        "SELECT customer_id, branch_id, name, whatsapp_name, phone, email, loyalty_points, created_at, notes
         FROM customers WHERE customer_id = ?",
    )
    .bind(&input.customer_id)
    .fetch_one(&state.db)
    .await?;

    let customer = map_row(&row);

    // sync_status='pending' is set explicitly in UPDATE — sync worker picks it up

    // Audit trail
    let after = serde_json::json!({
        "customer_id": input.customer_id,
        "name": input.name.trim(),
        "phone": phone,
        "email": email,
        "notes": notes,
    })
    .to_string();
    if let Err(e) = audit_hash::insert_audit_entry(
        &state.db,
        "CUSTOMER_UPDATED",
        "customer",
        &input.customer_id,
        &input.actor_user_id,
        "user",
        &existing_device,
        &existing_branch,
        Some(&before),
        Some(&after),
        None,
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [CUSTOMER_UPDATED]: {:?}", e);
    }

    sync_commands::schedule_immediate_sync(&state);
    Ok(customer)
}
