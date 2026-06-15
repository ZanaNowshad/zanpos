use crate::commands::rbac;
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;
use ulid::Ulid;

const SIDECAR_URL: &str = "http://127.0.0.1:3131";

// ─── Types ────────────────────────────────────────────────────────────────────

/// M23: Extended status so the frontend can distinguish three states:
///   sidecar_running=false → sidecar process is dead / not started
///   sidecar_running=true, connected=false → sidecar alive but not paired
///   sidecar_running=true, connected=true → paired and ready
#[derive(Debug, Serialize, Deserialize)]
pub struct WhatsAppStatus {
    pub connected: bool,
    pub qr: Option<String>, // base64 PNG data URL: "data:image/png;base64,..."
    #[serde(default)]
    pub sidecar_running: bool,
}

#[derive(Debug, Deserialize)]
pub struct SendDeliveryInput {
    pub to: String,
    pub receipt_number: String,
    pub net_total_minor: i64,
    pub currency_exponent: i32,
    pub address_text: String,
    pub house_number: Option<String>,
    pub area: Option<String>,
    /// Pre-built message from the frontend template editor.
    /// When present, skips the Rust message builder entirely.
    pub message_override: Option<String>,
}

// ─── Sidecar auth helper ──────────────────────────────────────────────────────

/// Read the shared-secret token the Node sidecar writes to
/// `<wa_session_dir>/.sidecar_token` on startup.  Returns an empty string
/// if the file is not found (sidecar not yet started).
fn read_sidecar_token(state: &AppState) -> String {
    std::fs::read_to_string(&state.wa_token_file)
        .unwrap_or_default()
        .trim()
        .to_string()
}

// ─── Phone normalisation ──────────────────────────────────────────────────────

/// Normalize a phone number: strip spaces/hyphens, ensure single '+' prefix,
/// remove any doubled country code (e.g. +973973... → +973...).
/// Returns an empty string if the input has no digits (e.g. empty or all symbols).
fn normalize_phone(raw: &str) -> String {
    let stripped: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '+')
        .collect();
    let digits_only: String = stripped.trim_start_matches('+').to_string();
    // Reject inputs with no digits — avoid sending "+" to the sidecar.
    if digits_only.is_empty() {
        return String::new();
    }
    // Check for doubled Gulf country codes
    for cc in &["973", "966", "971", "965", "968", "974", "967"] {
        let double = format!("{}{}", cc, cc);
        if digits_only.starts_with(&double) {
            return format!("+{}", &digits_only[cc.len()..]);
        }
    }
    format!("+{}", digits_only)
}

// ─── Network guard ────────────────────────────────────────────────────────────

/// Quick connectivity probe — tries a TCP connect to Google DNS on port 53.
/// Times out after 2 seconds; returns false when the device has no WAN link.
async fn is_network_available() -> bool {
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        tokio::net::TcpStream::connect("8.8.8.8:53"),
    )
    .await
    .is_ok()
}

// ─── Message builder ──────────────────────────────────────────────────────────

pub struct WhatsAppDeliveryParams<'a> {
    pub receipt_number: &'a str,
    pub net_total_minor: i64,
    pub currency_exponent: i32,
    pub address_text: &'a str,
    pub house_number: Option<&'a str>,
    pub area: Option<&'a str>,
    pub store_name: &'a str,
    pub store_phone: Option<&'a str>,
    pub benefit_number: Option<&'a str>,
}

/// Format minor units to decimal string, e.g. 1500 with exp=3 → "1.500"
fn fmt_money(minor: i64, exp: i32) -> String {
    // Delegate to the canonical integer-only formatter to avoid the negative-amount
    // sign-loss bug (e.g. -500 with exp=3 was emitted as "0.500" instead of "-0.500").
    crate::domain::money::format_minor(minor, exp as u32)
}

