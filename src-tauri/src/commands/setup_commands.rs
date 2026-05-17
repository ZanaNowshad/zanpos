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
    pub setup_complete:      bool,
    /// True when Supabase URL + service key are persisted in app_config.
    pub supabase_configured: bool,
    /// ISO datetime when the 7-day cloud-setup grace period expires.
    /// None = Supabase is configured (grace period not applicable).
    pub cloud_grace_deadline: Option<String>,
    pub branch_id:           String,
    pub device_id:           String,
    pub branch_name:         String,
    pub branch_code:         String,
    pub currency:            String,
    pub currency_exponent:   i32,
    pub address:             Option<String>,
    pub phone:               Option<String>,
    pub receipt_header:      Option<String>,
    pub receipt_footer:      Option<String>,
    pub tax_number:          Option<String>,
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

    // Load Supabase configuration state
    let sb_url: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'supabase_url'"
    )
    .fetch_optional(&state.db)
    .await?
    .flatten();
    let sb_key: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'supabase_service_key'"
    )
    .fetch_optional(&state.db)
    .await?
    .flatten();
    let supabase_configured = sb_url.as_deref().is_some_and(|u| !u.is_empty())
        && sb_key.as_deref().is_some_and(|k| !k.is_empty());

    // Load cloud grace deadline (empty string → None)
    let grace_raw: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'cloud_grace_deadline'"
    )
    .fetch_optional(&state.db)
    .await?
    .flatten();
    let cloud_grace_deadline = grace_raw.filter(|s| !s.is_empty());

    let currency: String = branch_row.get("currency");
    let exp = currency_exponent(&currency);

    Ok(AppConfig {
        setup_complete,
        supabase_configured,
        cloud_grace_deadline,
        branch_id:           branch_row.get("branch_id"),
        device_id:           device_row.get("device_id"),
        branch_name:         branch_row.get("name"),
        branch_code:         branch_row.get("branch_code"),
        currency,
        currency_exponent:   exp,
        address:             branch_row.get("address"),
        phone:               branch_row.get("phone"),
        receipt_header:      branch_row.get("receipt_header"),
        receipt_footer:      branch_row.get("receipt_footer"),
        tax_number:          branch_row.get("tax_number"),
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

    // Check if Supabase is already configured
    let sb_url: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'supabase_url'"
    )
    .fetch_optional(&state.db)
    .await?
    .flatten();
    let supabase_configured = sb_url.as_deref().is_some_and(|u| !u.is_empty());

    if supabase_configured {
        // Clear any existing grace deadline — Supabase is set
        sqlx::query(
            "INSERT INTO app_config(key, value, updated_at) VALUES ('cloud_grace_deadline','',?)
             ON CONFLICT(key) DO UPDATE SET value='', updated_at=excluded.updated_at"
        )
        .bind(&now)
        .execute(&state.db)
        .await?;

        // Upsert branch record to central Supabase store registry
        let sb_key: Option<String> = sqlx::query_scalar(
            "SELECT value FROM app_config WHERE key = 'supabase_service_key'"
        )
        .fetch_optional(&state.db)
        .await?
        .flatten();
        let branch_row = sqlx::query(
            "SELECT branch_id, branch_code, name, currency, timezone,
                    address, phone, receipt_header, receipt_footer, tax_number
             FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1"
        )
        .fetch_optional(&state.db)
        .await?;

        if let (Some(key), Some(br)) = (sb_key, branch_row) {
            use crate::sync::supabase_client::SupabaseClient;
            let client = SupabaseClient::new(sb_url.unwrap_or_default(), key);
            let branch_json = serde_json::json!({
                "branch_id":      br.get::<String, _>("branch_id"),
                "branch_code":    br.get::<String, _>("branch_code"),
                "name":           br.get::<String, _>("name"),
                "currency":       br.get::<String, _>("currency"),
                "timezone":       br.get::<String, _>("timezone"),
                "address":        br.get::<Option<String>, _>("address"),
                "phone":          br.get::<Option<String>, _>("phone"),
                "receipt_header": br.get::<Option<String>, _>("receipt_header"),
                "receipt_footer": br.get::<Option<String>, _>("receipt_footer"),
                "tax_number":     br.get::<Option<String>, _>("tax_number"),
                "is_active":      true,
                "created_at":     &now,
                "updated_at":     &now,
            });
            // Best-effort — don't fail setup if cloud upsert fails
            let _ = client.upsert_branch(&branch_json).await;
        }
    } else {
        // Record grace deadline: now + 7 days
        let deadline = (chrono::Utc::now() + chrono::Duration::days(7)).to_rfc3339();
        sqlx::query(
            "INSERT INTO app_config(key, value, updated_at) VALUES ('cloud_grace_deadline',?,?)
             ON CONFLICT(key) DO UPDATE SET
               value = CASE WHEN value = '' THEN excluded.value ELSE value END,
               updated_at = excluded.updated_at"
        )
        .bind(&deadline)
        .bind(&now)
        .execute(&state.db)
        .await?;
    }

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

// ─── Join existing store ──────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct JoinStoreInput {
    pub supabase_url:     String,
    pub supabase_key:     String,
    pub device_name:      String,
    pub device_code:      String,
}

