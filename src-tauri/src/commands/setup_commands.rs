use crate::commands::rbac;
use crate::db::repositories::{ai_admin_repo, auth_repo};
use crate::errors::AppError;
use crate::AppState;
use serde::{Deserialize, Serialize};
use sqlx::Row;
/// First-run setup wizard and persistent store settings.
use tauri::State;

// ─── AppConfig — returned on every startup ────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct AppConfig {
    pub setup_complete: bool,
    /// True when Supabase URL + service key are persisted in app_config.
    pub supabase_configured: bool,
    /// ISO datetime when the 7-day cloud-setup grace period expires.
    /// None = Supabase is configured (grace period not applicable).
    pub cloud_grace_deadline: Option<String>,
    pub branch_id: String,
    pub device_id: String,
    pub branch_name: String,
    pub branch_code: String,
    pub currency: String,
    pub currency_exponent: i32,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub receipt_header: Option<String>,
    pub receipt_footer: Option<String>,
    pub tax_number: Option<String>,
    pub cr_number: Option<String>,
    pub whatsapp_benefit_number: Option<String>,
}

/// Map currency code → decimal exponent (minor units).
pub(crate) fn currency_exponent(currency: &str) -> i32 {
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
                receipt_header, receipt_footer, tax_number, cr_number
         FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No branch configured".into()))?;

    // Load device (first active device for this branch)
    let device_row = sqlx::query(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No device configured".into()))?;

    // Load setup_complete flag
    let setup_val: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'setup_complete'")
            .fetch_optional(&state.db)
            .await?
            .flatten();
    let setup_complete = setup_val.as_deref() == Some("1");

    // Load Supabase configuration state
    let sb_url: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'supabase_url'")
            .fetch_optional(&state.db)
            .await?
            .flatten();
    let sb_key = crate::secure_store::get_secret("supabase_service_key");
    let supabase_configured = sb_url.as_deref().is_some_and(|u| !u.is_empty())
        && sb_key.as_deref().is_some_and(|k| !k.is_empty());

    // Load cloud grace deadline (empty string → None)
    let grace_raw: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'cloud_grace_deadline'")
            .fetch_optional(&state.db)
            .await?
            .flatten();
    let cloud_grace_deadline = grace_raw.filter(|s| !s.is_empty());

    let wa_benefit: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'whatsapp_benefit_number'")
            .fetch_optional(&state.db)
            .await?
            .flatten();

    let currency: String = branch_row.get("currency");
    let exp = currency_exponent(&currency);

    Ok(AppConfig {
        setup_complete,
        supabase_configured,
        cloud_grace_deadline,
        branch_id: branch_row.get("branch_id"),
        device_id: device_row.get("device_id"),
        branch_name: branch_row.get("name"),
        branch_code: branch_row.get("branch_code"),
        currency,
        currency_exponent: exp,
        address: branch_row.get("address"),
        phone: branch_row.get("phone"),
        receipt_header: branch_row.get("receipt_header"),
        receipt_footer: branch_row.get("receipt_footer"),
        tax_number: branch_row.get("tax_number"),
        cr_number: branch_row.get("cr_number"),
        whatsapp_benefit_number: wa_benefit,
    })
}

// ─── Setup wizard ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SetupWizardInput {
    pub store_name: String,
    pub store_address: Option<String>,
    pub store_phone: Option<String>,
    pub receipt_header: Option<String>,
    pub receipt_footer: Option<String>,
    pub tax_number: Option<String>,
    pub cr_number: Option<String>,
    pub currency: String,
    pub timezone: String,
    pub owner_display_name: String,
    pub owner_username: String,
    pub owner_pin: String,
}

