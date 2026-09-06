//! Phase 1 — AI payment verification (WhatsApp screenshot → local OCR → AI confirm).
//!
//! When a delivery receipt is sent over WhatsApp (`whatsapp_send_delivery`), a
//! `pending` row is recorded here. When that customer replies with a payment
//! screenshot, the inbox poll (`whatsapp_poll_messages`) calls `run_verification`:
//! the sidecar runs deterministic local OCR, then the AI judges only the extracted
//! TEXT (amount == expected AND recipient == business). On a match the matching
//! delivery is marked paid via the existing `delivery_repo::confirm_payment`, and
//! the resolved row surfaces on the POS Notification panel.

use crate::commands::rbac;
use crate::commands::whatsapp_commands::{read_sidecar_token, SIDECAR_URL};
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use std::time::Duration;
use tauri::State;
use ulid::Ulid;

#[derive(Debug, Serialize)]
pub struct PaymentConfirmation {
    pub id: String,
    pub customer_jid: String,
    pub customer_name: Option<String>,
    pub receipt_number: String,
    pub expected_amount_minor: i64,
    pub currency_exponent: i32,
    pub amount_found: Option<String>,
    pub name_matched: bool,
    pub status: String,
    pub reason: Option<String>,
    pub seen: bool,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(60)) // OCR can take a few seconds on first run
        .build()
        .unwrap_or_default()
}

/// WhatsApp 1:1 JID for a phone number (digits + @s.whatsapp.net), matching the
/// `chat_jid` the sidecar reports for inbound customer messages.
pub(crate) fn jid_for_phone(phone: &str) -> String {
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
    format!("{digits}@s.whatsapp.net")
}

// ── Recording (called on receipt send) ────────────────────────────────────────

/// Record a pending confirmation. Any older still-pending row for the same
/// customer is superseded so only the latest receipt is matched next.
pub(crate) async fn record_pending(
    db: &SqlitePool,
    customer_jid: &str,
    receipt_number: &str,
    expected_amount_minor: i64,
    currency_exponent: i32,
    business_name: &str,
    branch_id: Option<&str>,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE payment_confirmations SET status='failed', reason='superseded by newer receipt', resolved_at=? \
         WHERE customer_jid=? AND status='pending'",
    )
    .bind(&now)
    .bind(customer_jid)
    .execute(db)
    .await
    .ok();
    sqlx::query(
        "INSERT INTO payment_confirmations \
         (id, customer_jid, receipt_number, expected_amount_minor, currency_exponent, business_name, branch_id, status, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', ?)",
    )
    .bind(Ulid::new().to_string())
    .bind(customer_jid)
    .bind(receipt_number)
    .bind(expected_amount_minor)
    .bind(currency_exponent)
    .bind(business_name)
    .bind(branch_id)
    .bind(&now)
    .execute(db)
    .await?;
    Ok(())
}

/// Customer JIDs with an open pending confirmation — the inbox poll only runs
/// verification for image messages from these chats (and uses this to decide
/// whether to poll at all when no owner/group is configured).
pub(crate) async fn pending_jids(db: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT DISTINCT customer_jid FROM payment_confirmations WHERE status='pending'",
    )
    .fetch_all(db)
    .await
    .unwrap_or_default()
}

// ── Verification pipeline (called from the inbox poll) ─────────────────────────

#[derive(serde::Deserialize)]
struct OcrResp {
    ok: bool,
    text: Option<String>,
    error: Option<String>,
}

struct Verdict {
    matched: bool,
    amount_found: Option<String>,
    name_matched: bool,
    customer_name: Option<String>,
    reason: Option<String>,
}

/// Best-effort entry point. Logs and leaves the row pending on any failure so a
/// later screenshot/poll can retry.
pub(crate) async fn run_verification(
    state: &AppState,
    customer_jid: &str,
    media_id: &str,
    sender_name: Option<&str>,
) {
    if let Err(e) = run_verification_inner(state, customer_jid, media_id, sender_name).await {
        tracing::warn!("payment verification failed for {customer_jid}: {e}");
    }
}