pub fn build_delivery_whatsapp_message(p: &WhatsAppDeliveryParams) -> String {
    let total = fmt_money(p.net_total_minor, p.currency_exponent);

    let location_line_en = match (p.house_number, p.area) {
        (Some(h), Some(a)) if !h.is_empty() || !a.is_empty() => {
            format!("\n🏠 {}, {}", h, a)
        }
        (Some(h), None) if !h.is_empty() => format!("\n🏠 {}", h),
        (None, Some(a)) if !a.is_empty() => format!("\n🏠 {}", a),
        _ => String::new(),
    };
    let location_line_ar = match (p.house_number, p.area) {
        (Some(h), Some(a)) if !h.is_empty() || !a.is_empty() => {
            format!("\n🏠 {}، {}", h, a)
        }
        (Some(h), None) if !h.is_empty() => format!("\n🏠 {}", h),
        (None, Some(a)) if !a.is_empty() => format!("\n🏠 {}", a),
        _ => String::new(),
    };

    let benefit_section_en = match p.benefit_number {
        Some(bn) if !bn.is_empty() => format!(
            "\n\n💳 Please send payment via BenefitPay to:\n    *{}*\n📸 Share the receipt screenshot to confirm payment.",
            bn
        ),
        _ => String::new(),
    };
    let benefit_section_ar = match p.benefit_number {
        Some(bn) if !bn.is_empty() => format!(
            "\n\n💳 يرجى إرسال الدفع عبر BenefitPay إلى:\n    *{}*\n📸 شارك صورة الإيصال لتأكيد الدفع.",
            bn
        ),
        _ => String::new(),
    };

    let footer = match (p.store_name, p.store_phone) {
        (n, Some(ph)) if !n.is_empty() => format!("\n_{}  •  {}_", n, ph),
        (n, None) if !n.is_empty() => format!("\n_{}_", n),
        _ => String::new(),
    };

    format!(
        "🛵 *Your delivery order is confirmed!*\n\n\
         📋 Order: #{receipt}\n\
         💰 Total: BHD {total}\n\
         📍 Address: {address}{loc_en}\
         {benefit_en}\n\n\
         ---\n\n\
         🛵 *تم تأكيد طلب التوصيل الخاص بك!*\n\n\
         📋 الطلب: #{receipt}\n\
         💰 الإجمالي: BHD {total}\n\
         📍 العنوان: {address}{loc_ar}\
         {benefit_ar}\n\n\
         ---\
         {footer}\n\
         شكراً لطلبك — Thank you 🙏",
        receipt = p.receipt_number,
        total = total,
        address = p.address_text,
        loc_en = location_line_en,
        benefit_en = benefit_section_en,
        loc_ar = location_line_ar,
        benefit_ar = benefit_section_ar,
        footer = footer,
    )
}

// ─── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn whatsapp_status(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<WhatsAppStatus> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    let token = read_sidecar_token(&state);
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();
    match client
        .get(format!("{}/status", SIDECAR_URL))
        .header("X-Sidecar-Token", &token)
        .send()
        .await
    {
        Ok(resp) => {
            // M23: Sidecar is reachable — it's running; parse its response
            let mut status = resp
                .json::<WhatsAppStatus>()
                .await
                .unwrap_or(WhatsAppStatus {
                    connected: false,
                    qr: None,
                    sidecar_running: true,
                });
            status.sidecar_running = true;
            Ok(status)
        }
        Err(e) => {
            // M23: Sidecar unreachable — distinguish from "not paired"
            tracing::debug!("whatsapp_status: sidecar unreachable — {}", e);
            Ok(WhatsAppStatus {
                connected: false,
                qr: None,
                sidecar_running: false,
            })
        }
    }
}

#[tauri::command]
pub async fn whatsapp_send_delivery(
    input: SendDeliveryInput,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<bool> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;

    if !is_network_available().await {
        return Err(AppError::Internal("WhatsApp: device is offline".into()));
    }

    let phone = normalize_phone(&input.to);
    if phone.is_empty() {
        return Err(AppError::Validation(
            "Recipient phone number is required".into(),
        ));
    }

    whatsapp_send_delivery_impl(
        &state,
        &phone,
        &input.receipt_number,
        input.net_total_minor,
        input.currency_exponent,
        &input.address_text,
        input.house_number.as_deref(),
        input.area.as_deref(),
        input.message_override.as_deref(),
    )
    .await?;

    Ok(true)
}

