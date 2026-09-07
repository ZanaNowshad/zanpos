//! WhatsApp → POS notification inbox.
//!
//! Lets an admin pick the business owner's WhatsApp contact and the store group
//! (Settings → WhatsApp), then surfaces every incoming message from those two
//! chats as a POS notification. Messages are buffered by the Baileys sidecar and
//! pulled here via a cursor, filtered to the configured JIDs, and stored in
//! `wa_messages` for the notification popup. Manager/owner only.

use crate::commands::rbac;
use crate::commands::whatsapp_commands::{read_sidecar_token, SIDECAR_URL};
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::time::Duration;
use tauri::State;

// ── Types ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct WaContact {
    pub id: String,
    /// What to show. Prefers the shop's own name for this person.
    pub name: String,
    /// The address-book name — what this shop saved them as.
    ///
    /// Carried separately from `name` so a customer can be found by either the
    /// name the shop gave them or the one they gave themselves. Collapsing the
    /// two meant a cashier had to know the customer's WhatsApp profile name to
    /// find them, which is not the name they think of at the counter.
    #[serde(default, rename = "savedName")]
    pub saved_name: Option<String>,
    /// The pushName — what the person set on their own WhatsApp profile.
    #[serde(default, rename = "pushName")]
    pub push_name: Option<String>,
    /// A WhatsApp Business verified name, when there is one.
    #[serde(default, rename = "verifiedName")]
    pub verified_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WaGroup {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct WaTargets {
    pub owner_jid: String,
    pub owner_name: String,
    pub group_jid: String,
    pub group_name: String,
}

#[derive(Debug, Serialize)]
pub struct WaMessage {
    pub id: String,
    pub chat_jid: String,
    pub chat_name: Option<String>,
    pub is_group: bool,
    pub sender_jid: Option<String>,
    pub sender_name: Option<String>,
    pub body: String,
    pub ts: i64,
    pub read: bool,
    pub media_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WaMedia {
    pub ok: bool,
    pub base64: Option<String>,
    pub mimetype: Option<String>,
}

/// One message as served by the sidecar GET /messages endpoint.
#[derive(Debug, Deserialize)]
struct SidecarMessage {
    id: String,
    #[serde(rename = "chatJid")]
    chat_jid: String,
    #[serde(rename = "isGroup")]
    is_group: bool,
    #[serde(rename = "senderJid")]
    sender_jid: String,
    #[serde(rename = "senderName")]
    sender_name: String,
    text: String,
    ts: i64,
    #[serde(rename = "mediaType")]
    media_type: Option<String>,
    #[serde(rename = "orderId", default)]
    order_id: Option<String>,
    #[serde(rename = "orderToken", default)]
    order_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    messages: Vec<SidecarMessage>,
    cursor: i64,
}

/// Sidecar GET /media response.
#[derive(Debug, Deserialize)]
struct SidecarMedia {
    ok: bool,
    base64: Option<String>,
    mimetype: Option<String>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────────

fn sidecar_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_default()
}

async fn get_cfg(db: &SqlitePool, key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM app_config WHERE key = ?")
        .bind(key)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

async fn set_cfg(db: &SqlitePool, key: &str, value: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("INSERT OR REPLACE INTO app_config (key, value, updated_at) VALUES (?, ?, ?)")
        .bind(key)
        .bind(value)
        .bind(&now)
        .execute(db)
        .await?;
    Ok(())
}

fn normalize_jid(raw: &str) -> String {
    let value = raw.trim().to_ascii_lowercase();
    let Some((local, domain)) = value.split_once('@') else {
        return value;
    };

    let domain = if domain == "c.us" {
        "s.whatsapp.net"
    } else {
        domain
    };
    let local = if domain == "s.whatsapp.net" {
        local.split(':').next().unwrap_or(local)
    } else {
        local
    };

    format!("{local}@{domain}")
}

fn target_matches(configured: &str, actual: &str) -> bool {
    let configured = normalize_jid(configured);
    !configured.is_empty() && configured == normalize_jid(actual)
}

async fn unread_count(db: &SqlitePool) -> AppResult<i64> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM wa_messages WHERE read = 0")
            .fetch_one(db)
            .await
            .unwrap_or(0),
    )
}

// ── Pickers: contacts + groups (Settings) ────────────────────────────────────────

/// Read-only list of WhatsApp contacts for the owner picker. (Distinct from
/// `whatsapp_import_contacts`, which writes them into the customers table.)
#[tauri::command]
pub async fn whatsapp_list_contacts(
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<WaContact>> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;
    let token = read_sidecar_token(&state);
    let resp = sidecar_client()
        .get(format!("{}/contacts", SIDECAR_URL))
        .header("X-Sidecar-Token", &token)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("WhatsApp service unreachable: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal("Failed to fetch contacts".into()));
    }
    resp.json()
        .await
        .map_err(|e| AppError::Internal(format!("Bad contacts response: {e}")))
}

/// Groups the connected account participates in, for the store-group picker.
#[tauri::command]
pub async fn whatsapp_list_groups(
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<WaGroup>> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;
    let token = read_sidecar_token(&state);
    let resp = sidecar_client()
        .get(format!("{}/groups", SIDECAR_URL))
        .header("X-Sidecar-Token", &token)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("WhatsApp service unreachable: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal("Failed to fetch groups".into()));
    }
    resp.json()
        .await
        .map_err(|e| AppError::Internal(format!("Bad groups response: {e}")))
}

/// Save the chosen owner contact + store group. Empty strings clear a target.
#[tauri::command]
pub async fn whatsapp_set_targets(
    owner_jid: String,
    owner_name: String,
    group_jid: String,
    group_name: String,
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;
    set_cfg(&state.db, "whatsapp_owner_jid", &normalize_jid(&owner_jid)).await?;
    set_cfg(&state.db, "whatsapp_owner_name", owner_name.trim()).await?;
    set_cfg(&state.db, "whatsapp_group_jid", &normalize_jid(&group_jid)).await?;
    set_cfg(&state.db, "whatsapp_group_name", group_name.trim()).await?;
    Ok(())
}

/// Current owner/group selection, to pre-fill the Settings pickers.
#[tauri::command]
pub async fn whatsapp_get_targets(
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<WaTargets> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;
    Ok(WaTargets {
        owner_jid: get_cfg(&state.db, "whatsapp_owner_jid")
            .await
            .unwrap_or_default(),
        owner_name: get_cfg(&state.db, "whatsapp_owner_name")
            .await
            .unwrap_or_default(),
        group_jid: get_cfg(&state.db, "whatsapp_group_jid")
            .await
            .unwrap_or_default(),
        group_name: get_cfg(&state.db, "whatsapp_group_name")
            .await
            .unwrap_or_default(),
    })
}

// ── Poll + read (POS notification centre) ────────────────────────────────────────

/// Pull new messages from the sidecar, store those from the configured owner/group,
/// and return the current unread count for the POS bell badge. Called on a short
/// interval by the POS. Sidecar/network failures are non-fatal (returns the count).
#[tauri::command]
pub async fn whatsapp_poll_messages(
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<i64> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;

    let owner_jid = get_cfg(&state.db, "whatsapp_owner_jid")
        .await
        .unwrap_or_default();
    let group_jid = get_cfg(&state.db, "whatsapp_group_jid")
        .await
        .unwrap_or_default();
    // Customers awaiting payment-screenshot verification are arbitrary JIDs (not the
    // owner/group), so the poll must also run when only confirmations are pending.
    let pending_pay_jids: std::collections::HashSet<String> =
        crate::commands::payment_confirm_commands::pending_jids(&state.db)
            .await
            .into_iter()
            .collect();
    // Public storefront checkout blocks may arrive from any customer. Keep the
    // sidecar poll active while storefront ingestion is enabled, even when the
    // notification owner/group targets have not been configured.
    let storefront_enabled = matches!(
        get_cfg(&state.db, "storefront_enabled").await.as_deref(),
        Some("1" | "true")
    );
    if owner_jid.is_empty()
        && group_jid.is_empty()
        && pending_pay_jids.is_empty()
        && !storefront_enabled
    {
        return unread_count(&state.db).await; // nothing targeted yet
    }
    let owner_name = get_cfg(&state.db, "whatsapp_owner_name")
        .await
        .unwrap_or_default();
    let group_name = get_cfg(&state.db, "whatsapp_group_name")
        .await
        .unwrap_or_default();

    let cursor: i64 = get_cfg(&state.db, "whatsapp_msg_cursor")
        .await
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let token = read_sidecar_token(&state);
    let resp = match sidecar_client()
        .get(format!("{}/messages?after={}", SIDECAR_URL, cursor))
        .header("X-Sidecar-Token", &token)
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r,
        _ => return unread_count(&state.db).await, // sidecar down — try again next poll
    };
    let payload: MessagesResponse = match resp.json().await {
        Ok(p) => p,
        Err(_) => return unread_count(&state.db).await,
    };

    // Sidecar restart detection: its seq is in-memory and resets to 0 on restart.
    // If the server's cursor went backwards, our stored cursor is stale and would
    // skip the new messages — rewind to 0 so the next poll re-reads the buffer
    // (the message-id PK makes re-inserting already-stored messages a no-op).
    if payload.cursor < cursor {
        set_cfg(&state.db, "whatsapp_msg_cursor", "0").await?;
        return unread_count(&state.db).await;
    }

    let now = chrono::Utc::now().to_rfc3339();
    for m in &payload.messages {
        // Payment-screenshot auto-verification: a customer with an open pending
        // confirmation replied with an image. Runs regardless of owner/group filter.
        if m.media_type.as_deref() == Some("image") && pending_pay_jids.contains(&m.chat_jid) {
            crate::commands::payment_confirm_commands::run_verification(
                state.inner(),
                &m.chat_jid,
                &m.id,
                Some(m.sender_name.as_str()),
            )
            .await;
        }
        // WhatsApp commerce order: resolve line items via the sidecar and store
        // in wa_orders. Runs regardless of owner/group filter — an order can come
        // from any customer. Best-effort; failures are logged and skipped.
        if let (Some(oid), Some(otok)) = (m.order_id.as_deref(), m.order_token.as_deref()) {
            if !oid.is_empty() && !otok.is_empty() {
                crate::commands::whatsapp_catalog_commands::resolve_and_store_order(
                    state.inner(),
                    &token,
                    oid,
                    otok,
                    &m.id,
                    &m.chat_jid,
                    &m.sender_name,
                )
                .await;
            }
        }
        // Public storefront order marker: unlike inbox notifications, this is
        // intentionally accepted from arbitrary customer JIDs. Rust parses a
        // bounded block and re-fetches every product/current price before insert.
        // Native WhatsApp orderMessage handling above remains unchanged.
        if storefront_enabled && m.text.contains("ZANPOS:v1") {
            if let Err(error) = crate::storefront::orders::ingest_storefront_message(
                &state.db,
                &m.text,
                &m.id,
                &m.chat_jid,
                &m.sender_name,
            )
            .await
            {
                tracing::warn!(
                    "Rejected storefront order marker in WhatsApp message {}: {}",
                    m.id,
                    error
                );
            }
        }
        let is_owner = target_matches(&owner_jid, &m.chat_jid);
        let is_group = target_matches(&group_jid, &m.chat_jid);
        if !is_owner && !is_group {
            continue;
        }
        let chat_name = if is_owner { &owner_name } else { &group_name };
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO wa_messages \
             (id, chat_jid, chat_name, is_group, sender_jid, sender_name, body, ts, read, media_type, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(&m.id)
        .bind(&m.chat_jid)
        .bind(chat_name)
        .bind(m.is_group as i64)
        .bind(&m.sender_jid)
        .bind(&m.sender_name)
        .bind(&m.text)
        .bind(m.ts)
        .bind(&m.media_type)
        .bind(&now)
        .execute(&state.db)
        .await;
    }

    // Advance the cursor past everything served, even unmatched messages.
    set_cfg(
        &state.db,
        "whatsapp_msg_cursor",
        &payload.cursor.to_string(),
    )
    .await?;

    unread_count(&state.db).await
}

/// Recent stored messages for the notification popup (unread first, newest first).
#[tauri::command]
pub async fn whatsapp_list_messages(
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<WaMessage>> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;
    let rows = sqlx::query(
        "SELECT id, chat_jid, chat_name, is_group, sender_jid, sender_name, body, ts, read, media_type \
         FROM wa_messages ORDER BY read ASC, ts DESC LIMIT 100",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            let is_group: i64 = r.get("is_group");
            let read: i64 = r.get("read");
            WaMessage {
                id: r.get("id"),
                chat_jid: r.get("chat_jid"),
                chat_name: r.get("chat_name"),
                is_group: is_group != 0,
                sender_jid: r.get("sender_jid"),
                sender_name: r.get("sender_name"),
                body: r.get("body"),
                ts: r.get("ts"),
                read: read != 0,
                media_type: r.get("media_type"),
            }
        })
        .collect())
}