async fn run_verification_inner(
    state: &AppState,
    customer_jid: &str,
    media_id: &str,
    sender_name: Option<&str>,
) -> AppResult<()> {
    let db = &state.db;
    let row = sqlx::query(
        "SELECT id, receipt_number, expected_amount_minor, currency_exponent, business_name \
         FROM payment_confirmations WHERE customer_jid=? AND status='pending' \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(customer_jid)
    .fetch_optional(db)
    .await?;
    let Some(row) = row else { return Ok(()) };
    let conf_id: String = row.get("id");
    let receipt_number: String = row.get("receipt_number");
    let expected_minor: i64 = row.get("expected_amount_minor");
    let exp: i64 = row.get("currency_exponent");
    let business_name: String = row.get("business_name");

    // 1) Deterministic local OCR via the sidecar.
    let token = read_sidecar_token(state);
    let ocr: OcrResp = client()
        .get(format!("{}/ocr", SIDECAR_URL))
        .query(&[("id", media_id)])
        .header("X-Sidecar-Token", &token)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("OCR request failed: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("OCR bad response: {e}")))?;
    if !ocr.ok {
        return Err(AppError::Internal(
            ocr.error.unwrap_or_else(|| "OCR unavailable".into()),
        ));
    }
    let ocr_text = ocr.text.unwrap_or_default();

    // 2) Deterministic fast path for clear BenefitPay OCR. This keeps payment
    // confirmation working even when the model is unavailable for obvious matches.
    let expected_str = crate::domain::money::format_minor(expected_minor, exp as u32);
    let verdict = match deterministic_verdict(&ocr_text, &expected_str, &business_name) {
        Some(v) => v,
        None => ai_verify(db, &ocr_text, &expected_str, &business_name).await?,
    };

    // 3) Resolve the row.
    let now = chrono::Utc::now().to_rfc3339();
    let customer_name = sender_name
        .map(|s| s.to_string())
        .or_else(|| verdict.customer_name.clone());
    let status = if verdict.matched {
        "confirmed"
    } else {
        "failed"
    };
    sqlx::query(
        "UPDATE payment_confirmations \
         SET status=?, ocr_text=?, amount_found=?, name_matched=?, customer_name=?, reason=?, resolved_at=? \
         WHERE id=?",
    )
    .bind(status)
    .bind(&ocr_text)
    .bind(&verdict.amount_found)
    .bind(verdict.name_matched as i64)
    .bind(&customer_name)
    .bind(&verdict.reason)
    .bind(&now)
    .bind(&conf_id)
    .execute(db)
    .await?;

    if verdict.matched {
        mark_delivery_paid(
            db,
            &receipt_number,
            &conf_id,
            "zanai-auto",
            "Auto-confirmed via OCR + AI match",
        )
        .await;
        tracing::info!(
            "ZanAI confirmed payment for receipt {receipt_number} from {}",
            customer_name.as_deref().unwrap_or(customer_jid)
        );
    }
    Ok(())
}

async fn ai_verify(
    db: &SqlitePool,
    ocr_text: &str,
    expected_amount: &str,
    business_name: &str,
) -> AppResult<Verdict> {
    let provider = crate::ai::provider::Provider::from_db_with_fallback(db)
        .await?
        .ok_or_else(|| {
            AppError::Internal("No AI provider configured for payment verification".into())
        })?;
    let params = crate::ai::config::load_ai_params(db).await;
    let system = "You verify a payment-transfer screenshot. You are given the raw OCR text of \
        the screenshot, an expected amount, and the recipient business name. Numbers may use \
        Arabic-Indic digits; ignore OCR noise and currency formatting. Respond with ONLY a compact \
        JSON object and nothing else: \
        {\"matched\":boolean,\"amount_found\":string,\"name_matched\":boolean,\"customer_name\":string,\"reason\":string}. \
        Set matched=true ONLY if BOTH the paid amount equals the expected amount AND the recipient \
        clearly matches the business name. customer_name is the sender's name if visible, else \"\".";
    let user = format!(
        "Expected amount: {expected_amount}\nRecipient business name: {business_name}\n\nOCR text of the screenshot:\n\"\"\"\n{ocr_text}\n\"\"\"",
    );
    let result = provider
        .send_chat(system, &[], &user, &[], params.context_window_chars, None)
        .await?;
    parse_verdict(&result.text)
}

/// Extract the first balanced `{...}` block and parse the verdict. Tolerant of
/// code fences or prose around the JSON.
fn parse_verdict(text: &str) -> AppResult<Verdict> {
    let json = extract_json(text)
        .ok_or_else(|| AppError::Internal("AI returned no JSON verdict".into()))?;
    let v: serde_json::Value = serde_json::from_str(&json)
        .map_err(|e| AppError::Internal(format!("verdict parse: {e}")))?;
    Ok(Verdict {
        matched: v.get("matched").and_then(|x| x.as_bool()).unwrap_or(false),
        amount_found: v
            .get("amount_found")
            .and_then(|x| x.as_str())
            .map(String::from),
        name_matched: v
            .get("name_matched")
            .and_then(|x| x.as_bool())
            .unwrap_or(false),
        customer_name: v
            .get("customer_name")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from),
        reason: v.get("reason").and_then(|x| x.as_str()).map(String::from),
    })
}

fn extract_json(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end > start).then(|| text[start..=end].to_string())
}