#[tauri::command]
pub async fn whatsapp_disconnect(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<bool> {
    crate::commands::rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let token = read_sidecar_token(&state);
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();
    Ok(client
        .post(format!("{}/disconnect", SIDECAR_URL))
        .header("X-Sidecar-Token", &token)
        .send()
        .await
        .is_ok())
}

#[tauri::command]
pub async fn whatsapp_save_config(
    benefit_number: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    crate::commands::rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    // BUG-WA-PHONE-VALIDATION: validate the BenefitPay phone number before saving.
    // Allow empty string (clears the setting). Non-empty strings must be a valid
    // international phone number: starts with '+', digits only after '+', total 8–16 chars.
    let trimmed = benefit_number.trim();
    if !trimmed.is_empty() {
        if !trimmed.starts_with('+') {
            return Err(AppError::Validation(
                "Phone number must start with '+' followed by the country code (e.g. +97333050666)"
                    .into(),
            ));
        }
        let after_plus = &trimmed[1..];
        if after_plus.is_empty() || !after_plus.chars().all(|c| c.is_ascii_digit()) {
            return Err(AppError::Validation(
                "Phone number must contain only digits after '+'".into(),
            ));
        }
        if trimmed.len() < 8 || trimmed.len() > 16 {
            return Err(AppError::Validation(
                "Phone number must be 8–16 characters including the '+' prefix".into(),
            ));
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR REPLACE INTO app_config (key, value, updated_at) VALUES ('whatsapp_benefit_number', ?, ?)",
    )
    .bind(trimmed)
    .bind(&now)
    .execute(&state.db)
    .await?;
    Ok(())
}

// ─── Notify arrival ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NotifyArrivalInput {
    pub to: String,
    pub receipt_number: String,
    pub delivery_id: String,
}

/// Sends a short bilingual WhatsApp message: "Delivery is outside, please come collect."
#[tauri::command]
pub async fn whatsapp_notify_arrival(
    input: NotifyArrivalInput,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<bool> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;

    // Hard rule: arrival message only within 1 hour of delivery bill creation.
    if !input.delivery_id.is_empty() {
        let created_at_str: Option<String> = sqlx::query_scalar(
            "SELECT created_at FROM deliveries WHERE delivery_id = ?",
        )
        .bind(&input.delivery_id)
        .fetch_optional(&state.db)
        .await?
        .flatten();
        if let Some(s) = created_at_str {
            if let Ok(created_at) = chrono::DateTime::parse_from_rfc3339(&s) {
                let age = chrono::Utc::now()
                    .signed_duration_since(created_at.with_timezone(&chrono::Utc));
                if age > chrono::Duration::hours(1) {
                    return Err(AppError::Validation(
                        "The 'delivery outside' message can only be sent within 1 hour of the delivery bill being created.".into(),
                    ));
                }
            }
        }
    }

    if !is_network_available().await {
        return Err(AppError::Internal("WhatsApp: device is offline".into()));
    }

    let phone = normalize_phone(&input.to);
    if phone.is_empty() {
        return Err(AppError::Validation(
            "Recipient phone number is required".into(),
        ));
    }

    whatsapp_notify_arrival_impl(&state, &phone, &input.receipt_number).await?;

    Ok(true)
}

// ─── Payment reminder ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PaymentReminderInput {
    pub to: String,
    pub receipt_number: String,
    pub amount_minor: i64,
    pub currency_exponent: i32,
    /// ISO currency code passed from frontend DEVICE constant, e.g. "BHD"
    pub currency: String,
    pub delivery_id: String,
}

/// Sends a bilingual WhatsApp payment reminder that includes the store's
/// BenefitPay number (fetched from app_config) and the outstanding amount.
#[tauri::command]
pub async fn whatsapp_payment_reminder(
    input: PaymentReminderInput,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<bool> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;

    // Hard rule: payment reminder only on the same calendar day as the delivery bill (Bahrain UTC+3).
    if !input.delivery_id.is_empty() {
        let created_at_str: Option<String> = sqlx::query_scalar(
            "SELECT created_at FROM deliveries WHERE delivery_id = ?",
        )
        .bind(&input.delivery_id)
        .fetch_optional(&state.db)
        .await?
        .flatten();
        if let Some(s) = created_at_str {
            if let Ok(created_at) = chrono::DateTime::parse_from_rfc3339(&s) {
                let bahrain = chrono::FixedOffset::east_opt(3 * 3600).unwrap();
                let created_date = created_at.with_timezone(&bahrain).date_naive();
                let today = chrono::Utc::now().with_timezone(&bahrain).date_naive();
                if created_date != today {
                    return Err(AppError::Validation(
                        "Payment reminders can only be sent on the same day as the delivery bill.".into(),
                    ));
                }
            }
        }
    }

    if !is_network_available().await {
        return Err(AppError::Internal("WhatsApp: device is offline".into()));
    }

    let phone = normalize_phone(&input.to);
    if phone.is_empty() {
        return Err(AppError::Validation(
            "Recipient phone number is required".into(),
        ));
    }

    whatsapp_payment_reminder_impl(
        &state,
        &phone,
        &input.receipt_number,
        input.amount_minor,
        input.currency_exponent,
        &input.currency,
    )
    .await?;

    Ok(true)
}

