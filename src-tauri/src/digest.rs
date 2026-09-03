//! Evening digest: one composed WhatsApp summary sent to the store owner
//! after end-of-day, replacing the previous drip of proactive alerts.
//!
//! Triggered (fire-and-forget) from `commands::shift_commands::shift_close`
//! on every shift close. A per-business-date claim row in `app_config`
//! (see `claim_send_slot`) guarantees the digest goes out at most once a
//! day even when several shifts close on the same day, on one device or
//! several. Every failure path — missing config, offline, sidecar down,
//! query error — is logged and swallowed: this module must never block or
//! fail a shift close.

use crate::commands::whatsapp_commands::{read_sidecar_token, SIDECAR_URL};
use crate::AppState;
use sqlx::SqlitePool;
use std::time::Duration;

/// app_config key prefix; today's business date is appended so each day
/// claims its own row, e.g. "evening_digest_sent_2026-07-25".
const CLAIM_KEY_PREFIX: &str = "evening_digest_sent_";
const CLAIM_PENDING: &str = "pending";
const CLAIM_SENT: &str = "sent";

/// Entry point called from `shift_close`. Never propagates an error —
/// every step is best-effort so a WhatsApp or network problem can never
/// block or fail a shift close.
pub async fn maybe_send_evening_digest(state: &AppState, shift_id: &str) {
    let pool = &state.db;
    let business_date = shift_business_date(pool, shift_id).await;
    let claim_key = format!("{CLAIM_KEY_PREFIX}{business_date}");

    if !claim_send_slot(pool, &claim_key).await {
        return; // already sent (or in flight) for today — the duplicate-send guard
    }

    match send_digest_now(state, &business_date).await {
        Ok(()) => finalize_claim(pool, &claim_key).await,
        Err(e) => {
            tracing::warn!("evening digest: {e} — releasing today's claim for a later retry");
            release_claim(pool, &claim_key).await;
        }
    }
}

/// The shift's OWN business date, not today's calendar date. A shift that
/// opens in the evening and closes after local midnight still belongs to the
/// day it traded — reading the clock here would digest the wrong (empty) day
/// and claim the wrong slot. Falls back to the local date only when the
/// column is null, which is how pre-close rows are stored.
async fn shift_business_date(pool: &SqlitePool, shift_id: &str) -> String {
    sqlx::query_scalar::<_, Option<String>>("SELECT business_date FROM shifts WHERE shift_id = ?")
        .bind(shift_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten()
        .filter(|date| !date.is_empty())
        .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string())
}

// ── Duplicate-send guard ─────────────────────────────────────────────────────
//
// A claim is taken BEFORE sending (not after) so the once-per-day guarantee
// is atomic rather than relying on a check-then-write race. `app_config.key`
// is a PRIMARY KEY, so INSERT OR IGNORE is a safe compare-and-set even if two
// shift closes land at the same instant. If the send then fails, the claim
// is released so a later shift close the same day can retry; if the process
// crashes between claiming and releasing, the claim is left 'pending' and
// the digest is skipped for the rest of that day — an accepted rare edge case.

/// Atomically claims today's send slot. Returns true only if this call won
/// the claim (no digest sent or in flight yet for `claim_key`).
async fn claim_send_slot(pool: &SqlitePool, claim_key: &str) -> bool {
    let now = chrono::Utc::now().to_rfc3339();
    match sqlx::query("INSERT OR IGNORE INTO app_config (key, value, updated_at) VALUES (?, ?, ?)")
        .bind(claim_key)
        .bind(CLAIM_PENDING)
        .bind(&now)
        .execute(pool)
        .await
    {
        Ok(result) => result.rows_affected() > 0,
        Err(e) => {
            tracing::warn!("evening digest: claim check failed, skipping: {e}");
            false
        }
    }
}

async fn finalize_claim(pool: &SqlitePool, claim_key: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    let _ = sqlx::query("UPDATE app_config SET value = ?, updated_at = ? WHERE key = ?")
        .bind(CLAIM_SENT)
        .bind(&now)
        .bind(claim_key)
        .execute(pool)
        .await;
}

/// Releases a failed claim so a later shift close the same day can retry.
/// Only ever touches a 'pending' row — a finalized 'sent' claim is untouched.
async fn release_claim(pool: &SqlitePool, claim_key: &str) {
    let _ = sqlx::query("DELETE FROM app_config WHERE key = ? AND value = ?")
        .bind(claim_key)
        .bind(CLAIM_PENDING)
        .execute(pool)
        .await;
}

