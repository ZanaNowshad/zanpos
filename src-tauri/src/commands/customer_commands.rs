use crate::commands::rbac;
use crate::db::repositories::audit_hash;
use crate::errors::{AppError, AppResult};
use crate::sync::outbox;
use crate::AppState;
use serde::{Deserialize, Serialize};
use sqlx::Row;
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

async fn active_branch_id(state: &AppState) -> AppResult<String> {
    let row = sqlx::query(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No active branch configured".into()))?;
    Ok(row.get("branch_id"))
}

fn map_row(r: &sqlx::sqlite::SqliteRow) -> CustomerRow {
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

// ─── Commands ─────────────────────────────────────────────────────────────────

/// List customers, optionally filtered by name/phone search query.
#[tauri::command]
pub async fn customer_list(
    actor_user_id: String,
    search: String,
    state: State<'_, AppState>,
) -> Result<Vec<CustomerRow>, AppError> {
    // F-HIGH-04: customer records contain PII (phone, email) — require an active role.
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    let rows = if search.trim().is_empty() {
        sqlx::query(
            "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
             FROM customers ORDER BY name",
        )
        .fetch_all(&state.db)
        .await?
    } else {
        let pattern = format!("%{}%", search.trim());
        sqlx::query(
            "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
             FROM customers
             WHERE name LIKE ? OR phone LIKE ?
             ORDER BY name",
        )
        .bind(&pattern)
        .bind(&pattern)
        .fetch_all(&state.db)
        .await?
    };

    Ok(rows.iter().map(map_row).collect())
}

/// Create a new customer, returning the created row.
#[tauri::command]
pub async fn customer_create(
    input: CustomerInput,
    state: State<'_, AppState>,
) -> Result<CustomerRow, AppError> {
    rbac::require_any_role(&state.db, &input.actor_user_id).await?;
    if input.name.trim().is_empty() {
        return Err(AppError::Validation("Customer name is required".into()));
    }

    let branch_id = active_branch_id(&state).await?;
    let customer_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let device_id: String =
        sqlx::query_scalar("SELECT device_id FROM devices WHERE is_active=1 ORDER BY device_code LIMIT 1")
            .fetch_optional(&state.db).await?.flatten().unwrap_or_default();

    sqlx::query(
        "INSERT INTO customers
           (customer_id, branch_id, origin_device_id, name, phone, email, loyalty_points, created_at, updated_at, notes)
         VALUES (?,?,?,?,?,?,0,?,?,?)",
    )
    .bind(&customer_id)
    .bind(&branch_id)
    .bind(&device_id)
    .bind(input.name.trim())
    .bind(input.phone.as_deref().filter(|s| !s.is_empty()))
    .bind(input.email.as_deref().filter(|s| !s.is_empty()))
    .bind(&now)
    .bind(&now)
    .bind(input.notes.as_deref().filter(|s| !s.is_empty()))
    .execute(&state.db)
    .await?;

    let row = sqlx::query(
        "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
         FROM customers WHERE customer_id = ?",
    )
    .bind(&customer_id)
    .fetch_one(&state.db)
    .await?;

    // Enqueue for sync — best-effort, don't fail the create if sync write fails.
    let _ = outbox::enqueue_customer(
        &state.db, &device_id, &branch_id,
        &customer_id,
        input.name.trim(),
        input.phone.as_deref().filter(|s| !s.is_empty()),
        input.email.as_deref().filter(|s| !s.is_empty()),
        0,
        input.notes.as_deref().filter(|s| !s.is_empty()),
        &now,
        &now,
    ).await;

    // F-LOW-04: audit trail records who created the customer
    let after = serde_json::json!({
        "customer_id": customer_id, "name": input.name.trim(),
        "phone": input.phone, "email": input.email,
    }).to_string();
    let _ = audit_hash::insert_audit_entry(
        &state.db, "CUSTOMER_CREATED", "customer", &customer_id,
        &input.actor_user_id, "user", &device_id, &branch_id,
        None, Some(&after), None,
    ).await;

    Ok(map_row(&row))
}

/// Update an existing customer.
#[tauri::command]
pub async fn customer_update(
    input: CustomerUpdateInput,
    state: State<'_, AppState>,
) -> Result<CustomerRow, AppError> {
    rbac::require_any_role(&state.db, &input.actor_user_id).await?;
    if input.name.trim().is_empty() {
        return Err(AppError::Validation("Customer name is required".into()));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let affected = sqlx::query(
        "UPDATE customers
         SET name=?, phone=?, email=?, notes=?, updated_at=?
         WHERE customer_id=?",
    )
    .bind(input.name.trim())
    .bind(input.phone.as_deref().filter(|s| !s.is_empty()))
    .bind(input.email.as_deref().filter(|s| !s.is_empty()))
    .bind(input.notes.as_deref().filter(|s| !s.is_empty()))
    .bind(&now)
    .bind(&input.customer_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "Customer {} not found",
            input.customer_id
        )));
    }

    let row = sqlx::query(
        "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
         FROM customers WHERE customer_id = ?",
    )
    .bind(&input.customer_id)
    .fetch_one(&state.db)
    .await?;

    let customer = map_row(&row);

    // Enqueue updated customer for sync.
    let device_id: String =
        sqlx::query_scalar("SELECT device_id FROM devices WHERE is_active=1 ORDER BY device_code LIMIT 1")
            .fetch_optional(&state.db).await?.flatten().unwrap_or_default();
    let _ = outbox::enqueue_customer(
        &state.db, &device_id, &customer.branch_id,
        &customer.customer_id,
        &customer.name,
        customer.phone.as_deref(),
        customer.email.as_deref(),
        customer.loyalty_points,
        customer.notes.as_deref(),
        &customer.created_at,
        &now,
    ).await;

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
    let row = sqlx::query(
        "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
         FROM customers WHERE customer_id = ?",
    )
    .bind(&customer_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Customer {} not found", customer_id)))?;

    Ok(map_row(&row))
}

/// Add loyalty points to a customer. Returns the new total.
#[tauri::command]
pub async fn customer_add_loyalty(
    actor_user_id: String,
    customer_id: String,
    points: i64,
    state: State<'_, AppState>,
) -> Result<i64, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    if points == 0 {
        return Err(AppError::Validation("Points delta must be non-zero".into()));
    }

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE customers SET loyalty_points = loyalty_points + ?, updated_at = ? WHERE customer_id = ?")
        .bind(points)
        .bind(&now)
        .bind(&customer_id)
        .execute(&state.db)
        .await?;

    let row = sqlx::query(
        "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
         FROM customers WHERE customer_id = ?",
    )
    .bind(&customer_id)
    .fetch_one(&state.db)
    .await?;

    let customer = map_row(&row);
    let new_total = customer.loyalty_points;

    // Enqueue updated customer for sync.
    let device_id: String =
        sqlx::query_scalar("SELECT device_id FROM devices WHERE is_active=1 ORDER BY device_code LIMIT 1")
            .fetch_optional(&state.db).await?.flatten().unwrap_or_default();
    let _ = outbox::enqueue_customer(
        &state.db, &device_id, &customer.branch_id,
        &customer.customer_id,
        &customer.name,
        customer.phone.as_deref(),
        customer.email.as_deref(),
        customer.loyalty_points,
        customer.notes.as_deref(),
        &customer.created_at,
        &now,
    ).await;

    Ok(new_total)
}