// ─── Contact import ──────────────────────────────────────────────────────────

/// One contact entry returned by the sidecar GET /contacts endpoint.
#[derive(Debug, Deserialize)]
struct SidecarContact {
    id: String,   // e.g. "97333050666@s.whatsapp.net"
    name: String, // push name or address-book name
}

/// Result returned to the frontend after a contact import run.
#[derive(Debug, Serialize)]
pub struct ImportContactsResult {
    pub imported: usize,
    pub skipped: usize,
    pub total: usize,
}

/// Fetch all WhatsApp contacts from the sidecar and import them into the
/// `customers` table.  Duplicates (matched by phone number) are silently
/// skipped via INSERT OR IGNORE on the UNIQUE index added in migration 0023.
#[tauri::command]
pub async fn whatsapp_import_contacts(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<ImportContactsResult> {
    // Only managers and owners may import contacts.
    crate::commands::rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    // Fetch contacts from the Node sidecar.
    let token = read_sidecar_token(&state);
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();
    let resp = client
        .get(format!("{}/contacts", SIDECAR_URL))
        .header("X-Sidecar-Token", &token)
        .send()
        .await
        .map_err(|e| crate::errors::AppError::Internal(format!("Sidecar unreachable: {e}")))?;

    if !resp.status().is_success() {
        return Err(crate::errors::AppError::Internal(
            "Failed to fetch contacts from sidecar".into(),
        ));
    }

    let contacts: Vec<SidecarContact> = resp
        .json()
        .await
        .map_err(|e| crate::errors::AppError::Internal(format!("Bad contacts response: {e}")))?;

    // Resolve the active branch.
    let branch_id: String = sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .flatten()
    .ok_or_else(|| crate::errors::AppError::Internal("No active branch found".into()))?;

    let total = contacts.len();
    let mut imported = 0usize;

    for contact in &contacts {
        // Strip @s.whatsapp.net to get the bare phone number.
        let bare = contact.id.split('@').next().unwrap_or("").trim();
        if bare.is_empty() {
            continue;
        }
        // Normalise: ensure the number is prefixed with '+', strip double country codes.
        let phone = normalize_phone(bare);

        let customer_id = Ulid::new().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let rows = sqlx::query(
            "INSERT OR IGNORE INTO customers \
             (customer_id, branch_id, name, phone, loyalty_points, created_at, updated_at) \
             VALUES (?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(&customer_id)
        .bind(&branch_id)
        .bind(&contact.name)
        .bind(&phone)
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await?
        .rows_affected();

        if rows > 0 {
            imported += 1;
        }
    }

    Ok(ImportContactsResult {
        imported,
        skipped: total - imported,
        total,
    })
}

// ─── Impl helpers (pub(crate), for delivery-triggered calls) ──────────────────

/// Best-effort delivery notification — callable from delivery_commands.
/// Silently returns Ok(()) when offline so delivery status updates are not blocked.
pub(crate) async fn whatsapp_send_delivery_impl(
    state: &AppState,
    phone: &str,
    receipt_number: &str,
    net_total_minor: i64,
    currency_exponent: i32,
    address_text: &str,
    house_number: Option<&str>,
    area: Option<&str>,
    message_override: Option<&str>,
) -> AppResult<()> {
    if !is_network_available().await {
        tracing::debug!("whatsapp_send_delivery_impl: offline, skipping");
        return Ok(()); // Best-effort — don't fail the delivery status update
    }
    let phone = normalize_phone(phone);
    if phone.is_empty() {
        tracing::debug!("whatsapp_send_delivery_impl: no phone number, skipping");
        return Ok(());
    }

    let benefit_number: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'whatsapp_benefit_number'")
            .fetch_optional(&state.db)
            .await?
            .flatten();

    let store_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM branches WHERE is_active = 1 LIMIT 1")
            .fetch_optional(&state.db)
            .await?
            .flatten();

    let store_phone: Option<String> =
        sqlx::query_scalar("SELECT phone FROM branches WHERE is_active = 1 LIMIT 1")
            .fetch_optional(&state.db)
            .await?
            .flatten();

    // Use frontend-built message if provided, otherwise fall back to Rust builder
    let message = message_override.map(|s| s.to_string()).unwrap_or_else(|| {
        build_delivery_whatsapp_message(&WhatsAppDeliveryParams {
            receipt_number,
            net_total_minor,
            currency_exponent,
            address_text,
            house_number,
            area,
            store_name: store_name.as_deref().unwrap_or(""),
            store_phone: store_phone.as_deref(),
            benefit_number: benefit_number.as_deref(),
        })
    });

    let token = read_sidecar_token(state);
    send_raw(&phone, &message, &token).await?;
    Ok(())
}

/// Best-effort arrival notification — callable from delivery_commands.
/// Silently returns Ok(()) when offline.
pub(crate) async fn whatsapp_notify_arrival_impl(
    state: &AppState,
    phone: &str,
    receipt_number: &str,
) -> AppResult<()> {
    if !is_network_available().await {
        tracing::debug!("whatsapp_notify_arrival_impl: offline, skipping");
        return Ok(());
    }
    let phone = normalize_phone(phone);
    if phone.is_empty() {
        tracing::debug!("whatsapp_notify_arrival_impl: no phone number, skipping");
        return Ok(());
    }

    let message = format!(
        "🚚 Your delivery is here!\n\
         The delivery man is outside. Please come out to collect your order.\n\n\
         Order #{r}\n\
         ─────────────────\n\
         🚚 طلبك وصل!\n\
         عامل التوصيل في الخارج. من فضلك انزل لاستلام طلبك.\n\n\
         طلب #{r}",
        r = receipt_number,
    );
    let token = read_sidecar_token(state);
    send_raw(&phone, &message, &token).await?;
    Ok(())
}

/// Best-effort payment reminder — callable from delivery_commands.
/// Silently returns Ok(()) when offline.
pub(crate) async fn whatsapp_payment_reminder_impl(
    state: &AppState,
    phone: &str,
    receipt_number: &str,
    amount_minor: i64,
    currency_exponent: i32,
    currency: &str,
) -> AppResult<()> {
    if !is_network_available().await {
        tracing::debug!("whatsapp_payment_reminder_impl: offline, skipping");
        return Ok(());
    }
    let phone = normalize_phone(phone);
    if phone.is_empty() {
        tracing::debug!("whatsapp_payment_reminder_impl: no phone number, skipping");
        return Ok(());
    }

    let benefit_number: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'whatsapp_benefit_number'")
            .fetch_optional(&state.db)
            .await?
            .flatten();

    let amount = fmt_money(amount_minor, currency_exponent);

    let message = match benefit_number.as_deref() {
        Some(bn) => format!(
            "💳 Payment Reminder — Order #{r}\n\
             Amount due: {cur} {amount}\n\n\
             Please send payment via BenefitPay to: {bn}\n\
             Then reply with a screenshot of your payment receipt to confirm. 🧾\n\n\
             Thank you! 🙏\n\
             ─────────────────\n\
             💳 تذكير بالدفع — طلب #{r}\n\
             المبلغ المستحق: {amount} {cur}\n\n\
             يرجى إرسال المبلغ عبر BenefitPay إلى: {bn}\n\
             ثم أرسل لنا صورة من إيصال الدفع للتأكيد. 🧾\n\n\
             شكراً! 🙏",
            r = receipt_number,
            cur = currency,
            amount = amount,
            bn = bn,
        ),
        None => format!(
            "💳 Payment Reminder — Order #{r}\n\
             Amount due: {cur} {amount}\n\n\
             Please reply with a screenshot of your payment receipt to confirm. 🧾\n\n\
             Thank you! 🙏\n\
             ─────────────────\n\
             💳 تذكير بالدفع — طلب #{r}\n\
             المبلغ المستحق: {amount} {cur}\n\n\
             يرجى إرسال لنا صورة من إصمال الدفع للتأكيد. 🧾\n\n\
             شكراً! 🙏",
            r = receipt_number,
            cur = currency,
            amount = amount,
        ),
    };
    let token = read_sidecar_token(state);
    send_raw(&phone, &message, &token).await?;
    Ok(())
}

/// Check whether the sidecar is alive and WhatsApp session is active.
/// Returns Err with a user-friendly message if the sidecar is unreachable or
/// the session has expired and needs QR re-scanning.
async fn sidecar_health_check(token: &str) -> AppResult<()> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(3))
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();
    match client
        .get(format!("{}/status", SIDECAR_URL))
        .header("X-Sidecar-Token", token)
        .send()
        .await
    {
        Ok(resp) => {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let status = body.get("status").and_then(|v| v.as_str()).unwrap_or("");
            let connected = body
                .get("connected")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !connected || status == "DISCONNECTED" || status == "QR_REQUIRED" {
                return Err(AppError::Internal(
                    "WhatsApp session expired. Please re-scan the QR code in Settings.".into(),
                ));
            }
            Ok(())
        }
        Err(e) => {
            tracing::warn!("WhatsApp sidecar health check failed: {}", e);
            Err(AppError::Internal(
                "WhatsApp service is unavailable. Please restart the application.".into(),
            ))
        }
    }
}