/// Second terminal joining an already-provisioned store.
/// 1. Validate Supabase credentials.
/// 2. Pull the branch record from Supabase's `branches` table.
/// 3. Update the local branch row with the central store data.
/// 4. Register this device locally.
/// 5. Persist Supabase credentials and mark setup complete.
#[tauri::command]
pub async fn setup_join_store(
    input: JoinStoreInput,
    state: State<'_, AppState>,
) -> Result<AppConfig, AppError> {
    let url = input.supabase_url.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        return Err(AppError::Validation("Supabase URL is required".into()));
    }
    if input.supabase_key.is_empty() {
        return Err(AppError::Validation("Service role key is required".into()));
    }
    let device_name = input.device_name.trim().to_string();
    let device_code = input.device_code.trim().to_uppercase();
    if device_name.is_empty() {
        return Err(AppError::Validation("Device name is required".into()));
    }
    if device_code.is_empty() {
        return Err(AppError::Validation("Device code is required".into()));
    }

    use crate::sync::supabase_client::SupabaseClient;
    let client = SupabaseClient::new(url.clone(), input.supabase_key.clone());

    // Validate credentials
    client.validate().await
        .map_err(|_| AppError::Validation(
            "Could not connect to Supabase — check the URL and service role key".into()
        ))?;

    // Pull branch from central store registry
    let branch_val = client.pull_branch().await
        .map_err(|e| AppError::Validation(format!("Failed to read store data: {e}")))?
        .ok_or_else(|| AppError::Validation(
            "No active store found in this Supabase project. \
             If this is a new store, use 'New Store' setup instead.".into()
        ))?;

    let now = chrono::Utc::now().to_rfc3339();

    // Update local branch with central data
    let central_name:     String = branch_val["name"].as_str().unwrap_or("").to_string();
    let central_currency: String = branch_val["currency"].as_str().unwrap_or("BHD").to_string();
    let central_timezone: String = branch_val["timezone"].as_str().unwrap_or("UTC").to_string();

    if central_name.is_empty() {
        return Err(AppError::Validation("Store data from Supabase is incomplete".into()));
    }

    sqlx::query(
        "UPDATE branches SET name=?, currency=?, timezone=?,
           address=?, phone=?, receipt_header=?, receipt_footer=?, tax_number=?,
           updated_at=?
         WHERE is_active=1"
    )
    .bind(&central_name)
    .bind(&central_currency)
    .bind(&central_timezone)
    .bind(branch_val["address"].as_str())
    .bind(branch_val["phone"].as_str())
    .bind(branch_val["receipt_header"].as_str())
    .bind(branch_val["receipt_footer"].as_str())
    .bind(branch_val["tax_number"].as_str())
    .bind(&now)
    .execute(&state.db)
    .await?;

    // Register this terminal — use provided code, fall back to ULID-derived
    let device_id = ulid::Ulid::new().to_string();
    let branch_id: String = sqlx::query_scalar("SELECT branch_id FROM branches WHERE is_active=1 LIMIT 1")
        .fetch_one(&state.db)
        .await?;

    // Remove any seed device and insert this terminal's record
    sqlx::query("DELETE FROM devices WHERE device_code = '01JDEVICE0000000000000001' OR device_code = 'POS01'")
        .execute(&state.db).await.ok();
    sqlx::query(
        "INSERT OR REPLACE INTO devices (device_id, branch_id, device_code, name, status, is_active)
         VALUES (?,?,?,?,'online',1)"
    )
    .bind(&device_id)
    .bind(&branch_id)
    .bind(&device_code)
    .bind(&device_name)
    .execute(&state.db)
    .await?;

    // Persist Supabase credentials + clear grace deadline
    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('supabase_url',?,?)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at"
    )
    .bind(&url).bind(&now).execute(&state.db).await?;

    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('supabase_service_key',?,?)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at"
    )
    .bind(&input.supabase_key).bind(&now).execute(&state.db).await?;

    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('cloud_grace_deadline','',?)
         ON CONFLICT(key) DO UPDATE SET value='', updated_at=excluded.updated_at"
    )
    .bind(&now).execute(&state.db).await?;

    // Mark setup complete
    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('setup_complete','1',?)
         ON CONFLICT(key) DO UPDATE SET value='1', updated_at=excluded.updated_at"
    )
    .bind(&now)
    .execute(&state.db)
    .await?;

    // Trigger initial sync to pull catalog from Supabase
    state.sync_worker.run_once().await;

    app_config_load(state).await
}
