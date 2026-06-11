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
    pub owner_user_id: Option<String>,
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

    // Load device — prefer app_config 'device_id' (written at setup time, immune to
    // multi-device sync polluting the devices table with other terminals' records).
    let device_row = sqlx::query(
        "SELECT COALESCE(
           (SELECT value FROM app_config WHERE key = 'device_id' AND value != ''),
           (SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1)
         ) AS device_id",
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

    let owner_user_id: Option<String> = sqlx::query_scalar(
        "SELECT u.user_id FROM users u JOIN roles r ON r.role_id = u.role_id
         WHERE r.name = 'owner' AND u.is_active = 1 LIMIT 1",
    )
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
        owner_user_id,
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
    tracing::info!("setup_wizard_complete: started for store '{}'", input.store_name.trim());

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
        tracing::warn!("setup_wizard_complete: rejected — setup already complete");
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

    // ── BUG-BACKEND-6: Atomicity fix ──────────────────────────────────────────
    // All reads are performed BEFORE the transaction so we validate first and only
    // write if everything checks out. Async side-effects (Supabase upsert, sync
    // trigger) run AFTER tx.commit() — they cannot be inside a SQLite transaction.

    // --- PHASE 1: reads before any writes ---

    // F-MED-06: Look up role by name instead of hardcoding the seed ID.
    let owner_role_id: String = sqlx::query_scalar(
        "SELECT role_id FROM roles WHERE name = 'owner' LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::Internal("Owner role not found in database".into()))?;

    // Check if a user with this username already exists (owner may be the seeded admin)
    let existing_id: Option<String> =
        sqlx::query_scalar("SELECT user_id FROM users WHERE username = ?")
            .bind(&input.owner_username)
            .fetch_optional(&state.db)
            .await?;

    // Determine new owner_user_id before entering the transaction
    let new_owner_uid = if existing_id.is_none() {
        Some(ulid::Ulid::new().to_string())
    } else {
        None
    };
    let owner_user_id = existing_id.clone()
        .unwrap_or_else(|| new_owner_uid.clone().unwrap());

    let branch_id_for_new_user: Option<String> = if existing_id.is_none() {
        Some(
            sqlx::query_scalar(
                "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
            )
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::Internal("No active branch found".into()))?,
        )
    } else {
        None
    };

    // Read Supabase config so we can validate BEFORE writing anything
    let sb_url: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'supabase_url'")
            .fetch_optional(&state.db)
            .await?
            .flatten();
    let supabase_configured = sb_url.as_deref().is_some_and(|u| !u.is_empty());

    if supabase_configured {
        // CRITICAL: Guard against New-Store completing without schema migration.
        // NOTE: this check runs BEFORE writing anything so a failure here does not
        // permanently leave the DB in a partial state (BUG-BACKEND-6 fix).
        let schema_migrated: Option<String> =
            sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'schema_migrated'")
                .fetch_optional(&state.db)
                .await?
                .flatten();
        if schema_migrated.as_deref() != Some("1") {
            return Err(AppError::Permission(
                "Supabase is configured but the schema migration did not complete. \
                 Go back to the Cloud step and reconnect with a valid Personal Access Token."
                    .into(),
            ));
        }
    }

    // --- PHASE 2: single transaction for all local DB writes ---
    tracing::info!("setup_wizard_complete: starting atomic write transaction…");
    let mut tx = state.db.begin().await?;

    // Update the branch
    sqlx::query(
        "UPDATE branches SET
           name = ?, currency = ?, timezone = ?,
           address = ?, phone = ?,
           receipt_header = ?, receipt_footer = ?, tax_number = ?, cr_number = ?,
           updated_at = ?, sync_status = 'pending'
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
    .execute(&mut *tx)
    .await?;

    if let Some(uid) = &existing_id {
        // Update existing user — re-activate it (is_active=1).
        sqlx::query(
            "UPDATE users SET display_name=?, pin_hash=?, role_id=?, is_active=1,
             updated_at=?, sync_status='pending'
             WHERE user_id=?",
        )
        .bind(&input.owner_display_name)
        .bind(&pin_hash)
        .bind(&owner_role_id)
        .bind(&now)
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    } else {
        let uid = new_owner_uid.as_deref().unwrap();
        let bid = branch_id_for_new_user.as_deref().unwrap();
        sqlx::query(
            "INSERT INTO users
               (user_id, branch_id, display_name, username, pin_hash, role_id,
                branch_scope, is_active, created_at, updated_at, version)
             VALUES (?,?,?,?,?,?,'[]',1,?,?,1)",
        )
        .bind(uid)
        .bind(bid)
        .bind(&input.owner_display_name)
        .bind(&input.owner_username)
        .bind(&pin_hash)
        .bind(&owner_role_id)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    // Grace deadline / cloud config
    if supabase_configured {
        sqlx::query(
            "INSERT INTO app_config(key, value, updated_at) VALUES ('cloud_grace_deadline','',?)
             ON CONFLICT(key) DO UPDATE SET value='', updated_at=excluded.updated_at",
        )
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    } else {
        let deadline = (chrono::Utc::now() + chrono::Duration::days(7)).to_rfc3339();
        sqlx::query(
            "INSERT INTO app_config(key, value, updated_at) VALUES ('cloud_grace_deadline',?,?)
             ON CONFLICT(key) DO UPDATE SET
               value = CASE WHEN value = '' THEN excluded.value ELSE value END,
               updated_at = excluded.updated_at",
        )
        .bind(&deadline)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    // Mark setup complete — inside the transaction so branch+user+setup_complete are atomic.
    tracing::info!("setup_wizard_complete: writing setup_complete='1' inside transaction…");
    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('setup_complete','1',?)
         ON CONFLICT(key) DO UPDATE SET value='1', updated_at=excluded.updated_at",
    )
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    // Device reactivation safety-net (inside transaction)
    let active_device_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE is_active = 1")
            .fetch_one(&mut *tx)
            .await
            .unwrap_or(0);
    if active_device_count == 0 {
        let candidate: Option<String> = sqlx::query_scalar(
            "SELECT device_id FROM devices
             WHERE device_code = 'POS01' OR device_id = '01JDEVICE0000000000000001'
             ORDER BY created_at LIMIT 1",
        )
        .fetch_optional(&mut *tx)
        .await
        .ok()
        .flatten();
        if let Some(did) = candidate {
            let _ = sqlx::query(
                "UPDATE devices SET is_active = 1, status = 'online', updated_at = ?, sync_status = 'pending'
                 WHERE device_id = ?",
            )
            .bind(&now)
            .bind(&did)
            .execute(&mut *tx)
            .await;
        }
    }

    // Persist this terminal's identity key inside the transaction so it is
    // atomic with setup_complete. Ensures sync worker and RBAC resolve the
    // correct device_id even after pulling other terminals' records from Supabase.
    // Moved inside tx so a crash between commit and this write cannot leave the
    // device_id entry missing.
    let this_device_id: Option<String> = sqlx::query_scalar(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(&mut *tx)
    .await
    .ok()
    .flatten();

    if let Some(ref did) = this_device_id {
        let _ = sqlx::query(
            "INSERT INTO app_config(key, value, updated_at) VALUES ('device_id',?,?)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
        )
        .bind(did)
        .bind(&now)
        .execute(&mut *tx)
        .await;
    }

    tx.commit().await?;
    tracing::info!("setup_wizard_complete: transaction committed");

    // --- PHASE 3: post-commit async side-effects (network I/O) ---
    if supabase_configured {
        let sb_key: Option<String> = crate::secure_store::get_secret("supabase_service_key");
        let branch_row = sqlx::query(
            "SELECT branch_id, branch_code, name, currency, timezone,
                    address, phone, receipt_header, receipt_footer, tax_number, cr_number,
                    created_at
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
                "cr_number":      br.get::<Option<String>, _>("cr_number"),
                "is_active":      true,
                "created_at":     br.get::<String, _>("created_at"),
                "updated_at":     &now,
            });
            // Best-effort — don't fail setup if cloud upsert fails.
            tauri::async_runtime::spawn(async move {
                let _ = client.upsert_branch(&branch_json).await;
            });
        }

        // Trigger initial sync so the cloud is notified of the newly configured store.
        let worker = state.sync_worker.clone();
        tauri::async_runtime::spawn(async move {
            let _ = worker.run_once().await;
        });
    }

    // Return updated config with owner user_id so the frontend can
    // call RBAC-gated commands (CSV import, etc.) as the new owner.
    let mut cfg = app_config_load(state).await?;
    cfg.owner_user_id = Some(owner_user_id);
    tracing::info!("setup_wizard_complete: done — POS ready");
    Ok(cfg)
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
pub async fn settings_get_branch(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<BranchSettings, AppError> {
    // Branch settings contain PII (phone, address, tax/cr numbers) — require auth.
    rbac::require_any_role(&state.db, &actor_user_id).await?;
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
    // BUG-BRANCH-SYNC: set sync_status='pending' so the sync worker pushes the
    // updated branch row to Supabase via the normal incremental sync path.
    sqlx::query(
        "UPDATE branches SET
           name=?, timezone=?, address=?, phone=?,
           receipt_header=?, receipt_footer=?, tax_number=?, cr_number=?,
           updated_at=?, sync_status='pending'
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

    settings_get_branch(input.actor_user_id, state).await
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
    // Bug-Join-10: enforce a minimum length so single-char codes cannot be registered.
    if device_code.len() < 2 {
        return Err(AppError::Validation(
            "Device code must be at least 2 characters".into(),
        ));
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

    // Bug-Join-01: Wrap all local DB writes in an atomic transaction.
    // A failure at any step rolls back entirely — terminal is never left half-configured.
    let mut tx = state.db.begin().await?;

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
    .execute(&mut *tx)
    .await?;

    // Register this terminal — use provided code, fall back to ULID-derived
    let device_id = ulid::Ulid::new().to_string();
    let branch_id: String =
        sqlx::query_scalar("SELECT branch_id FROM branches WHERE is_active=1 LIMIT 1")
            .fetch_one(&mut *tx)
            .await?;

    // Bug-Join-08: Delete ALL inactive devices (not just hardcoded seed IDs).
    // Any row with is_active=0 is a safe cleanup candidate; active rows are real terminals.
    sqlx::query("DELETE FROM devices WHERE is_active = 0")
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        "INSERT OR REPLACE INTO devices (device_id, branch_id, device_code, name, status, is_active, created_at, updated_at)
         VALUES (?,?,?,?,'online',1,?,?)"
    )
    .bind(&device_id)
    .bind(&branch_id)
    .bind(&device_code)
    .bind(&device_name)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    // Persist this terminal's identity key — used by sync worker and RBAC to
    // identify THIS device even after other terminals' records sync locally.
    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('device_id',?,?)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
    )
    .bind(&device_id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    // Persist Supabase URL + clear grace deadline
    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('supabase_url',?,?)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
    )
    .bind(&url)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('cloud_grace_deadline','',?)
         ON CONFLICT(key) DO UPDATE SET value='', updated_at=excluded.updated_at",
    )
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    // Mark setup complete
    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('setup_complete','1',?)
         ON CONFLICT(key) DO UPDATE SET value='1', updated_at=excluded.updated_at",
    )
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    // Commit all DB writes atomically before touching any external resource.
    tx.commit().await?;

    // Post-commit: write service key to OS credential store.
    // Runs after commit so a keyring failure does not roll back the completed join.
    // A keyring failure is a hard error — we REFUSE to store the key in plaintext SQLite.
    if crate::secure_store::set_secret("supabase_service_key", &input.supabase_key) {
        // Key is safely in OS credential store — clear any stale plaintext entry.
        let _ = ai_admin_repo::set_config(&state.db, "supabase_service_key", "").await;
    } else {
        tracing::error!("CRITICAL: Falling back to plaintext secret storage — OS credential store write failed for supabase_service_key. Join aborted.");
        return Err(AppError::Internal(
            "Windows Credential Manager is unavailable. \
             Cannot store the Supabase service key securely. \
             Ensure the Credential Manager service is running and try again.".into(),
        ));
    }
    // Sync supabase_url to AI admin config (post-commit, non-critical).
    let _ = ai_admin_repo::set_config(&state.db, "supabase_url", &url).await;

    // Bug-Join-03: Do NOT spawn a background run_once() here.
    // The wizard calls setup_pull_catalog immediately after this returns, which
    // drives run_once() directly. A concurrent spawn races with it and both
    // invocations are no-ops due to the execution mutex.

    app_config_load(state).await
}