/// Internal helper: POST a raw message to the sidecar /send endpoint.
/// Performs a health check first, then attempts the send once; on failure retries
/// once after a ~2 s delay. Returns Ok(true) on confirmed send, Err otherwise.
/// R-15: the underlying cause is always logged so a failure is diagnosable.
async fn send_raw(to: &str, message: &str, token: &str) -> AppResult<bool> {
    // Health check — surface sidecar-dead or QR-expired before attempting send.
    sidecar_health_check(token).await?;

    // Apply a bounded timeout so a stuck sidecar can't hang the caller.
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .unwrap_or_default();

    let attempt = |c: &reqwest::Client| {
        c.post(format!("{}/send", SIDECAR_URL))
            .header("X-Sidecar-Token", token)
            .json(&serde_json::json!({ "to": to, "message": message }))
            .send()
    };

    // First attempt
    let result = attempt(&client).await;

    // On network failure, wait ~2 s and retry once.
    let resp = match result {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                "WhatsApp send attempt 1 to '{}' failed: {} — retrying in 2s",
                to,
                e
            );
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            attempt(&client).await.map_err(|e2| {
                tracing::warn!("WhatsApp send attempt 2 to '{}' failed: {}", to, e2);
                AppError::Internal(format!("WhatsApp send failed after retry: {}", e2))
            })?
        }
    };

    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    let ok = body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if !ok {
        tracing::warn!(
            "WhatsApp send to '{}' rejected by sidecar: {}",
            to,
            body.get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown reason")
        );
    }
    Ok(ok)
}