/// Complete the first-run setup wizard.
/// Updates branch details, sets owner credentials, marks setup done.
#[tauri::command]
pub async fn setup_wizard_complete(
    input: SetupWizardInput,
    state: State<'_, AppState>,
) -> Result<AppConfig, AppError> {
    // F-CRIT-01: Reject if setup has already been completed. Without this guard,
    // any code with Tauri IPC access can call this command post-setup to overwrite
    // the owner PIN and branch settings — a full account takeover from the webview.
    let already_done: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'setup_complete'")
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .flatten();
    if already_done.as_deref() == Some("1") {
        return Err(AppError::Permission(
            "Store setup is already complete. Use Back Office settings to make changes.".into(),
        ));
    }

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
           receipt_header = ?, receipt_footer = ?, tax_number = ?, cr_number = ?,
           updated_at = ?
         WHERE is_active = 1",
    )
    .bind(input.store_name.trim())
    .bind(&input.currency)
    .bind(&input.timezone)
    .bind(&input.store_address)
    .bind(&input.store_phone)
    .bind(&input.receipt_header)
    .bind(&input.receipt_footer)
    .bind(&input.tax_number)
    .bind(&input.cr_number)
    .bind(&now)
    .execute(&state.db)
    .await?;

    // Check if a user with this username already exists (owner may be the seeded admin)
    let existing_id: Option<String> =
        sqlx::query_scalar("SELECT user_id FROM users WHERE username = ?")
            .bind(&input.owner_username)
            .fetch_optional(&state.db)
            .await?;

    // F-MED-06: Look up role by name instead of hardcoding the seed ID.
    // Prevents broken role assignment if the seed ID ever changes in a migration.
    let owner_role_id: String = sqlx::query_scalar(
        "SELECT role_id FROM roles WHERE name = 'owner' LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::Internal("Owner role not found in database".into()))?;

    if let Some(user_id) = existing_id {
        // Update existing user — and CRITICALLY re-activate it (is_active=1).
        // The seeded 'admin' account is deactivated by migration 0030 (it ships with
        // a placeholder hash). When the wizard owner reuses that username, we must
        // flip is_active back on, otherwise every RBAC-gated command (CSV import,
        // product CRUD, etc.) fails with "User not found or inactive" and the owner
        // never appears in the active user list. (Bug: wizard owner inactive.)
        sqlx::query(
            "UPDATE users SET display_name=?, pin_hash=?, role_id=?, is_active=1, updated_at=?
             WHERE user_id=?",
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
        let branch_id: String = sqlx::query_scalar(
            "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
        )
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::Internal("No active branch found".into()))?;
        sqlx::query(
            "INSERT INTO users
               (user_id, branch_id, display_name, username, pin_hash, role_id,
                branch_scope, is_active, created_at, updated_at, version)
             VALUES (?,?,?,?,?,?,'[]',1,?,?,1)",
        )
        .bind(&user_id)
        .bind(&branch_id)
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
         ON CONFLICT(key) DO UPDATE SET value='1', updated_at=excluded.updated_at",
    )
    .bind(&now)
    .execute(&state.db)
    .await?;

    // Check if Supabase is already configured
    let sb_url: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'supabase_url'")
            .fetch_optional(&state.db)
            .await?
            .flatten();
    let supabase_configured = sb_url.as_deref().is_some_and(|u| !u.is_empty());

    if supabase_configured {
        // CRITICAL: Guard against New-Store completing without schema migration.
        // The admin_setup_supabase command sets schema_migrated='1' after running
        // CENTRAL_SCHEMA_SQL via Management API. If this flag is missing, the central
        // RPC doesn't exist and every push will 404 — a silent dead-end (§7.1, §8).
        let schema_migrated: Option<String> =
            sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'schema_migrated'")
                .fetch_optional(&state.db)
                .await?
                .flatten();
        if schema_migrated.as_deref() != Some("1") {
            return Err(AppError::Permission(
                "Supabase is configured but the central schema has not been migrated. \
                 Re-run setup with a Personal Access Token to complete the one-time schema setup. \
                 Without this, multi-terminal sync cannot work.".into(),
            ));
        }

        // Clear any existing grace deadline — Supabase is set
        sqlx::query(
            "INSERT INTO app_config(key, value, updated_at) VALUES ('cloud_grace_deadline','',?)
             ON CONFLICT(key) DO UPDATE SET value='', updated_at=excluded.updated_at",
        )
        .bind(&now)
        .execute(&state.db)
        .await?;

        // Upsert branch record to central Supabase store registry
        let sb_key: Option<String> =
            crate::secure_store::get_secret("supabase_service_key");
        let branch_row = sqlx::query(
            "SELECT branch_id, branch_code, name, currency, timezone,
                    address, phone, receipt_header, receipt_footer, tax_number
             FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
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

        // Trigger initial sync so the cloud is notified of the newly configured store.
        // Non-blocking: the UI must not block on network calls (offline-first principle).
        let worker = state.sync_worker.clone();
        tauri::async_runtime::spawn(async move {
            let _ = worker.run_once().await;
        });
    } else {
        // Record grace deadline: now + 7 days
        let deadline = (chrono::Utc::now() + chrono::Duration::days(7)).to_rfc3339();
        sqlx::query(
            "INSERT INTO app_config(key, value, updated_at) VALUES ('cloud_grace_deadline',?,?)
             ON CONFLICT(key) DO UPDATE SET
               value = CASE WHEN value = '' THEN excluded.value ELSE value END,
               updated_at = excluded.updated_at",
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
    pub branch_id: String,
    pub name: String,
    pub branch_code: String,
    pub currency: String,
    pub timezone: String,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub receipt_header: Option<String>,
    pub receipt_footer: Option<String>,
    pub tax_number: Option<String>,
    pub cr_number: Option<String>,
}

#[tauri::command]
pub async fn settings_get_branch(state: State<'_, AppState>) -> Result<BranchSettings, AppError> {
    let row = sqlx::query(
        "SELECT branch_id, name, branch_code, currency, timezone,
                address, phone, receipt_header, receipt_footer, tax_number, cr_number
         FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No branch found".into()))?;

    Ok(BranchSettings {
        branch_id: row.get("branch_id"),
        name: row.get("name"),
        branch_code: row.get("branch_code"),
        currency: row.get("currency"),
        timezone: row.get("timezone"),
        address: row.get("address"),
        phone: row.get("phone"),
        receipt_header: row.get("receipt_header"),
        receipt_footer: row.get("receipt_footer"),
        tax_number: row.get("tax_number"),
        cr_number: row.get("cr_number"),
    })
}

#[derive(Deserialize)]
pub struct UpdateBranchInput {
    pub name: String,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub receipt_header: Option<String>,
    pub receipt_footer: Option<String>,
    pub tax_number: Option<String>,
    pub cr_number: Option<String>,
    pub timezone: String,
    pub actor_user_id: String,
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
           receipt_header=?, receipt_footer=?, tax_number=?, cr_number=?,
           updated_at=?
         WHERE is_active=1",
    )
    .bind(input.name.trim())
    .bind(&input.timezone)
    .bind(&input.address)
    .bind(&input.phone)
    .bind(&input.receipt_header)
    .bind(&input.receipt_footer)
    .bind(&input.tax_number)
    .bind(&input.cr_number)
    .bind(&now)
    .execute(&state.db)
    .await?;

    // Push updated branch record to Supabase so other terminals pick it up.
    // Best-effort — don't fail the save if the cloud upsert errors.
    {
        let sb_url: Option<String> = sqlx::query_scalar(
            "SELECT value FROM app_config WHERE key='supabase_url'",
        )
        .fetch_optional(&state.db).await.ok().flatten().flatten();
        let sb_key = crate::secure_store::get_secret("supabase_service_key");

        if let (Some(url), Some(key)) = (
            sb_url.filter(|u| !u.is_empty()),
            sb_key.filter(|k| !k.is_empty()),
        ) {
            use sqlx::Row as _;
            let branch_row = sqlx::query(
                "SELECT branch_id, branch_code, name, currency, timezone,
                        address, phone, receipt_header, receipt_footer,
                        tax_number, cr_number, created_at
                 FROM branches WHERE is_active=1 ORDER BY created_at LIMIT 1",
            )
            .fetch_optional(&state.db).await.ok().flatten();

            if let Some(br) = branch_row {
                use crate::sync::supabase_client::SupabaseClient;
                let client = SupabaseClient::new(url, key);
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
                    "cr_number":      br.get::<Option<String>, _>("cr_number"),
                    "is_active":      true,
                    "created_at":     br.get::<String, _>("created_at"),
                    "updated_at":     &now,
                });
                let _ = client.upsert_branch(&branch_json).await;
            }
        }
    }

    settings_get_branch(state).await
}

// ─── Join existing store ──────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct JoinStoreInput {
    pub supabase_url: String,
    pub supabase_key: String,
    pub device_name: String,
    pub device_code: String,
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
    // F-CRIT-02: Reject if setup has already been completed on this terminal.
    // Prevents store reconfiguration attack (credential overwrite via IPC post-setup).
    // Note: a legitimately unconfigured terminal will have setup_complete absent or '0'.
    let already_done: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'setup_complete'")
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .flatten();
    if already_done.as_deref() == Some("1") {
        return Err(AppError::Permission(
            "This terminal is already set up. To re-join, reset the app from Back Office → Devices.".into(),
        ));
    }

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
    client.validate().await.map_err(|_| {
        AppError::Validation(
            "Could not connect to Supabase — check the URL and service role key".into(),
        )
    })?;

    // Pull branch from central store registry
    let branch_val = client
        .pull_branch()
        .await
        .map_err(|e| AppError::Validation(format!("Failed to read store data: {e}")))?
        .ok_or_else(|| {
            AppError::Validation(
                "No active store found in this Supabase project. \
             If this is a new store, use 'New Store' setup instead."
                    .into(),
            )
        })?;

    let now = chrono::Utc::now().to_rfc3339();

    // Update local branch with central data
    let central_name: String = branch_val["name"].as_str().unwrap_or("").to_string();
    let central_currency: String = branch_val["currency"].as_str().unwrap_or("BHD").to_string();
    let central_timezone: String = branch_val["timezone"].as_str().unwrap_or("UTC").to_string();

    if central_name.is_empty() {
        return Err(AppError::Validation(
            "Store data from Supabase is incomplete".into(),
        ));
    }

    sqlx::query(
        "UPDATE branches SET name=?, currency=?, timezone=?,
           address=?, phone=?, receipt_header=?, receipt_footer=?, tax_number=?, cr_number=?,
           updated_at=?
         WHERE is_active=1",
    )
    .bind(&central_name)
    .bind(&central_currency)
    .bind(&central_timezone)
    .bind(branch_val["address"].as_str())
    .bind(branch_val["phone"].as_str())
    .bind(branch_val["receipt_header"].as_str())
    .bind(branch_val["receipt_footer"].as_str())
    .bind(branch_val["tax_number"].as_str())
    .bind(branch_val["cr_number"].as_str())
    .bind(&now)
    .execute(&state.db)
    .await?;

    // Register this terminal — use provided code, fall back to ULID-derived
    let device_id = ulid::Ulid::new().to_string();
    let branch_id: String =
        sqlx::query_scalar("SELECT branch_id FROM branches WHERE is_active=1 LIMIT 1")
            .fetch_one(&state.db)
            .await?;

    // Remove any seed device and insert this terminal's record
    // Only delete inactive seed devices to avoid accidentally removing an active
    // device whose code happens to match one of the hardcoded seed values (M-18).
    sqlx::query(
        "DELETE FROM devices WHERE device_id IN (
           SELECT device_id FROM devices
           WHERE is_active = 0 AND device_code IN ('POS01', '01JDEVICE0000000000000001')
         )"
    )
    .execute(&state.db)
    .await?;
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
         ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
    )
    .bind(&url)
    .bind(&now)
    .execute(&state.db)
    .await?;

    ai_admin_repo::set_config(&state.db, "supabase_url", &url).await?;
    // Write service key to OS credential store. If it fails, keep the plaintext
    // fallback — never clear the only readable copy (prevents silent sync death).
    if crate::secure_store::set_secret("supabase_service_key", &input.supabase_key) {
        let _ = ai_admin_repo::set_config(&state.db, "supabase_service_key", "").await;
    } else {
        tracing::error!("CRITICAL: Failed to write service key to OS credential store. Keeping plaintext fallback to prevent sync death.");
    }

    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('cloud_grace_deadline','',?)
         ON CONFLICT(key) DO UPDATE SET value='', updated_at=excluded.updated_at",
    )
    .bind(&now)
    .execute(&state.db)
    .await?;

    // Mark setup complete
    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('setup_complete','1',?)
         ON CONFLICT(key) DO UPDATE SET value='1', updated_at=excluded.updated_at",
    )
    .bind(&now)
    .execute(&state.db)
    .await?;

    // sync_watermark rows are pre-seeded by migration 0010_sync.sql —
    // the first pull cycle will download all remote changes from 1970-01-01.
    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    // Trigger initial sync: push the device event + pull the full catalog from Supabase.
    // Non-blocking: the UI must not block on network calls (offline-first principle).
    // The sync will complete in the background and the first pull will apply
    // POS 1's products/categories/settings when connectivity is available.
    let worker = state.sync_worker.clone();
    tauri::async_runtime::spawn(async move {
        let _ = worker.run_once().await;
    });

    app_config_load(state).await
}