fn normalize_digits(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '٠' | '۰' => '0',
            '١' | '۱' => '1',
            '٢' | '۲' => '2',
            '٣' | '۳' => '3',
            '٤' | '۴' => '4',
            '٥' | '۵' => '5',
            '٦' | '۶' => '6',
            '٧' | '۷' => '7',
            '٨' | '۸' => '8',
            '٩' | '۹' => '9',
            _ => c,
        })
        .collect()
}

fn amount_digits(amount: &str) -> String {
    normalize_digits(amount)
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect()
}

/// An amount as OCR read it: the digits, and how many followed the last
/// separator.
///
/// The digits alone are not the amount. Stripping separators made `1.500` and
/// `15.00` the same string, so a screenshot of a 15.00 payment satisfied a 1.500
/// expectation — the check that is supposed to confirm the customer paid the
/// right amount could not see where the decimal point was.
///
/// The separator *character* still has to be ignored: OCR confuses `.`, `,`, `:`
/// and the Arabic decimal mark freely. What it cannot ignore is the separator's
/// *position*, which is what `decimals` records. `1,500` still matches `1.500` —
/// same digits, same three decimal places, just a comma the scanner read instead
/// of a point — while `15.00` no longer does.
#[derive(Debug, PartialEq, Eq)]
struct OcrAmount {
    digits: String,
    decimals: usize,
}

fn parse_ocr_amount(token: &str) -> Option<OcrAmount> {
    let normalized = normalize_digits(token);
    let digits = amount_digits(&normalized);
    if digits.is_empty() {
        return None;
    }
    let decimals = match normalized.rfind(['.', ',', ':', '٫']) {
        Some(at) => normalized[at + '٫'.len_utf8()..]
            .chars()
            .filter(|c| c.is_ascii_digit())
            .count(),
        None => 0,
    };
    Some(OcrAmount { digits, decimals })
}

fn ocr_amount_tokens(text: &str) -> Vec<OcrAmount> {
    let normalized = normalize_digits(text);
    let mut tokens = Vec::new();
    let mut current = String::new();
    for c in normalized.chars() {
        if c.is_ascii_digit() || matches!(c, '.' | ',' | ':' | '٫') {
            current.push(c);
        } else if !current.is_empty() {
            if let Some(amount) = parse_ocr_amount(&current) {
                tokens.push(amount);
            }
            current.clear();
        }
    }
    if !current.is_empty() {
        if let Some(amount) = parse_ocr_amount(&current) {
            tokens.push(amount);
        }
    }
    tokens
}