// ─── Receipt PDF send ─────────────────────────────────────────────────────────

/// Send a WhatsApp message that contains a PDF receipt as an attached document,
/// with the existing text message as the caption — both delivered in one message.
///
/// Performs a sidecar health check first:
///   - If the sidecar is dead or QR-expired, returns an **actionable error** so the
///     user knows to restart or re-scan the QR (text-only fallback won't work either).
///   - If the sidecar is alive but the /send-document endpoint fails or returns
///     `ok: false`, returns `Ok(false)` so the caller can fall back to text-only.
#[tauri::command]
pub async fn whatsapp_send_receipt_pdf(
    input: crate::commands::receipt_pdf::WhatsAppReceiptPdfInput,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<bool> {
    use base64::Engine as _;

    rbac::require_any_role(&state.db, &actor_user_id).await?;

    if !is_network_available().await {
        return Err(AppError::Internal("WhatsApp: device is offline".into()));
    }

    let phone = normalize_phone(&input.to);

    let token = read_sidecar_token(&state);
    // Health check — surface sidecar-dead or QR-expired before attempting send.
    // If the sidecar is actually down, text-only fallback is also impossible, so
    // we return an error rather than silently returning false.
    sidecar_health_check(&token).await?;

    let pdf_bytes = crate::commands::receipt_pdf::generate_receipt_pdf(&input)
        .map_err(|e| AppError::Internal(format!("PDF generation failed: {e}")))?;

    let pdf_b64 = base64::engine::general_purpose::STANDARD.encode(&pdf_bytes);
    let filename = format!("Receipt-{}.pdf", input.receipt_number);
    let caption = input.caption.as_deref().unwrap_or("").to_string();

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    match client
        .post(format!("{}/send-document", SIDECAR_URL))
        .header("X-Sidecar-Token", &token)
        .json(&serde_json::json!({
            "to":              phone,
            "caption":         caption,
            "document_base64": pdf_b64,
            "mimetype":        "application/pdf",
            "filename":        filename,
        }))
        .send()
        .await
    {
        Ok(resp) => {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let ok = body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
            if !ok {
                tracing::warn!(
                    "WhatsApp PDF send to '{}' rejected: {}",
                    phone,
                    body.get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                );
            }
            Ok(ok)
        }
        Err(e) => {
            tracing::warn!(
                "WhatsApp PDF send to '{}' failed after health check passed: {}",
                phone,
                e
            );
            Ok(false)
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn base_params<'a>() -> WhatsAppDeliveryParams<'a> {
        WhatsAppDeliveryParams {
            receipt_number: "0042",
            net_total_minor: 1500,
            currency_exponent: 3,
            address_text: "Block 5, Road 123",
            house_number: Some("12"),
            area: Some("Riffa"),
            store_name: "ZAN Café",
            store_phone: Some("+97317001234"),
            benefit_number: Some("33050666"),
        }
    }

    #[test]
    fn test_message_contains_receipt_number() {
        let msg = build_delivery_whatsapp_message(&base_params());
        assert!(msg.contains("#0042"), "must contain receipt number");
    }

    #[test]
    fn test_message_contains_formatted_total() {
        let msg = build_delivery_whatsapp_message(&base_params());
        assert!(
            msg.contains("1.500"),
            "BHD with 3 decimals: 1500 minor → 1.500"
        );
    }

    #[test]
    fn test_message_contains_benefit_number() {
        let msg = build_delivery_whatsapp_message(&base_params());
        assert!(msg.contains("33050666"), "must contain benefit number");
    }

    #[test]
    fn test_message_contains_arabic_section() {
        let msg = build_delivery_whatsapp_message(&base_params());
        assert!(
            msg.contains("تم تأكيد"),
            "must contain Arabic confirmation text"
        );
        assert!(msg.contains("BenefitPay"), "benefit section in Arabic too");
    }

    #[test]
    fn test_message_omits_location_when_empty() {
        let params = WhatsAppDeliveryParams {
            house_number: None,
            area: None,
            ..base_params()
        };
        let msg = build_delivery_whatsapp_message(&params);
        assert!(
            !msg.contains("🏠"),
            "no house emoji when house+area both absent"
        );
    }

    #[test]
    fn test_message_omits_benefit_when_not_configured() {
        let params = WhatsAppDeliveryParams {
            benefit_number: None,
            ..base_params()
        };
        let msg = build_delivery_whatsapp_message(&params);
        assert!(
            !msg.contains("BenefitPay"),
            "benefit section absent when unconfigured"
        );
    }

    #[test]
    fn test_fmt_money_3_decimals() {
        assert_eq!(fmt_money(1500, 3), "1.500");
        assert_eq!(fmt_money(1001, 3), "1.001");
        assert_eq!(fmt_money(500, 3), "0.500");
    }

    #[test]
    fn test_fmt_money_2_decimals() {
        assert_eq!(fmt_money(199, 2), "1.99");
        assert_eq!(fmt_money(100, 2), "1.00");
    }

    #[test]
    fn test_normalize_phone_strips_spaces_and_hyphens() {
        assert_eq!(normalize_phone("+973 3305 0666"), "+97333050666");
        assert_eq!(normalize_phone("973-3305-0666"), "+97333050666");
    }

    #[test]
    fn test_normalize_phone_double_country_code() {
        // +97397333050666 → +97333050666
        assert_eq!(normalize_phone("+97397333050666"), "+97333050666");
        // +966966501234567 → +966501234567
        assert_eq!(normalize_phone("+966966501234567"), "+966501234567");
    }

    #[test]
    fn test_normalize_phone_already_normal() {
        assert_eq!(normalize_phone("+97333050666"), "+97333050666");
    }

    #[test]
    fn test_normalize_phone_no_plus() {
        assert_eq!(normalize_phone("97333050666"), "+97333050666");
    }

    #[test]
    fn test_normalize_phone_empty() {
        assert_eq!(normalize_phone(""), "");
    }

    #[test]
    fn test_normalize_phone_all_symbols() {
        assert_eq!(normalize_phone("  - "), "");
    }
}