// ── Orchestration ────────────────────────────────────────────────────────────

async fn send_digest_now(state: &AppState, business_date: &str) -> Result<(), String> {
    let pool = &state.db;

    let (branch_id, currency) = active_branch(pool)
        .await
        .ok_or_else(|| "no active branch configured".to_string())?;
    let owner_phone = owner_phone(pool)
        .await
        .ok_or_else(|| "no WhatsApp owner configured in Settings".to_string())?;

    let net_total_minor = net_sales_minor(pool, &branch_id, business_date).await?;
    let top_item = top_item_today(pool, &branch_id, business_date).await?;
    let low_stock = low_stock_count(pool, &branch_id).await?;
    let currency_exponent = crate::domain::money::currency_exponent(&currency);

    let message = build_message(&DigestInputs {
        currency: &currency,
        currency_exponent,
        net_total_minor,
        top_item: top_item.as_deref(),
        low_stock_count: low_stock,
    });

    let token = read_sidecar_token(state);
    let ok = send_to_sidecar(&owner_phone, &message, &token).await?;
    if !ok {
        return Err("sidecar rejected the message".to_string());
    }
    Ok(())
}

// ── Data gathering ───────────────────────────────────────────────────────────

async fn active_branch(pool: &SqlitePool) -> Option<(String, String)> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT branch_id, currency FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

async fn owner_phone(pool: &SqlitePool) -> Option<String> {
    let jid: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'whatsapp_owner_jid'")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    phone_from_jid(&jid.unwrap_or_default())
}

/// `whatsapp_owner_jid` is stored as a normalized JID (e.g.
/// "97333050666@s.whatsapp.net"); the sidecar's /send endpoint wants a bare
/// phone number and appends the domain itself, so the JID suffix is stripped.
fn phone_from_jid(jid: &str) -> Option<String> {
    let phone = jid.split('@').next().unwrap_or("").trim();
    if phone.is_empty() {
        None
    } else {
        Some(phone.to_string())
    }
}

async fn net_sales_minor(
    pool: &SqlitePool,
    branch_id: &str,
    business_date: &str,
) -> Result<i64, String> {
    crate::db::repositories::report_repo::today_summary(pool, branch_id, business_date)
        .await
        .map(|s| s.net_total_minor)
        .map_err(|e| format!("today_summary failed: {}", e.internal_database_detail()))
}

/// Best-selling product for today by revenue. Mirrors the scope and
/// exclusion rules of `report_commands::report_top_products`, narrowed to a
/// single day and a single row — there is no existing single-day/top-1
/// helper to reuse.
async fn top_item_today(
    pool: &SqlitePool,
    branch_id: &str,
    business_date: &str,
) -> Result<Option<String>, String> {
    let (scope, origin_device_id) = crate::sync::scope::report_scope(pool).await;
    sqlx::query_scalar::<_, String>(
        "SELECT si.product_name_snapshot
         FROM sale_items si
         JOIN sales s ON s.sale_id = si.sale_id
         WHERE s.branch_id = ? AND s.business_date = ? AND s.status != 'voided' AND si.voided = 0
           AND (? = 'all' OR s.origin_device_id = ?)
         GROUP BY si.product_name_snapshot
         ORDER BY SUM(si.line_total_minor) DESC
         LIMIT 1",
    )
    .bind(branch_id)
    .bind(business_date)
    .bind(scope.as_str())
    .bind(&origin_device_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("top item query failed: {e}"))
}

/// Count of active, tracked products at or below their reorder point —
/// includes items already at zero. Same predicate as
/// `ai::proactive::rule_low_stock`, minus the `qty > 0` split that rule uses
/// to separate "out of stock" from "low stock" into two alert types; the
/// digest reports one combined "needs reordering" number.
async fn low_stock_count(pool: &SqlitePool, branch_id: &str) -> Result<i64, String> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM products p
         LEFT JOIN stock_levels sl ON sl.product_id = p.product_id AND sl.branch_id = ?
         WHERE p.is_active = 1 AND p.track_inventory = 1 AND p.deleted_at IS NULL
           AND CAST(COALESCE(sl.quantity_on_hand, '0') AS REAL) <= p.reorder_point",
    )
    .bind(branch_id)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("low stock query failed: {e}"))
}