// ─── Supabase connection test (setup wizard, read-only probe) ─────────────────

#[derive(Debug, Serialize)]
pub struct ConnectionTestResult {
    pub connected: bool,
    pub store_name: Option<String>,
    pub error: Option<String>,
}

/// Lightweight credential + connectivity test for the JoinStore wizard.
/// Validates URL format, opens a Supabase connection, and returns the store
/// name if one exists. Never writes to the database — read-only probe.
#[tauri::command]
pub async fn setup_test_supabase_connection(
    url: String,
    key: String,
) -> Result<ConnectionTestResult, AppError> {
    let url = url.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        return Err(AppError::Validation("Supabase URL is required".into()));
    }
    if !url.starts_with("https://") {
        return Err(AppError::Validation(
            "URL must start with https://".into(),
        ));
    }
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(AppError::Validation("Service role key is required".into()));
    }

    use crate::sync::supabase_client::SupabaseClient;
    let client = SupabaseClient::new(url, key);

    if let Err(e) = client.validate().await {
        let msg = e.to_string();
        let friendly = if msg.contains("UNAUTHORIZED")
            || msg.contains("FORBIDDEN")
            || msg.contains("401")
            || msg.contains("403")
            || msg.contains("Invalid API key")
        {
            "Invalid credentials — check the URL and service role key.".to_string()
        } else if msg.contains("connect")
            || msg.contains("timeout")
            || msg.contains("dns")
            || msg.contains("DNS")
        {
            "Could not reach Supabase — check your internet connection.".to_string()
        } else {
            "Connection failed — check the URL and try again.".to_string()
        };
        return Ok(ConnectionTestResult {
            connected: false,
            store_name: None,
            error: Some(friendly),
        });
    }

    let store_name = match client.pull_branch().await {
        Ok(Some(branch)) => branch["name"].as_str().map(|s| s.to_string()),
        _ => None,
    };

    Ok(ConnectionTestResult {
        connected: true,
        store_name,
        error: None,
    })
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
