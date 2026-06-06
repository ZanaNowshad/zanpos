use crate::commands::rbac;
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::Serialize;
use tauri::State;
use ulid::Ulid;
use sqlx::FromRow;

// ── Output types ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, FromRow)]
pub struct GhostBarcode {
    pub id: String,
    pub barcode: String,
    pub scan_count: i64,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub status: String,
    pub product_name: Option<String>,
    pub brand: Option<String>,
    pub category: Option<String>,
    pub image_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GhostSummary {
    pub pending: i64,
    pub found: i64,
    pub not_found: i64,
    pub dismissed: i64,  // M-19: previously omitted from summary counts
}

#[derive(Debug, Serialize)]
pub struct ProductPrefill {
    pub name: String,
    pub barcode: String,
    pub brand: Option<String>,
    pub category: Option<String>,
    pub image_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResolveResult {
    pub resolved: i64,
    pub not_found: i64,
}

// ── Helper: current time as Unix milliseconds ─────────────────────────────────

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Called by the POS frontend whenever a barcode scan fails.
/// Fire-and-forget from the frontend: returns Ok(()) always.
#[tauri::command]
pub async fn ghost_record(
    barcode: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    if barcode.trim().is_empty() {
        return Ok(());
    }
    let id = Ulid::new().to_string();
    let now = now_ms();
    // The WHERE status = 'pending' guard is intentional: once a barcode is
    // resolved ('found'/'not_found') or dismissed, re-scanning it should NOT
    // reset its resolved data or increment a stale count. Silently no-op.
    sqlx::query(
        "INSERT INTO unknown_barcodes (id, barcode, scan_count, first_seen_at, last_seen_at)
         VALUES (?, ?, 1, ?, ?)
         ON CONFLICT(barcode) DO UPDATE
           SET scan_count   = scan_count + 1,
               last_seen_at = excluded.last_seen_at
         WHERE status = 'pending'",
    )
    .bind(&id)
    .bind(&barcode)
    .bind(now)
    .bind(now)
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Returns counts of pending/found/not_found barcodes.
/// Used by BackOffice nav badge. Manager/owner only.
#[tauri::command]
pub async fn ghost_summary(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<GhostSummary> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    // M-19: include dismissed in the GROUP BY so the count is complete
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, COUNT(*) as cnt FROM unknown_barcodes GROUP BY status",
    )
    .fetch_all(&state.db)
    .await?;

    let mut summary = GhostSummary { pending: 0, found: 0, not_found: 0, dismissed: 0 };
    for (status, cnt) in rows {
        match status.as_str() {
            "pending"   => summary.pending   = cnt,
            "found"     => summary.found     = cnt,
            "not_found" => summary.not_found = cnt,
            "dismissed" => summary.dismissed = cnt,
            _ => {}
        }
    }
    Ok(summary)
}

/// Full list of non-dismissed barcodes, ordered by scan_count DESC.
/// Manager/owner only.
#[tauri::command]
pub async fn ghost_list(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<GhostBarcode>> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let rows: Vec<GhostBarcode> = sqlx::query_as(
        "SELECT id, barcode, scan_count, first_seen_at, last_seen_at,
                status, product_name, brand, category, image_url
         FROM unknown_barcodes
         WHERE status != 'dismissed'
         ORDER BY scan_count DESC, last_seen_at DESC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(rows)
}

/// Set a ghost barcode to 'dismissed' — removes it from the panel.
/// Manager/owner only.
#[tauri::command]
pub async fn ghost_dismiss(
    id: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    // Intentionally idempotent: if the row was already dismissed (or never
    // existed), this is a no-op rather than an error.  The manager panel will
    // have removed the card client-side already, so a 404 here is noise.
    sqlx::query("UPDATE unknown_barcodes SET status = 'dismissed' WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;
    Ok(())
}

/// Returns product data pre-filled from a 'found' ghost barcode row.
/// Used by "Create Product" button to pre-fill the product form.
/// Manager/owner only.
#[tauri::command]
pub async fn ghost_prefill(
    id: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<ProductPrefill> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let row: Option<(String, String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT product_name, barcode, brand, category, image_url
             FROM unknown_barcodes
             WHERE id = ? AND status = 'found'",
        )
        .bind(&id)
        .fetch_optional(&state.db)
        .await?;

    let (name, barcode, brand, category, image_url) =
        row.ok_or_else(|| AppError::NotFound("Ghost barcode not found or not resolved".into()))?;

    Ok(ProductPrefill { name, barcode, brand, category, image_url })
}

// ── HTTP lookup helpers ───────────────────────────────────────────────────────

/// Try UPCitemdb free tier.
/// Returns Some((name, brand, category, image_url, raw_json)) on hit.
async fn lookup_upcitemdb(
    client: &reqwest::Client,
    barcode: &str,
) -> Option<(String, Option<String>, Option<String>, Option<String>, String)> {
    let url = format!(
        "https://api.upcitemdb.com/prod/trial/lookup?upc={}",
        barcode
    );
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let text = resp.text().await.ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;

    let item = json.get("items")?.get(0)?;
    let name = item.get("title")?.as_str()?.to_string();
    if name.is_empty() {
        return None;
    }
    let brand     = item.get("brand").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(str::to_string);
    let category  = item.get("category").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(str::to_string);
    let image_url = item
        .get("images")
        .and_then(|v| v.get(0))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    Some((name, brand, category, image_url, text))
}

/// Try Open Food Facts.
/// Returns Some((name, brand, category, image_url, raw_json)) on hit.
async fn lookup_off(
    client: &reqwest::Client,
    barcode: &str,
) -> Option<(String, Option<String>, Option<String>, Option<String>, String)> {
    let url = format!(
        "https://world.openfoodfacts.org/api/v0/product/{}.json",
        barcode
    );
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .header("User-Agent", "ZANPOS/1.0 (contact@zanpos.app)")
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let text = resp.text().await.ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;

    if json.get("status")?.as_i64()? != 1 {
        return None;
    }
    let product = json.get("product")?;
    let name = product
        .get("product_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if name.is_empty() {
        return None;
    }
    let brand = product.get("brands").and_then(|v| v.as_str()).map(|s| {
        s.split(',').next().unwrap_or(s).trim().to_string()
    }).filter(|s| !s.is_empty());
    let category = product
        .get("categories_tags")
        .and_then(|v| v.get(0))
        .and_then(|v| v.as_str())
        .map(|s| s.trim_start_matches("en:").replace('-', " "))
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        });
    let image_url = product
        .get("image_url")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    Some((name, brand, category, image_url, text))
}

/// Try AI fallback (one-shot Anthropic or OpenAI call).
/// Returns Some((name, brand, category)) on success.
async fn lookup_ai(
    client: &reqwest::Client,
    barcode: &str,
    pool: &sqlx::SqlitePool,
) -> Option<(String, Option<String>, Option<String>)> {
    let provider = crate::db::repositories::ai_admin_repo::get_config(pool, "ai_provider")
        .await
        .ok()
        .flatten();

    let prompt = format!(
        "The barcode {} was scanned at a retail POS but was not found in the product database. \
         Based on this barcode number, identify the product if you can. \
         Reply with ONLY valid JSON in this exact format: \
         {{\"product_name\": \"\", \"brand\": \"\", \"category\": \"\"}} \
         If you cannot identify the product, reply with: \
         {{\"product_name\": null, \"brand\": null, \"category\": null}}",
        barcode
    );

    match provider.as_deref() {
        Some("anthropic") | None => {
            // Prefer OS credential store; fall back to legacy plaintext SQLite key.
            let key = {
                let from_os = crate::secure_store::get_secret("anthropic_api_key").unwrap_or_default();
                if !from_os.is_empty() {
                    from_os
                } else {
                    crate::db::repositories::ai_admin_repo::get_config(pool, "anthropic_api_key")
                        .await.ok().flatten().unwrap_or_default()
                }
            };
            if key.is_empty() {
                return None;
            }
            let body = serde_json::json!({
                "model": "claude-haiku-4-5",
                "max_tokens": 128,
                "messages": [{ "role": "user", "content": prompt }]
            });
            let resp = client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .ok()?;
            let text = resp.text().await.ok()?;
            let json: serde_json::Value = serde_json::from_str(&text).ok()?;
            let content = json
                .get("content")?
                .get(0)?
                .get("text")?
                .as_str()?;
            parse_ai_json(content)
        }
        Some("openai") => {
            // Prefer OS credential store; fall back to legacy plaintext SQLite key.
            let key = {
                let from_os = crate::secure_store::get_secret("openai_api_key").unwrap_or_default();
                if !from_os.is_empty() {
                    from_os
                } else {
                    crate::db::repositories::ai_admin_repo::get_config(pool, "openai_api_key")
                        .await.ok().flatten().unwrap_or_default()
                }
            };
            if key.is_empty() {
                return None;
            }
            let base_url = crate::db::repositories::ai_admin_repo::get_config(pool, "openai_base_url")
                .await.ok().flatten().unwrap_or_else(|| "https://api.openai.com/v1".into());
            let model = crate::db::repositories::ai_admin_repo::get_config(pool, "openai_model")
                .await.ok().flatten().unwrap_or_else(|| "gpt-4o-mini".into());

            let body = serde_json::json!({
                "model": model,
                "max_tokens": 128,
                "messages": [{ "role": "user", "content": prompt }]
            });
            let resp = client
                .post(format!("{}/chat/completions", base_url))
                .bearer_auth(&key)
                .json(&body)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .ok()?;
            let text = resp.text().await.ok()?;
            let json: serde_json::Value = serde_json::from_str(&text).ok()?;
            let content = json
                .get("choices")?
                .get(0)?
                .get("message")?
                .get("content")?
                .as_str()?;
            parse_ai_json(content)
        }
        _ => None,
    }
}

/// Parse the JSON blob returned by the AI into (name, brand, category).
/// Returns None if product_name is null or missing.
fn parse_ai_json(text: &str) -> Option<(String, Option<String>, Option<String>)> {
    let start = text.find('{')?;
    let end   = text.rfind('}')?;
    let json: serde_json::Value = serde_json::from_str(&text[start..=end]).ok()?;
    let name = json.get("product_name")?.as_str()?.to_string();
    if name.is_empty() {
        return None;
    }
    // Filter out empty strings and literal "null" strings (some models reply with the word "null")
    let brand    = json.get("brand").and_then(|v| v.as_str()).filter(|s| !s.is_empty() && *s != "null").map(str::to_string);
    let category = json.get("category").and_then(|v| v.as_str()).filter(|s| !s.is_empty() && *s != "null").map(str::to_string);
    Some((name, brand, category))
}

// ── Resolve command ───────────────────────────────────────────────────────────

/// Runs the HTTP lookup chain for all 'pending' barcodes.
/// Tier 1: UPCitemdb → Tier 2: Open Food Facts → Tier 3: AI fallback.
/// Stops at the first tier that returns a result for each barcode.
/// Updates rows in place and returns a summary.
/// Manager/owner only.
#[tauri::command]
pub async fn ghost_resolve(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<ResolveResult> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    let pending: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, barcode FROM unknown_barcodes WHERE status = 'pending'",
    )
    .fetch_all(&state.db)
    .await?;

