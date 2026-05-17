/// First-run setup wizard and persistent store settings.
use tauri::State;
use sqlx::Row;
use serde::{Deserialize, Serialize};
use crate::errors::AppError;
use crate::db::repositories::auth_repo;
use crate::commands::rbac;
use crate::AppState;

// ─── AppConfig — returned on every startup ────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct AppConfig {
    pub setup_complete:    bool,
    pub branch_id:         String,
    pub device_id:         String,
    pub branch_name:       String,
    pub branch_code:       String,
    pub currency:          String,
    pub currency_exponent: i32,
    pub address:           Option<String>,
    pub phone:             Option<String>,
    pub receipt_header:    Option<String>,
    pub receipt_footer:    Option<String>,
    pub tax_number:        Option<String>,
}

/// Map currency code → decimal exponent (minor units).
fn currency_exponent(currency: &str) -> i32 {
    match currency {
        "BHD" | "KWD" | "OMR" => 3,
        "JPY" | "KRW" | "IDR" => 0,
        _ => 2, // USD, EUR, GBP, SAR, AED, QAR, EGP, MAD, etc.
    }
}

/// Load the active branch + device from the DB, plus setup_complete flag.
#[tauri::command]
pub async fn app_config_load(state: State<'_, AppState>) -> Result<AppConfig, AppError> {
    // Load branch (there is always exactly one active branch in single-store mode)
    let branch_row = sqlx::query(
        "SELECT branch_id, branch_code, name, currency, address, phone,
                receipt_header, receipt_footer, tax_number
         FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No branch configured".into()))?;

    // Load device (first active device for this branch)
    let device_row = sqlx::query(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No device configured".into()))?;

    // Load setup_complete flag
    let setup_val: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'setup_complete'"
    )
    .fetch_optional(&state.db)
    .await?
    .flatten();
    let setup_complete = setup_val.as_deref() == Some("1");

    let currency: String = branch_row.get("currency");
    let exp = currency_exponent(&currency);

    Ok(AppConfig {
        setup_complete,
        branch_id:         branch_row.get("branch_id"),
        device_id:         device_row.get("device_id"),
        branch_name:       branch_row.get("name"),
        branch_code:       branch_row.get("branch_code"),
        currency,
        currency_exponent: exp,
        address:           branch_row.get("address"),
        phone:             branch_row.get("phone"),
        receipt_header:    branch_row.get("receipt_header"),
        receipt_footer:    branch_row.get("receipt_footer"),
        tax_number:        branch_row.get("tax_number"),
    })
}

// ─── Setup wizard ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SetupWizardInput {
    pub store_name:      String,
    pub store_address:   Option<String>,
    pub store_phone:     Option<String>,
    pub receipt_header:  Option<String>,
    pub receipt_footer:  Option<String>,
    pub tax_number:      Option<String>,
    pub currency:        String,
    pub timezone:        String,
    pub owner_display_name: String,
    pub owner_username:     String,
    pub owner_pin:          String,
}

