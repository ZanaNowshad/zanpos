/// Customer management commands — CRUD + loyalty points.
use tauri::State;
use sqlx::Row;
use ulid::Ulid;
use serde::{Deserialize, Serialize};
use crate::errors::{AppError, AppResult};
use crate::AppState;

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CustomerRow {
    pub customer_id:    String,
    pub branch_id:      String,
    pub name:           String,
    pub phone:          Option<String>,
    pub email:          Option<String>,
    pub loyalty_points: i64,
    pub created_at:     String,
    pub notes:          Option<String>,
}

// ─── Input types ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CustomerInput {
    pub name:  String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub notes: Option<String>,
}

#[derive(Deserialize)]
pub struct CustomerUpdateInput {
    pub customer_id: String,
    pub name:        String,
    pub phone:       Option<String>,
    pub email:       Option<String>,
    pub notes:       Option<String>,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

async fn active_branch_id(state: &AppState) -> AppResult<String> {
    let row = sqlx::query(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No active branch configured".into()))?;
    Ok(row.get("branch_id"))
}

fn map_row(r: &sqlx::sqlite::SqliteRow) -> CustomerRow {
    CustomerRow {
        customer_id:    r.get("customer_id"),
        branch_id:      r.get("branch_id"),
        name:           r.get("name"),
        phone:          r.get("phone"),
        email:          r.get("email"),
        loyalty_points: r.get("loyalty_points"),
        created_at:     r.get("created_at"),
        notes:          r.get("notes"),
    }
}

// ─── Commands ─────────────────────────────────────────────────────────────────

/// List customers, optionally filtered by name/phone search query.
#[tauri::command]
pub async fn customer_list(
    search: String,
    state: State<'_, AppState>,
) -> Result<Vec<CustomerRow>, AppError> {
    let rows = if search.trim().is_empty() {
        sqlx::query(
            "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
             FROM customers ORDER BY name"
        )
        .fetch_all(&state.db)
        .await?
    } else {
        let pattern = format!("%{}%", search.trim());
        sqlx::query(
            "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
             FROM customers
             WHERE name LIKE ? OR phone LIKE ?
             ORDER BY name"
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
    if input.name.trim().is_empty() {
        return Err(AppError::Validation("Customer name is required".into()));
    }

    let branch_id   = active_branch_id(&state).await?;
    let customer_id = Ulid::new().to_string();
    let now         = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO customers
           (customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes)
         VALUES (?,?,?,?,?,0,?,?)"
    )
    .bind(&customer_id)
    .bind(&branch_id)
    .bind(input.name.trim())
    .bind(input.phone.as_deref().filter(|s| !s.is_empty()))
    .bind(input.email.as_deref().filter(|s| !s.is_empty()))
    .bind(&now)
    .bind(input.notes.as_deref().filter(|s| !s.is_empty()))
    .execute(&state.db)
    .await?;

    let row = sqlx::query(
        "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
         FROM customers WHERE customer_id = ?"
    )
    .bind(&customer_id)
    .fetch_one(&state.db)
    .await?;

    Ok(map_row(&row))
}

/// Update an existing customer.
#[tauri::command]
pub async fn customer_update(
    input: CustomerUpdateInput,
    state: State<'_, AppState>,
) -> Result<CustomerRow, AppError> {
    if input.name.trim().is_empty() {
        return Err(AppError::Validation("Customer name is required".into()));
    }

    let affected = sqlx::query(
        "UPDATE customers
         SET name=?, phone=?, email=?, notes=?
         WHERE customer_id=?"
    )
    .bind(input.name.trim())
    .bind(input.phone.as_deref().filter(|s| !s.is_empty()))
    .bind(input.email.as_deref().filter(|s| !s.is_empty()))
    .bind(input.notes.as_deref().filter(|s| !s.is_empty()))
    .bind(&input.customer_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("Customer {} not found", input.customer_id)));
    }

    let row = sqlx::query(
        "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
         FROM customers WHERE customer_id = ?"
    )
    .bind(&input.customer_id)
    .fetch_one(&state.db)
    .await?;

    Ok(map_row(&row))
}

/// Get a single customer by ID.
#[tauri::command]
pub async fn customer_get(
    customer_id: String,
    state: State<'_, AppState>,
) -> Result<CustomerRow, AppError> {
    let row = sqlx::query(
        "SELECT customer_id, branch_id, name, phone, email, loyalty_points, created_at, notes
         FROM customers WHERE customer_id = ?"
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
    customer_id: String,
    points: i64,
    state: State<'_, AppState>,
) -> Result<i64, AppError> {
    sqlx::query(
        "UPDATE customers SET loyalty_points = loyalty_points + ? WHERE customer_id = ?"
    )
    .bind(points)
    .bind(&customer_id)
    .execute(&state.db)
    .await?;

    let new_total: i64 = sqlx::query_scalar(
        "SELECT loyalty_points FROM customers WHERE customer_id = ?"
    )
    .bind(&customer_id)
    .fetch_one(&state.db)
    .await?;

    Ok(new_total)
}