// ── Message composition (pure, unit-tested) ──────────────────────────────────

struct DigestInputs<'a> {
    currency: &'a str,
    currency_exponent: i32,
    net_total_minor: i64,
    top_item: Option<&'a str>,
    low_stock_count: i64,
}

/// Bilingual (EN/AR) template, matching the convention every other
/// system-composed WhatsApp message in this app follows (delivery
/// confirmation, arrival notice, payment reminder) — the only non-bilingual
/// sends are free-text messages a person typed themselves.
fn build_message(input: &DigestInputs) -> String {
    let net =
        crate::domain::money::format_minor(input.net_total_minor, input.currency_exponent as u32);
    let top_item_en = input.top_item.unwrap_or("No sales yet");
    let top_item_ar = input.top_item.unwrap_or("لا توجد مبيعات بعد");
    let (stock_en, stock_ar) = if input.low_stock_count > 0 {
        let unit_en = if input.low_stock_count == 1 {
            "item"
        } else {
            "items"
        };
        (
            format!("{} {unit_en} low stock — reorder?", input.low_stock_count),
            format!("{} صنف منخفض المخزون — إعادة الطلب؟", input.low_stock_count),
        )
    } else {
        (
            "All items well stocked.".to_string(),
            "جميع الأصناف متوفرة بكمية جيدة.".to_string(),
        )
    };

    format!(
        "📊 *Evening Summary*\n\
         💰 Sales: {cur} {net}\n\
         🏆 Top item: {top_item_en}\n\
         📦 {stock_en}\n\
         ─────────────────\n\
         📊 *الملخص المسائي*\n\
         💰 المبيعات: {net} {cur}\n\
         🏆 الأكثر مبيعاً: {top_item_ar}\n\
         📦 {stock_ar}",
        cur = input.currency,
    )
}

// ── Sidecar send ──────────────────────────────────────────────────────────────
// Reuses the shared SIDECAR_URL constant + read_sidecar_token helper and the
// same POST /send JSON shape as whatsapp_commands::send_raw and
// whatsapp_catalog_commands::whatsapp_order_message — no new HTTP client.

