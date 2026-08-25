use crate::commands::customer_scope::{actor_branch_id, customer_in_branch};
use crate::commands::customer_search::{customer_search_patterns, customer_search_where};
use crate::commands::{rbac, sync_commands};
use crate::db::repositories::audit_hash;
use crate::errors::AppError;
use crate::AppState;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
/// Customer management commands — CRUD + loyalty points.
use tauri::State;
use ulid::Ulid;

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CustomerRow {
    pub customer_id: String,
    pub branch_id: String,
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub loyalty_points: i64,
    pub created_at: String,
    pub notes: Option<String>,
}

/// One page of customers plus the branch-wide match count, so the UI can show
/// how many rows exist without loading them.
#[derive(Debug, Serialize)]
pub struct CustomerPage {
    pub items: Vec<CustomerRow>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

// ─── Input types ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CustomerInput {
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub notes: Option<String>,
    pub actor_user_id: String,
}

#[derive(Deserialize)]
pub struct CustomerUpdateInput {
    pub customer_id: String,
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub notes: Option<String>,
    pub actor_user_id: String,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Paging bounds for `customer_list`. A supermarket's customer table is
/// high-cardinality; an unbounded SELECT loaded every row on every keystroke.
const CUSTOMER_LIST_DEFAULT_LIMIT: i64 = 50;
pub(crate) const CUSTOMER_LIST_MAX_LIMIT: i64 = 200;

pub(crate) fn map_row(r: &sqlx::sqlite::SqliteRow) -> CustomerRow {
    CustomerRow {
        customer_id: r.get("customer_id"),
        branch_id: r.get("branch_id"),
        name: r.get("name"),
        phone: r.get("phone"),
        email: r.get("email"),
        loyalty_points: r.get("loyalty_points"),
        created_at: r.get("created_at"),
        notes: r.get("notes"),
    }
}

fn clean_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

// ─── Commands ─────────────────────────────────────────────────────────────────

/// List customers, optionally filtered by name/phone search query.
#[tauri::command]
pub async fn customer_list(
    actor_user_id: String,
    search: String,
    offset: Option<i64>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<CustomerPage, AppError> {
    customer_list_inner(&state.db, &actor_user_id, &search, offset, limit).await
}

async fn customer_list_inner(
    pool: &SqlitePool,
    actor_user_id: &str,
    search: &str,
    offset: Option<i64>,
    limit: Option<i64>,
) -> Result<CustomerPage, AppError> {
    // F-HIGH-04: customer records contain PII (phone, email) — require an active role.
    rbac::require_any_role(pool, actor_user_id).await?;
    let branch_id = actor_branch_id(pool, actor_user_id).await?;

    let limit = limit
        .unwrap_or(CUSTOMER_LIST_DEFAULT_LIMIT)
        .clamp(1, CUSTOMER_LIST_MAX_LIMIT);
    let offset = offset.unwrap_or(0).max(0);

    // `name` is not unique, so ordering by it alone lets rows swap between
    // pages. customer_id is the primary key and breaks every tie, which is
    // what makes paging free of duplicates and gaps.
    let rows = if search.trim().is_empty() {
        sqlx::query(
            "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
             FROM customers
             WHERE branch_id = ?
             ORDER BY name, customer_id
             LIMIT ? OFFSET ?",
        )
        .bind(&branch_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else {
        // Search runs in SQL over the whole authorised branch, not over the
        // page already loaded, so a match on page 40 is still reachable.
        let (pattern, digit_pattern) = customer_search_patterns(search);
        let sql = format!(
            "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
             FROM customers
             WHERE branch_id = ?
               AND {}
             ORDER BY name, customer_id
             LIMIT ? OFFSET ?",
            customer_search_where(digit_pattern.is_some()),
        );
        let mut q = sqlx::query(&sql)
            .bind(&branch_id)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern);
        if let Some(digits) = &digit_pattern {
            q = q.bind(digits);
        }
        q.bind(limit).bind(offset).fetch_all(pool).await?
    };

    let total: i64 = if search.trim().is_empty() {
        sqlx::query_scalar("SELECT COUNT(*) FROM customers WHERE branch_id = ?")
            .bind(&branch_id)
            .fetch_one(pool)
            .await?
    } else {
        let (pattern, digit_pattern) = customer_search_patterns(search);
        let sql = format!(
            "SELECT COUNT(*) FROM customers WHERE branch_id = ? AND {}",
            customer_search_where(digit_pattern.is_some()),
        );
        let mut q = sqlx::query_scalar(&sql)
            .bind(&branch_id)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern);
        if let Some(digits) = &digit_pattern {
            q = q.bind(digits);
        }
        q.fetch_one(pool).await?
    };

    Ok(CustomerPage {
        items: rows.iter().map(map_row).collect(),
        total,
        offset,
        limit,
    })
}

/// Branch-wide loyalty totals.
///
/// Aggregated in SQL rather than summed over a page: the directory is paginated,
/// so a client-side total would silently describe only the rows on screen. Every
/// figure here is a COUNT or SUM over the actor's whole branch.
#[tauri::command]
pub async fn customer_create(
    input: CustomerInput,
    state: State<'_, AppState>,
) -> Result<CustomerRow, AppError> {
    rbac::require_any_role(&state.db, &input.actor_user_id).await?;
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
        "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
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
    input: CustomerUpdateInput,
    state: State<'_, AppState>,
) -> Result<CustomerRow, AppError> {
    rbac::require_any_role(&state.db, &input.actor_user_id).await?;
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
        "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
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

/// Get a single customer by ID.
#[tauri::command]
pub async fn customer_get(
    actor_user_id: String,
    customer_id: String,
    state: State<'_, AppState>,
) -> Result<CustomerRow, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    let branch_id = actor_branch_id(&state.db, &actor_user_id).await?;
    // Scoped in the WHERE clause, not checked after fetching: a foreign
    // customer must never be loaded into memory in the first place.
    let row = sqlx::query(
        "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
         FROM customers WHERE customer_id = ? AND branch_id = ?",
    )
    .bind(&customer_id)
    .bind(&branch_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Customer {} not found", customer_id)))?;

    Ok(map_row(&row))
}

/// Add loyalty points to a customer. Returns the new total.
#[cfg(test)]
mod tests;
