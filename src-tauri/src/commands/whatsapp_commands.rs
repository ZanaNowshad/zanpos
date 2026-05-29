use crate::errors::AppResult;
use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;
use ulid::Ulid;

const SIDECAR_URL: &str = "http://127.0.0.1:3131";

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct WhatsAppStatus {
    pub connected: bool,
    pub qr: Option<String>, // base64 PNG data URL: "data:image/png;base64,..."
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
    if exp == 0 {
        return minor.to_string();
    }
    let divisor = 10_i64.pow(exp as u32);
    let whole = minor / divisor;
    let frac = minor % divisor;
    format!("{}.{:0>width$}", whole, frac.abs(), width = exp as usize)
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
pub async fn whatsapp_status(state: State<'_, AppState>) -> AppResult<WhatsAppStatus> {
    let token = read_sidecar_token(&state);
    let client = reqwest::Client::new();
    match client
        .get(format!("{}/status", SIDECAR_URL))
        .header("X-Sidecar-Token", &token)
        .send()
        .await
    {
        Ok(resp) => Ok(resp
            .json::<WhatsAppStatus>()
            .await
            .unwrap_or(WhatsAppStatus {
                connected: false,
                qr: None,
            })),
        Err(_) => Ok(WhatsAppStatus {
            connected: false,
            qr: None,
        }),
    }
}

#[tauri::command]
pub async fn whatsapp_send_delivery(
    input: SendDeliveryInput,
    state: State<'_, AppState>,
) -> AppResult<bool> {
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
    let message = input.message_override.clone().unwrap_or_else(|| {
        build_delivery_whatsapp_message(&WhatsAppDeliveryParams {
            receipt_number: &input.receipt_number,
            net_total_minor: input.net_total_minor,
            currency_exponent: input.currency_exponent,
            address_text: &input.address_text,
            house_number: input.house_number.as_deref(),
            area: input.area.as_deref(),
            store_name: store_name.as_deref().unwrap_or(""),
            store_phone: store_phone.as_deref(),
            benefit_number: benefit_number.as_deref(),
        })
    });

    let token = read_sidecar_token(&state);
    send_raw(&input.to, &message, &token).await
}

#[tauri::command]
pub async fn whatsapp_disconnect(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<bool> {
    crate::commands::rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let token = read_sidecar_token(&state);
    let client = reqwest::Client::new();
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
    sqlx::query(
        "INSERT OR REPLACE INTO app_config (key, value) VALUES ('whatsapp_benefit_number', ?)",
    )
    .bind(&benefit_number)
    .execute(&state.db)
    .await?;
    Ok(())
}

// ─── Notify arrival ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NotifyArrivalInput {
    pub to: String,
    pub receipt_number: String,
}

/// Sends a short bilingual WhatsApp message: "Delivery is outside, please come collect."
#[tauri::command]
pub async fn whatsapp_notify_arrival(
    input: NotifyArrivalInput,
    state: State<'_, AppState>,
) -> AppResult<bool> {
    let message = format!(
        "🚚 Your delivery is here!\n\
         The delivery man is outside. Please come out to collect your order.\n\n\
         Order #{r}\n\
         ─────────────────\n\
         🚚 طلبك وصل!\n\
         عامل التوصيل في الخارج. من فضلك انزل لاستلام طلبك.\n\n\
         طلب #{r}",
        r = input.receipt_number,
    );
    let token = read_sidecar_token(&state);
    send_raw(&input.to, &message, &token).await
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
}

/// Sends a bilingual WhatsApp payment reminder that includes the store's
/// BenefitPay number (fetched from app_config) and the outstanding amount.
#[tauri::command]
pub async fn whatsapp_payment_reminder(
    input: PaymentReminderInput,
    state: State<'_, AppState>,
) -> AppResult<bool> {
    let benefit_number: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'whatsapp_benefit_number'")
            .fetch_optional(&state.db)
            .await?
            .flatten();

    let amount = fmt_money(input.amount_minor, input.currency_exponent);

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
            r = input.receipt_number,
            cur = input.currency,
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
             يرجى إرسال لنا صورة من إيصال الدفع للتأكيد. 🧾\n\n\
             شكراً! 🙏",
            r = input.receipt_number,
            cur = input.currency,
            amount = amount,
        ),
    };
    let token = read_sidecar_token(&state);
    send_raw(&input.to, &message, &token).await
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
    let client = reqwest::Client::new();
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
        // Normalise: ensure the number is prefixed with '+'.
        let phone = if bare.starts_with('+') {
            bare.to_string()
        } else {
            format!("+{}", bare)
        };

        let customer_id = Ulid::new().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let rows = sqlx::query(
            "INSERT OR IGNORE INTO customers \
             (customer_id, branch_id, name, phone, loyalty_points, created_at) \
             VALUES (?, ?, ?, ?, 0, ?)",
        )
        .bind(&customer_id)
        .bind(&branch_id)
        .bind(&contact.name)
        .bind(&phone)
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

/// Internal helper: POST a raw message to the sidecar /send endpoint.
async fn send_raw(to: &str, message: &str, token: &str) -> AppResult<bool> {
    let client = reqwest::Client::new();
    match client
        .post(format!("{}/send", SIDECAR_URL))
        .header("X-Sidecar-Token", token)
        .json(&serde_json::json!({ "to": to, "message": message }))
        .send()
        .await
    {
        Ok(resp) => {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            Ok(body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false))
        }
        Err(_) => Ok(false),
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
}