/// Save BenefitPay number. Requires manager or owner after setup is complete.
/// During initial setup wizard (before any users exist), actor_user_id may be empty —
/// we skip RBAC in that specific case only.
#[tauri::command]
pub async fn setup_save_benefit_number(
    benefit_number: String,
    actor_user_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    // F-HIGH-03: Apply RBAC post-setup. During initial setup wizard the owner has not
    // been created yet so we allow it; after setup, require manager/owner.
    let setup_done: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'setup_complete'")
            .fetch_optional(&state.db).await.ok().flatten().flatten();
    if setup_done.as_deref() == Some("1") {
        let uid = actor_user_id.as_deref().unwrap_or("");
        crate::commands::rbac::manager_or_owner(&state.db, uid).await?;
    }
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR REPLACE INTO app_config (key, value, updated_at) VALUES ('whatsapp_benefit_number', ?, ?)",
    )
    .bind(&benefit_number)
    .bind(&now)
    .execute(&state.db)
    .await?;

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    Ok(())
}

// ─── Business flags ───────────────────────────────────────────────────────────

/// Operational toggles that control business rules in the POS.
/// All flags are stored as "0"/"1" strings in app_config.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BusinessFlags {
    /// Allow a sale to finalize even when stock quantity would go below zero.
    pub allow_negative_stock: bool,
    /// When true, a non-empty reason is required for every discount applied.
    pub require_discount_reason: bool,
    /// When true, cashiers (not just managers/owners) may apply discounts.
    pub cashier_can_discount: bool,
    /// When true, the thermal receipt prints automatically after every sale.
    pub auto_print_receipt: bool,
}