/// Download + return the decrypted image for a stored message (View action).
#[tauri::command]
pub async fn whatsapp_get_media(
    message_id: String,
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<WaMedia> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;
    let token = read_sidecar_token(&state);
    let resp = sidecar_client()
        .get(format!("{}/media", SIDECAR_URL))
        .query(&[("id", message_id.as_str())])
        .header("X-Sidecar-Token", &token)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("WhatsApp service unreachable: {e}")))?;
    if !resp.status().is_success() {
        return Ok(WaMedia {
            ok: false,
            base64: None,
            mimetype: None,
        });
    }
    let m: SidecarMedia = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Bad media response: {e}")))?;
    Ok(WaMedia {
        ok: m.ok,
        base64: m.base64,
        mimetype: m.mimetype,
    })
}

/// Mark a single message read.
#[tauri::command]
pub async fn whatsapp_mark_read(
    id: String,
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;
    sqlx::query("UPDATE wa_messages SET read = 1 WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;
    Ok(())
}

/// Mark every unread message read (popup close / "mark all read").
#[tauri::command]
pub async fn whatsapp_mark_all_read(
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;
    sqlx::query("UPDATE wa_messages SET read = 1 WHERE read = 0")
        .execute(&state.db)
        .await?;
    Ok(())
}

/// Clear all stored WhatsApp notifications ("Clear all" in the panel). The poll
/// cursor is left as-is, so already-consumed messages don't reappear; new ones
/// still arrive normally.
#[tauri::command]
pub async fn whatsapp_clear_messages(
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;
    sqlx::query("DELETE FROM wa_messages")
        .execute(&state.db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_matching_tolerates_whatsapp_jid_variants() {
        assert!(target_matches(
            " 97333050666@s.whatsapp.net ",
            "97333050666:12@S.WHATSAPP.NET"
        ));
        assert!(target_matches(
            "97333050666@c.us",
            "97333050666@s.whatsapp.net"
        ));
        assert!(target_matches(
            "120363040000000000@g.us",
            " 120363040000000000@G.US "
        ));
        assert!(!target_matches(
            "120363040000000000@g.us",
            "120363050000000000@g.us"
        ));
    }
}