async fn send_to_sidecar(to: &str, message: &str, token: &str) -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap_or_default();

    let resp = client
        .post(format!("{SIDECAR_URL}/send"))
        .header("X-Sidecar-Token", token)
        .json(&serde_json::json!({ "to": to, "message": message }))
        .send()
        .await
        .map_err(|e| format!("sidecar unreachable: {e}"))?;

    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    Ok(body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    fn sample_input(top_item: Option<&str>, low_stock: i64) -> DigestInputs<'_> {
        DigestInputs {
            currency: "BHD",
            currency_exponent: 3,
            net_total_minor: 125_500,
            top_item,
            low_stock_count: low_stock,
        }
    }

    #[test]
    fn message_includes_formatted_net_sales() {
        let msg = build_message(&sample_input(Some("Latte"), 3));
        assert!(msg.contains("125.500"), "BHD 3dp: 125500 minor -> 125.500");
        assert!(msg.contains("BHD"));
    }

    #[test]
    fn message_includes_top_item_and_reorder_prompt_when_low_stock() {
        let msg = build_message(&sample_input(Some("Latte"), 3));
        assert!(msg.contains("Latte"));
        assert!(msg.contains("3 items low stock — reorder?"));
    }

    #[test]
    fn message_uses_singular_item_for_count_of_one() {
        let msg = build_message(&sample_input(Some("Latte"), 1));
        assert!(msg.contains("1 item low stock — reorder?"));
        assert!(!msg.contains("1 items"));
    }

    #[test]
    fn message_omits_reorder_prompt_when_nothing_low() {
        let msg = build_message(&sample_input(Some("Latte"), 0));
        assert!(!msg.contains("reorder?"));
        assert!(msg.contains("well stocked"));
    }

    #[test]
    fn message_handles_no_sales_yet() {
        let msg = build_message(&sample_input(None, 0));
        assert!(msg.contains("No sales yet"));
    }

    #[test]
    fn message_contains_arabic_mirror() {
        let msg = build_message(&sample_input(Some("Latte"), 2));
        assert!(msg.contains("الملخص المسائي"));
        assert!(msg.contains("إعادة الطلب"));
    }

    #[test]
    fn phone_from_jid_strips_domain() {
        assert_eq!(
            phone_from_jid("97333050666@s.whatsapp.net"),
            Some("97333050666".to_string())
        );
    }

    #[test]
    fn phone_from_jid_none_when_unconfigured() {
        assert_eq!(phone_from_jid(""), None);
    }

    async fn config_test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("memory db");
        sqlx::query(
            "CREATE TABLE app_config (
                key        TEXT PRIMARY KEY,
                value      TEXT NOT NULL DEFAULT '',
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .expect("app_config table");
        pool
    }

    #[tokio::test]
    async fn claim_send_slot_is_won_exactly_once() {
        let pool = config_test_pool().await;
        assert!(claim_send_slot(&pool, "evening_digest_sent_2026-07-25").await);
        assert!(!claim_send_slot(&pool, "evening_digest_sent_2026-07-25").await);
    }

    #[tokio::test]
    async fn released_claim_can_be_reclaimed() {
        let pool = config_test_pool().await;
        assert!(claim_send_slot(&pool, "evening_digest_sent_2026-07-25").await);
        release_claim(&pool, "evening_digest_sent_2026-07-25").await;
        assert!(claim_send_slot(&pool, "evening_digest_sent_2026-07-25").await);
    }

    #[tokio::test]
    async fn finalized_claim_cannot_be_reclaimed_or_released() {
        let pool = config_test_pool().await;
        assert!(claim_send_slot(&pool, "evening_digest_sent_2026-07-25").await);
        finalize_claim(&pool, "evening_digest_sent_2026-07-25").await;
        assert!(!claim_send_slot(&pool, "evening_digest_sent_2026-07-25").await);
        // release_claim only deletes 'pending' rows — a finalized 'sent' claim
        // must survive it, otherwise a later shift close would re-send.
        release_claim(&pool, "evening_digest_sent_2026-07-25").await;
        assert!(!claim_send_slot(&pool, "evening_digest_sent_2026-07-25").await);
    }

    #[tokio::test]
    async fn low_stock_count_counts_items_at_or_below_reorder_point() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("memory db");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");
        // Note: low_stock_count() joins only products + stock_levels, so no
        // branch row is needed — branch_id below is just a filter value.
        sqlx::query(
            "INSERT OR IGNORE INTO categories (category_id, name, sort_order, is_active, created_at, updated_at, version)
             VALUES ('01JCATDIGEST00000000DRINK', 'Drinks', 1, 1, datetime('now'), datetime('now'), 1)",
        )
        .execute(&pool)
        .await
        .expect("seed category");
        sqlx::query(
            "INSERT OR IGNORE INTO products
             (product_id, category_id, name, sku, barcode, description, track_inventory, reorder_point, is_active, tax_rule_id, currency, created_at, updated_at, version)
             VALUES
             ('01JPRODDIGEST0000000LOW01', '01JCATDIGEST00000000DRINK', 'Low Stock Item', 'LOW-1', '1111111111111', NULL, 1, 10, 1, NULL, 'BHD', datetime('now'), datetime('now'), 1),
             ('01JPRODDIGEST0000000OK001', '01JCATDIGEST00000000DRINK', 'Well Stocked Item', 'OK-1', '2222222222222', NULL, 1, 5, 1, NULL, 'BHD', datetime('now'), datetime('now'), 1)",
        )
        .execute(&pool)
        .await
        .expect("seed products");
        sqlx::query(
            "INSERT OR IGNORE INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at, sync_status, sync_attempts)
             VALUES
             ('SL-DIGEST-LOW', '01JPRODDIGEST0000000LOW01', '01JBRANCH0000000000000001', '2', datetime('now'), datetime('now'), 'synced', 0),
             ('SL-DIGEST-OK',  '01JPRODDIGEST0000000OK001', '01JBRANCH0000000000000001', '50', datetime('now'), datetime('now'), 'synced', 0)",
        )
        .execute(&pool)
        .await
        .expect("seed stock levels");

        let count = low_stock_count(&pool, "01JBRANCH0000000000000001")
            .await
            .expect("low stock query");
        assert_eq!(
            count, 1,
            "only the item at qty 2 <= reorder_point 10 counts"
        );
    }
}