/// Complete the first-run setup wizard.
/// Updates branch details, sets owner credentials, marks setup done.
#[tauri::command]
pub async fn setup_wizard_complete(
    input: SetupWizardInput,
    state: State<'_, AppState>,
) -> Result<AppConfig, AppError> {
    // Validate
    if input.store_name.trim().is_empty() {
        return Err(AppError::Validation("Store name is required".into()));
    }
    if input.owner_display_name.trim().is_empty() {
        return Err(AppError::Validation("Owner name is required".into()));
    }
    if input.owner_username.trim().is_empty() {
        return Err(AppError::Validation("Username is required".into()));
    }
    if input.owner_pin.len() < 4 {
        return Err(AppError::Validation("PIN must be at least 4 digits".into()));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let pin_hash = auth_repo::hash_pin(&input.owner_pin)?;

    // Update the branch
    sqlx::query(
        "UPDATE branches SET
           name = ?, currency = ?, timezone = ?,
           address = ?, phone = ?,
           receipt_header = ?, receipt_footer = ?, tax_number = ?,
           updated_at = ?
         WHERE is_active = 1"
    )
    .bind(input.store_name.trim())
    .bind(&input.currency)
    .bind(&input.timezone)
    .bind(&input.store_address)
    .bind(&input.store_phone)
    .bind(&input.receipt_header)
    .bind(&input.receipt_footer)
    .bind(&input.tax_number)
    .bind(&now)
    .execute(&state.db)
    .await?;

    // Check if a user with this username already exists (owner may be the seeded admin)
    let existing_id: Option<String> = sqlx::query_scalar(
        "SELECT user_id FROM users WHERE username = ?"
    )
    .bind(&input.owner_username)
    .fetch_optional(&state.db)
    .await?;

    let owner_role_id = "01JROLE00000000000OWNER001";

    if let Some(user_id) = existing_id {
        // Update existing user
        sqlx::query(
            "UPDATE users SET display_name=?, pin_hash=?, role_id=?, updated_at=?
             WHERE user_id=?"
        )
        .bind(&input.owner_display_name)
        .bind(&pin_hash)
        .bind(owner_role_id)
        .bind(&now)
        .bind(&user_id)
        .execute(&state.db)
        .await?;
    } else {
        // Create new owner user
        let user_id = ulid::Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO users
               (user_id, display_name, username, pin_hash, role_id,
                branch_scope, is_active, created_at, updated_at, version)
             VALUES (?,?,?,?,?,'[]',1,?,?,1)"
        )
        .bind(&user_id)
        .bind(&input.owner_display_name)
        .bind(&input.owner_username)
        .bind(&pin_hash)
        .bind(owner_role_id)
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await?;
    }

    // Mark setup complete
    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('setup_complete','1',?)
         ON CONFLICT(key) DO UPDATE SET value='1', updated_at=excluded.updated_at"
    )
    .bind(&now)
    .execute(&state.db)
    .await?;

    // Return updated config
    app_config_load(state).await
}

// ─── Branch settings (post-setup, back-office) ────────────────────────────────

#[derive(Debug, Serialize)]
pub struct BranchSettings {
    pub branch_id:      String,
    pub name:           String,
    pub branch_code:    String,
    pub currency:       String,
    pub timezone:       String,
    pub address:        Option<String>,
    pub phone:          Option<String>,
    pub receipt_header: Option<String>,
    pub receipt_footer: Option<String>,
    pub tax_number:     Option<String>,
}

#[tauri::command]
pub async fn settings_get_branch(
    state: State<'_, AppState>,
) -> Result<BranchSettings, AppError> {
    let row = sqlx::query(
        "SELECT branch_id, name, branch_code, currency, timezone,
                address, phone, receipt_header, receipt_footer, tax_number
         FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No branch found".into()))?;

    Ok(BranchSettings {
        branch_id:      row.get("branch_id"),
        name:           row.get("name"),
        branch_code:    row.get("branch_code"),
        currency:       row.get("currency"),
        timezone:       row.get("timezone"),
        address:        row.get("address"),
        phone:          row.get("phone"),
        receipt_header: row.get("receipt_header"),
        receipt_footer: row.get("receipt_footer"),
        tax_number:     row.get("tax_number"),
    })
}

#[derive(Deserialize)]
pub struct UpdateBranchInput {
    pub name:           String,
    pub address:        Option<String>,
    pub phone:          Option<String>,
    pub receipt_header: Option<String>,
    pub receipt_footer: Option<String>,
    pub tax_number:     Option<String>,
    pub timezone:       String,
    pub actor_user_id:  String,
}

#[tauri::command]
pub async fn settings_update_branch(
    input: UpdateBranchInput,
    state: State<'_, AppState>,
) -> Result<BranchSettings, AppError> {
    rbac::owner_only(&state.db, &input.actor_user_id).await?;
    if input.name.trim().is_empty() {
        return Err(AppError::Validation("Store name is required".into()));
    }
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE branches SET
           name=?, timezone=?, address=?, phone=?,
           receipt_header=?, receipt_footer=?, tax_number=?,
           updated_at=?
         WHERE is_active=1"
    )
    .bind(input.name.trim())
    .bind(&input.timezone)
    .bind(&input.address)
    .bind(&input.phone)
    .bind(&input.receipt_header)
    .bind(&input.receipt_footer)
    .bind(&input.tax_number)
    .bind(&now)
    .execute(&state.db)
    .await?;

    settings_get_branch(state).await
}