    if pending.is_empty() {
        return Ok(ResolveResult { resolved: 0, not_found: 0 });
    }

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client build failed: {e}")))?;

    let mut resolved  = 0i64;
    let mut not_found = 0i64;

    for (id, barcode) in &pending {
        // Tier 1: UPCitemdb
        if let Some((name, brand, category, image_url, raw_json)) =
            lookup_upcitemdb(&client, barcode).await
        {
            sqlx::query(
                "UPDATE unknown_barcodes
                 SET status='found', product_name=?, brand=?, category=?, image_url=?, raw_json=?
                 WHERE id=?",
            )
            .bind(&name).bind(&brand).bind(&category).bind(&image_url).bind(&raw_json).bind(id)
            .execute(&state.db)
            .await?;
            resolved += 1;
            continue;
        }

        // Tier 2: Open Food Facts
        if let Some((name, brand, category, image_url, raw_json)) =
            lookup_off(&client, barcode).await
        {
            sqlx::query(
                "UPDATE unknown_barcodes
                 SET status='found', product_name=?, brand=?, category=?, image_url=?, raw_json=?
                 WHERE id=?",
            )
            .bind(&name).bind(&brand).bind(&category).bind(&image_url).bind(&raw_json).bind(id)
            .execute(&state.db)
            .await?;
            resolved += 1;
            continue;
        }

        // Tier 3: AI fallback
        if let Some((name, brand, category)) = lookup_ai(&client, barcode, &state.db).await {
            let raw_json = serde_json::json!({
                "source": "ai_fallback",
                "product_name": name,
                "brand": brand,
                "category": category
            })
            .to_string();
            sqlx::query(
                "UPDATE unknown_barcodes
                 SET status='found', product_name=?, brand=?, category=?, raw_json=?
                 WHERE id=?",
            )
            .bind(&name).bind(&brand).bind(&category).bind(&raw_json).bind(id)
            .execute(&state.db)
            .await?;
            resolved += 1;
            continue;
        }

        // All tiers exhausted
        sqlx::query("UPDATE unknown_barcodes SET status='not_found' WHERE id=?")
            .bind(id)
            .execute(&state.db)
            .await?;
        not_found += 1;
    }

    Ok(ResolveResult { resolved, not_found })
}