impl Default for BusinessFlags {
    fn default() -> Self {
        Self {
            allow_negative_stock: false,
            require_discount_reason: true,
            cashier_can_discount: false,
            auto_print_receipt: false,
        }
    }
}

/// Read a single boolean flag from app_config ("1" == true, anything else == false).
async fn read_flag(pool: &sqlx::SqlitePool, key: &str, default: bool) -> bool {
    let val: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = ?",
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    match val.as_deref() {
        Some(v) => v == "1",
        None => default,
    }
}

/// Write a single boolean flag to app_config (upsert).
async fn write_flag(
    pool: &sqlx::SqlitePool,
    key: &str,
    value: bool,
) -> crate::errors::AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(if value { "1" } else { "0" })
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn business_flags_load(
    state: State<'_, AppState>,
) -> Result<BusinessFlags, AppError> {
    let flags = BusinessFlags {
        allow_negative_stock:    read_flag(&state.db, "flag_allow_negative_stock",    false).await,
        require_discount_reason: read_flag(&state.db, "flag_require_discount_reason", true).await,
        cashier_can_discount:    read_flag(&state.db, "flag_cashier_can_discount",    false).await,
        auto_print_receipt:      read_flag(&state.db, "flag_auto_print_receipt",      false).await,
    };
    Ok(flags)
}

#[derive(Deserialize)]
pub struct SaveBusinessFlagsInput {
    pub flags: BusinessFlags,
    pub actor_user_id: String,
}

#[tauri::command]
pub async fn business_flags_save(
    input: SaveBusinessFlagsInput,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    // Only managers and owners may change business rules.
    rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;

    write_flag(&state.db, "flag_allow_negative_stock",    input.flags.allow_negative_stock).await?;
    write_flag(&state.db, "flag_require_discount_reason", input.flags.require_discount_reason).await?;
    write_flag(&state.db, "flag_cashier_can_discount",    input.flags.cashier_can_discount).await?;
    write_flag(&state.db, "flag_auto_print_receipt",      input.flags.auto_print_receipt).await?;

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    Ok(())
}