fn normalize_name(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter_map(|c| match c {
            'a'..='z' | '0'..='9' => Some(c),
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => Some('a'),
            'è' | 'é' | 'ê' | 'ë' => Some('e'),
            'ì' | 'í' | 'î' | 'ï' => Some('i'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => Some('o'),
            'ù' | 'ú' | 'û' | 'ü' => Some('u'),
            'ç' => Some('c'),
            'ñ' => Some('n'),
            _ => None,
        })
        .collect()
}

fn deterministic_verdict(
    ocr_text: &str,
    expected_amount: &str,
    business_name: &str,
) -> Option<Verdict> {
    let expected = parse_ocr_amount(expected_amount)?;
    if !ocr_amount_tokens(ocr_text).contains(&expected) {
        return None;
    }

    let business = normalize_name(business_name);
    if business.len() < 3 {
        return None;
    }
    let ocr_name = normalize_name(ocr_text);
    if !ocr_name.contains(&business) {
        return None;
    }

    Some(Verdict {
        matched: true,
        amount_found: Some(expected_amount.to_string()),
        name_matched: true,
        customer_name: None,
        reason: Some("Deterministic OCR match: expected amount and business name found".into()),
    })
}

/// Mark the delivery for a receipt paid using the existing idempotent repo fn.
/// Best-effort: a receipt without a delivery row simply records no mark-paid.
///
/// `confirmed_by` is who actually decided, not who usually does. It was hardcoded
/// to the automatic verifier, so a manager overriding a rejected screenshot was
/// recorded in `delivery_orders.paid_confirmed_by_user_id` and in the audit entry
/// as `zanai-auto`. The question that gets asked afterwards is who accepted this
/// payment, and the trail answered with the wrong name.
async fn mark_delivery_paid(
    db: &SqlitePool,
    receipt_number: &str,
    conf_id: &str,
    confirmed_by: &str,
    how: &str,
) {
    let delivery_id: Option<String> = sqlx::query_scalar(
        "SELECT delivery_id FROM delivery_orders WHERE receipt_number=? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(receipt_number)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    if let Some(did) = delivery_id {
        let input = crate::domain::delivery::ConfirmPaymentInput {
            delivery_id: did.clone(),
            payment_reference: Some("WhatsApp screenshot".into()),
            payment_note: Some(how.to_string()),
        };
        if let Err(e) =
            crate::db::repositories::delivery_repo::confirm_payment(db, confirmed_by, &input).await
        {
            tracing::warn!("payment matched but delivery {did} mark-paid failed: {e}");
        }
        sqlx::query("UPDATE payment_confirmations SET delivery_id=? WHERE id=?")
            .bind(&did)
            .bind(conf_id)
            .execute(db)
            .await
            .ok();
    }
}

// ── Tauri commands (Notification panel) ────────────────────────────────────────

fn row_to_conf(r: &sqlx::sqlite::SqliteRow) -> PaymentConfirmation {
    PaymentConfirmation {
        id: r.get("id"),
        customer_jid: r.get("customer_jid"),
        customer_name: r.get("customer_name"),
        receipt_number: r.get("receipt_number"),
        expected_amount_minor: r.get("expected_amount_minor"),
        currency_exponent: r.get::<i64, _>("currency_exponent") as i32,
        amount_found: r.get("amount_found"),
        name_matched: r.get::<i64, _>("name_matched") != 0,
        status: r.get("status"),
        reason: r.get("reason"),
        seen: r.get::<i64, _>("seen") != 0,
        created_at: r.get("created_at"),
        resolved_at: r.get("resolved_at"),
    }
}

#[tauri::command]
pub async fn payment_confirmations_list(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<PaymentConfirmation>> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    let rows = sqlx::query(
        "SELECT id, customer_jid, customer_name, receipt_number, expected_amount_minor, \
                currency_exponent, amount_found, name_matched, status, reason, seen, created_at, resolved_at \
         FROM payment_confirmations WHERE status IN ('confirmed','failed') \
         ORDER BY seen ASC, COALESCE(resolved_at, created_at) DESC LIMIT 50",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(rows.iter().map(row_to_conf).collect())
}

#[tauri::command]
pub async fn payment_confirmations_unseen_count(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<i64> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM payment_confirmations WHERE status IN ('confirmed','failed') AND seen=0",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0))
}

#[tauri::command]
pub async fn payment_confirmations_mark_all_seen(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    sqlx::query("UPDATE payment_confirmations SET seen=1 WHERE seen=0")
        .execute(&state.db)
        .await?;
    Ok(())
}

/// Manager/owner manual override for a confirmation the AI couldn't verify.
#[tauri::command]
pub async fn payment_confirmation_override(
    id: String,
    confirm: bool,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let receipt: Option<String> =
        sqlx::query_scalar("SELECT receipt_number FROM payment_confirmations WHERE id=?")
            .bind(&id)
            .fetch_optional(&state.db)
            .await?;
    let now = chrono::Utc::now().to_rfc3339();
    let status = if confirm { "confirmed" } else { "failed" };
    let reason = format!(
        "Manually {} by {}",
        if confirm { "confirmed" } else { "rejected" },
        actor_user_id
    );
    sqlx::query(
        "UPDATE payment_confirmations SET status=?, reason=?, seen=1, resolved_at=? WHERE id=?",
    )
    .bind(status)
    .bind(&reason)
    .bind(&now)
    .bind(&id)
    .execute(&state.db)
    .await?;
    if confirm {
        if let Some(rn) = receipt {
            mark_delivery_paid(
                &state.db,
                &rn,
                &id,
                &actor_user_id,
                "Manually confirmed from the notification panel",
            )
            .await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    #[test]
    fn the_amount_check_sees_where_the_decimal_point_is() {
        let business = "ZAN Cafe W.L.L.";

        // The right amount, however the scanner rendered the separator.
        for rendered in [
            "BENEFITPAY Paid BHD 1.500 Recipient: ZAN Cafe W.L.L.",
            "BENEFITPAY Paid BHD 1,500 Recipient: ZAN Cafe W.L.L.",
            "BENEFITPAY Paid BHD 1:500 Recipient: ZAN Cafe W.L.L.",
        ] {
            assert!(
                deterministic_verdict(rendered, "1.500", business).is_some(),
                "a correct payment was rejected over the separator character: {rendered}"
            );
        }

        // Same digits, decimal point somewhere else. Stripping separators made
        // these identical, so a screenshot of a 15.00 payment confirmed a 1.500
        // bill — the amount check could not see the difference it exists to see.
        for wrong in [
            "BENEFITPAY Paid BHD 15.00 Recipient: ZAN Cafe W.L.L.",
            "BENEFITPAY Paid BHD 150.0 Recipient: ZAN Cafe W.L.L.",
        ] {
            assert!(
                deterministic_verdict(wrong, "1.500", business).is_none(),
                "a payment of a different amount was accepted: {wrong}"
            );
        }

        // And the business still has to be the recipient.
        assert!(
            deterministic_verdict(
                "BENEFITPAY Paid BHD 1.500 Recipient: Someone Else",
                "1.500",
                business,
            )
            .is_none(),
            "a payment to another recipient was accepted"
        );
    }

    use super::*;

    #[test]
    fn jid_from_phone_strips_symbols() {
        assert_eq!(
            jid_for_phone("+973 3305 0666"),
            "97333050666@s.whatsapp.net"
        );
        assert_eq!(jid_for_phone("97333050666"), "97333050666@s.whatsapp.net");
    }

    #[test]
    fn extract_json_handles_fences_and_prose() {
        assert_eq!(
            extract_json("```json\n{\"a\":1}\n```").as_deref(),
            Some("{\"a\":1}")
        );
        assert_eq!(
            extract_json("verdict: {\"matched\":true} done").as_deref(),
            Some("{\"matched\":true}")
        );
        assert_eq!(extract_json("no json here"), None);
    }

    #[test]
    fn parse_verdict_matched_and_mismatch() {
        let ok = parse_verdict("{\"matched\":true,\"amount_found\":\"12.500\",\"name_matched\":true,\"customer_name\":\"Ali\",\"reason\":\"ok\"}").unwrap();
        assert!(ok.matched && ok.name_matched);
        assert_eq!(ok.amount_found.as_deref(), Some("12.500"));
        assert_eq!(ok.customer_name.as_deref(), Some("Ali"));

        let no = parse_verdict("{\"matched\":false,\"name_matched\":false,\"customer_name\":\"\",\"reason\":\"amount differs\"}").unwrap();
        assert!(!no.matched);
        assert_eq!(no.customer_name, None);

        assert!(parse_verdict("garbage").is_err());
    }

    #[test]
    fn deterministic_verdict_matches_amount_and_business_name_without_ai() {
        let ocr = "BENEFITPAY\nPaid BHD 12.500\nRecipient: ZAN Cafe W.L.L.";
        let verdict = deterministic_verdict(ocr, "12.500", "ZAN Café").unwrap();

        assert!(verdict.matched);
        assert_eq!(verdict.amount_found.as_deref(), Some("12.500"));
        assert!(verdict.name_matched);
    }

    #[test]
    fn deterministic_verdict_rejects_wrong_amount() {
        let ocr = "Paid BHD 12.000\nRecipient: ZAN Cafe";

        assert!(deterministic_verdict(ocr, "12.500", "ZAN Cafe").is_none());
    }
}
