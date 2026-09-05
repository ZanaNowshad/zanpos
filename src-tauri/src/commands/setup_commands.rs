use crate::commands::{rbac, sync_commands};
use crate::db::repositories::auth_repo;
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
    pub database_path: String,
    /// "hub" | "terminal" | "standalone" — drives Settings/Hub UI + SyncChip.
    pub hub_mode: String,
    pub hub_url: Option<String>,
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
    /// A real session for the owner the wizard just created.
    ///
    /// Only ever set by `setup_wizard_complete`; `app_config_load` leaves it
    /// `None`, so it cannot be obtained by asking for the configuration.
    ///
    /// The wizard finishes by calling privileged commands — enabling the hub,
    /// importing a catalogue — at a moment when nobody has logged in. It used
    /// to do that by passing `owner_user_id`, which is the same "name a
    /// privileged user and be treated as them" shape being removed everywhere
    /// else; it simply had a plausible excuse. Issuing a session here gives the
    /// wizard genuine authority for those calls instead of a claim.
    pub owner_session_token: Option<String>,
}

pub(crate) use crate::domain::money::currency_exponent;

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

    // Load hub configuration state
    let hub_flag: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'hub_mode'")
            .fetch_optional(&state.db)
            .await?
            .flatten();
    let hub_url_raw: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'hub_url'")
            .fetch_optional(&state.db)
            .await?
            .flatten();
    let hub_url = hub_url_raw.filter(|s: &String| !s.is_empty());
    let hub_mode = if hub_flag.as_deref() == Some("1") {
        "hub"
    } else if hub_url.is_some() {
        "terminal"
    } else {
        "standalone"
    }
    .to_string();

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
    let database_path = sqlx::query("PRAGMA database_list")
        .fetch_all(&state.db)
        .await
        .ok()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row.get::<String, _>("name") == "main")
                .map(|row| row.get::<String, _>("file"))
        })
        .unwrap_or_default();

    Ok(AppConfig {
        setup_complete,
        database_path,
        hub_mode,
        hub_url,
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
        // Never here: reading the configuration must not hand out authority.
        // Only `setup_wizard_complete` issues one, for the owner it just made.
        owner_session_token: None,
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
    tracing::info!(
        "setup_wizard_complete: started for store '{}'",
        input.store_name.trim()
    );

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
    let owner_role_id: String =
        sqlx::query_scalar("SELECT role_id FROM roles WHERE name = 'owner' LIMIT 1")
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
    let owner_user_id = existing_id
        .clone()
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

    // Mark setup complete — inside the transaction so branch+user+setup_complete are atomic.
    tracing::info!("setup_wizard_complete: writing setup_complete='1' inside transaction…");
    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('setup_complete','1',?)
         ON CONFLICT(key) DO UPDATE SET value='1', updated_at=excluded.updated_at",
    )
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    // Device reactivation safety-net (inside transaction). Prefer the row this
    // terminal already claims as its identity; fall back to the seeded row on a
    // database that has not been re-keyed yet.
    let active_device_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE is_active = 1")
            .fetch_one(&mut *tx)
            .await
            .unwrap_or(0);
    if active_device_count == 0 {
        let candidate: Option<String> = sqlx::query_scalar(
            "SELECT device_id FROM devices
              WHERE device_id = (SELECT value FROM app_config WHERE key = 'device_id')
                 OR device_code = 'POS01'
                 OR device_id = '01JDEVICE0000000000000001'
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

    tx.commit().await?;
    tracing::info!("setup_wizard_complete: transaction committed");

    // Give this terminal an identity of its own. Every fresh database is seeded
    // with the same device row, and receipt_number embeds device_code, so a
    // second install left on the seed would mint receipt numbers a sibling has
    // already used. Runs after the commit rather than inside it because it opens
    // its own transaction; it is idempotent and also runs at startup, so a crash
    // in between is repaired on the next launch rather than leaving no identity.
    if let Err(e) = crate::device_identity::ensure(&state.db).await {
        tracing::error!("setup_wizard_complete: could not establish device identity: {e}");
    }
    sync_commands::schedule_immediate_sync(&state);

    // The wizard's remaining steps — enabling the hub, importing a catalogue —
    // are privileged, and nobody has logged in yet. Issue the new owner a real
    // session so those calls carry authority rather than an assertion.
    let issued = state.sessions.issue(&owner_user_id).await;

    let mut cfg = app_config_load(state).await?;
    cfg.owner_user_id = Some(owner_user_id);
    cfg.owner_session_token = Some(issued.token);
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

    sync_commands::schedule_immediate_sync(&state);

    settings_get_branch(input.actor_user_id, state).await
}

// ─── Save BenefitPay number ──────────────────────────────────────────────────
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
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .flatten();
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
    sync_commands::schedule_immediate_sync(&state);

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
    let val: Option<String> = sqlx::query_scalar("SELECT value FROM app_config WHERE key = ?")
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
pub async fn business_flags_load(state: State<'_, AppState>) -> Result<BusinessFlags, AppError> {
    let flags = BusinessFlags {
        allow_negative_stock: read_flag(&state.db, "flag_allow_negative_stock", false).await,
        require_discount_reason: read_flag(&state.db, "flag_require_discount_reason", true).await,
        cashier_can_discount: read_flag(&state.db, "flag_cashier_can_discount", false).await,
        auto_print_receipt: read_flag(&state.db, "flag_auto_print_receipt", false).await,
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

    write_flag(
        &state.db,
        "flag_allow_negative_stock",
        input.flags.allow_negative_stock,
    )
    .await?;
    write_flag(
        &state.db,
        "flag_require_discount_reason",
        input.flags.require_discount_reason,
    )
    .await?;
    write_flag(
        &state.db,
        "flag_cashier_can_discount",
        input.flags.cashier_can_discount,
    )
    .await?;
    write_flag(
        &state.db,
        "flag_auto_print_receipt",
        input.flags.auto_print_receipt,
    )
    .await?;

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up
    sync_commands::schedule_immediate_sync(&state);

    Ok(())
}

// ─── Operational settings ────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OperationalSettings {
    pub loyalty_points_per_bhd: i64,
    pub retention_days_sales: i64,
    pub retention_days_logs: i64,
    pub sync_interval_terminal_secs: i64,
    pub sync_interval_hub_secs: i64,
}

impl Default for OperationalSettings {
    fn default() -> Self {
        Self {
            loyalty_points_per_bhd: 1,
            retention_days_sales: 90,
            retention_days_logs: 30,
            sync_interval_terminal_secs: 10,
            sync_interval_hub_secs: 300,
        }
    }
}

async fn read_i64(pool: &sqlx::SqlitePool, key: &str, default: i64) -> i64 {
    sqlx::query_scalar("SELECT CAST(value AS INTEGER) FROM app_config WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten()
        .unwrap_or(default)
}

async fn write_i64(pool: &sqlx::SqlitePool, key: &str, value: i64) -> crate::errors::AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value.to_string())
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn operational_settings_load(
    state: State<'_, AppState>,
) -> Result<OperationalSettings, AppError> {
    let s = OperationalSettings {
        loyalty_points_per_bhd: read_i64(&state.db, "loyalty_points_per_bhd", 1).await,
        retention_days_sales: read_i64(&state.db, "retention_days_sales", 90).await,
        retention_days_logs: read_i64(&state.db, "retention_days_logs", 30).await,
        sync_interval_terminal_secs: read_i64(&state.db, "sync_interval_terminal_secs", 10).await,
        sync_interval_hub_secs: read_i64(&state.db, "sync_interval_hub_secs", 300).await,
    };
    Ok(s)
}

#[derive(Deserialize)]
pub struct SaveOperationalSettingsInput {
    pub settings: OperationalSettings,
    pub actor_user_id: String,
}

#[tauri::command]
pub async fn operational_settings_save(
    input: SaveOperationalSettingsInput,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;

    write_i64(
        &state.db,
        "loyalty_points_per_bhd",
        input.settings.loyalty_points_per_bhd,
    )
    .await?;
    write_i64(
        &state.db,
        "retention_days_sales",
        input.settings.retention_days_sales,
    )
    .await?;
    write_i64(
        &state.db,
        "retention_days_logs",
        input.settings.retention_days_logs,
    )
    .await?;
    write_i64(
        &state.db,
        "sync_interval_terminal_secs",
        input.settings.sync_interval_terminal_secs,
    )
    .await?;
    write_i64(
        &state.db,
        "sync_interval_hub_secs",
        input.settings.sync_interval_hub_secs,
    )
    .await?;

    sync_commands::schedule_immediate_sync(&state);

    Ok(())
}

// ─── Onboarding wizard progress (resumable across restarts) ──────────────────
// Power cuts happen in this market — the first-run wizard persists which of
// its six steps are resolved (done or explicitly skipped) so a relaunch can
// resume instead of starting over. See migration 0041_onboarding_state.sql.

#[derive(Debug, Serialize, Clone)]
pub struct OnboardingStepRow {
    pub step: String,
    pub completed_at: String,
}

/// Read which onboarding steps have been resolved. Unauthenticated like
/// `app_config_load` — it is polled at app startup, before any user is
/// signed in, purely to decide whether to resume the wizard.
#[tauri::command]
pub async fn onboarding_get_state(
    state: State<'_, AppState>,
) -> Result<Vec<OnboardingStepRow>, AppError> {
    let rows = sqlx::query("SELECT step, completed_at FROM onboarding_state")
        .fetch_all(&state.db)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| OnboardingStepRow {
            step: row.get("step"),
            completed_at: row.get("completed_at"),
        })
        .collect())
}

/// Mark one onboarding step resolved (completed or explicitly skipped).
/// Idempotent — safe to replay if the app restarts mid-step.
#[tauri::command]
pub async fn onboarding_mark_step(
    step: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::owner_only(&state.db, &actor_user_id).await?;
    if step.trim().is_empty() {
        return Err(AppError::Validation(
            "Onboarding step name is required".into(),
        ));
    }
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO onboarding_state (step, completed_at) VALUES (?, ?)
         ON CONFLICT(step) DO UPDATE SET completed_at = excluded.completed_at",
    )
    .bind(&step)
    .bind(&now)
    .execute(&state.db)
    .await?;
    // Wizard progress is the only signal for how far a store gets unassisted,
    // and where they stall. Recorded after the write so a failed step is not
    // reported as completed.
    crate::diagnostics::record_event(
        &state.db,
        "wizard_step",
        Some(serde_json::json!({ "step": step })),
    )
    .await;
    Ok(())
}
