use crate::commands::rbac;
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};
use tauri::State;
use ulid::Ulid;

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
    pub dismissed: i64, // M-19: previously omitted from summary counts
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
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE).await?;
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
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<GhostSummary> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    // M-19: include dismissed in the GROUP BY so the count is complete
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT status, COUNT(*) as cnt FROM unknown_barcodes GROUP BY status")
            .fetch_all(&state.db)
            .await?;

    let mut summary = GhostSummary {
        pending: 0,
        found: 0,
        not_found: 0,
        dismissed: 0,
    };
    for (status, cnt) in rows {
        match status.as_str() {
            "pending" => summary.pending = cnt,
            "found" => summary.found = cnt,
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
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<GhostBarcode>> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    list_ghosts(&state.db).await
}

/// Underlying query for [`ghost_list`], split out so it can be unit-tested
/// without a Tauri `State`.
///
/// first_seen_at/last_seen_at are declared TEXT in the schema but `ghost_record`
/// writes Unix-millis i64 into them (stored as text via TEXT affinity). CAST to
/// INTEGER so sqlx can decode them into the i64 struct fields regardless of how a
/// given row was stored — without the cast, `query_as` fails to decode once any
/// real row exists.
async fn list_ghosts(pool: &SqlitePool) -> AppResult<Vec<GhostBarcode>> {
    let rows: Vec<GhostBarcode> = sqlx::query_as(
        "SELECT id, barcode, scan_count,
                CAST(first_seen_at AS INTEGER) AS first_seen_at,
                CAST(last_seen_at  AS INTEGER) AS last_seen_at,
                status, product_name, brand, category, image_url
         FROM unknown_barcodes
         WHERE status != 'dismissed'
         ORDER BY scan_count DESC, last_seen_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Set a ghost barcode to 'dismissed' — removes it from the panel.
/// Manager/owner only.
#[tauri::command]
pub async fn ghost_dismiss(
    id: String,
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
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
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<ProductPrefill> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    let row: Option<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT product_name, barcode, brand, category, image_url
             FROM unknown_barcodes
             WHERE id = ? AND status = 'found'",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?;

    let (name, barcode, brand, category, image_url) =
        row.ok_or_else(|| AppError::NotFound("Ghost barcode not found or not resolved".into()))?;

    Ok(ProductPrefill {
        name,
        barcode,
        brand,
        category,
        image_url,
    })
}

// ── HTTP lookup providers ─────────────────────────────────────────────────────
//
// Each provider takes a barcode and returns Some(LookupHit) on a confident match.
// `lookup_chain` runs the free/unlimited community databases first (concurrently),
// then optional key-gated commercial databases, then the AI fallback. To add a new
// source: write one `lookup_*` function returning Option<LookupHit> and slot it
// into `lookup_chain`.

const LOOKUP_UA: &str = "ZANPOS/1.0 (contact@zanpos.app)";

struct LookupHit {
    name: String,
    brand: Option<String>,
    category: Option<String>,
    image_url: Option<String>,
    raw_json: String,
}

/// Trimmed, non-empty string field from a JSON object.
fn str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Upper-case the first character of a string (for tag-derived categories).
fn capitalize(s: String) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

/// Read a non-empty API key from app_config; None when unset (provider skipped).
async fn config_key(pool: &SqlitePool, key: &str) -> Option<String> {
    crate::db::repositories::ai_admin_repo::get_config(pool, key)
        .await
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
}

/// UPCitemdb free trial — broad retail coverage, ~100 lookups/day, US-centric.
async fn lookup_upcitemdb(client: &reqwest::Client, barcode: &str) -> Option<LookupHit> {
    let url = format!("https://api.upcitemdb.com/prod/trial/lookup?upc={barcode}");
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
    let name = item.get("title")?.as_str()?.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let image_url = item
        .get("images")
        .and_then(|v| v.get(0))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Some(LookupHit {
        name,
        brand: str_field(item, "brand"),
        category: str_field(item, "category"),
        image_url,
        raw_json: text,
    })
}

/// Open*Facts family — Open Food / Beauty / Products / Pet Food Facts. Free,
/// unlimited, community-maintained; all four share the same v0 API schema and
/// differ only by host, so one function serves every host.
async fn lookup_openfacts(
    client: &reqwest::Client,
    barcode: &str,
    host: &str,
) -> Option<LookupHit> {
    let url = format!("https://{host}/api/v0/product/{barcode}.json");
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .header("User-Agent", LOOKUP_UA)
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
        .trim()
        .to_string();
    if name.is_empty() {
        return None;
    }
    let brand = product
        .get("brands")
        .and_then(|v| v.as_str())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .filter(|s| !s.is_empty());
    let category = product
        .get("categories_tags")
        .and_then(|v| v.get(0))
        .and_then(|v| v.as_str())
        .map(|s| s.trim_start_matches("en:").replace('-', " "))
        .map(capitalize);
    let image_url = product
        .get("image_url")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Some(LookupHit {
        name,
        brand,
        category,
        image_url,
        raw_json: text,
    })
}

/// Datakick — open, community product database (general merchandise). Free, no key.
async fn lookup_datakick(client: &reqwest::Client, barcode: &str) -> Option<LookupHit> {
    let url = format!("https://www.datakick.org/api/items/{barcode}");
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
    let name = json
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        return None;
    }
    let image_url = json
        .get("images")
        .and_then(|v| v.get(0))
        .and_then(|i| i.get("url"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Some(LookupHit {
        name,
        brand: str_field(&json, "brand_name"),
        category: None,
        image_url,
        raw_json: text,
    })
}

/// Brocade.io — open barcode database (community). Free, no key. Best-effort.
async fn lookup_brocade(client: &reqwest::Client, barcode: &str) -> Option<LookupHit> {
    let url = format!("https://www.brocade.io/api/items/{barcode}");
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(4))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let text = resp.text().await.ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let name = json
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        return None;
    }
    Some(LookupHit {
        name,
        brand: str_field(&json, "brand_name"),
        category: None,
        image_url: None,
        raw_json: text,
    })
}

/// Barcode Lookup (barcodelookup.com) — large commercial DB. Needs an API key in
/// app_config['barcodelookup_api_key']; skipped when unset.
async fn lookup_barcodelookup(
    client: &reqwest::Client,
    barcode: &str,
    pool: &SqlitePool,
) -> Option<LookupHit> {
    let key = config_key(pool, "barcodelookup_api_key").await?;
    let url = format!(
        "https://api.barcodelookup.com/v3/products?barcode={barcode}&formatted=y&key={key}"
    );
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let text = resp.text().await.ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let item = json.get("products")?.get(0)?;
    let name = item.get("title")?.as_str()?.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let image_url = item
        .get("images")
        .and_then(|v| v.get(0))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Some(LookupHit {
        name,
        brand: str_field(item, "brand"),
        category: str_field(item, "category"),
        image_url,
        raw_json: text,
    })
}

/// UPCDatabase.org — commercial DB. Needs app_config['upcdatabase_api_key'].
async fn lookup_upcdatabase(
    client: &reqwest::Client,
    barcode: &str,
    pool: &SqlitePool,
) -> Option<LookupHit> {
    let key = config_key(pool, "upcdatabase_api_key").await?;
    let url = format!("https://api.upcdatabase.org/product/{barcode}?apikey={key}");
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let text = resp.text().await.ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    if !json
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return None;
    }
    let name = json.get("title")?.as_str()?.trim().to_string();
    if name.is_empty() {
        return None;
    }
    Some(LookupHit {
        name,
        brand: str_field(&json, "brand"),
        category: str_field(&json, "category"),
        image_url: None,
        raw_json: text,
    })
}

/// Go-UPC — commercial DB. Needs app_config['goupc_api_key'] (Bearer token).
async fn lookup_goupc(
    client: &reqwest::Client,
    barcode: &str,
    pool: &SqlitePool,
) -> Option<LookupHit> {
    let key = config_key(pool, "goupc_api_key").await?;
    let url = format!("https://go-upc.com/api/v1/code/{barcode}");
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(6))
        .bearer_auth(&key)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let text = resp.text().await.ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let product = json.get("product")?;
    let name = product.get("name")?.as_str()?.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let image_url = product
        .get("imageUrl")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Some(LookupHit {
        name,
        brand: str_field(product, "brand"),
        category: str_field(product, "category"),
        image_url,
        raw_json: text,
    })
}

/// EAN-Search.org — 1.2B+ EAN database with strong international (EAN-13) and
/// issuing-country coverage, so it catches Gulf/EU products that US-only sources
/// miss. Needs app_config['eansearch_api_token']; skipped when unset.
async fn lookup_eansearch(
    client: &reqwest::Client,
    barcode: &str,
    pool: &SqlitePool,
) -> Option<LookupHit> {
    let token = config_key(pool, "eansearch_api_token").await?;
    let url = format!(
        "https://api.ean-search.org/api?token={token}&op=barcode-lookup&format=json&ean={barcode}"
    );
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let text = resp.text().await.ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    // Response is a JSON array: [{ "ean", "name", "categoryName", "issuingCountry" }].
    let item = json.get(0)?;
    let name = item
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        return None;
    }
    Some(LookupHit {
        name,
        brand: None, // EAN-Search folds the brand into the product name
        category: str_field(item, "categoryName"),
        image_url: None,
        raw_json: text,
    })
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
            let key = crate::secure_store::get_secret("anthropic_api_key").unwrap_or_default();
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
            let content = json.get("content")?.get(0)?.get("text")?.as_str()?;
            parse_ai_json(content)
        }
        Some("openai") => {
            let key = crate::secure_store::get_secret("openai_api_key").unwrap_or_default();
            if key.is_empty() {
                return None;
            }
            let base_url =
                crate::db::repositories::ai_admin_repo::get_config(pool, "openai_base_url")
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "https://api.openai.com/v1".into());
            let model = crate::db::repositories::ai_admin_repo::get_config(pool, "openai_model")
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "gpt-4o-mini".into());

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
    let end = text.rfind('}')?;
    let json: serde_json::Value = serde_json::from_str(&text[start..=end]).ok()?;
    let name = json.get("product_name")?.as_str()?.to_string();
    if name.is_empty() {
        return None;
    }
    // Filter out empty strings and literal "null" strings (some models reply with the word "null")
    let brand = json
        .get("brand")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty() && *s != "null")
        .map(str::to_string);
    let category = json
        .get("category")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty() && *s != "null")
        .map(str::to_string);
    Some((name, brand, category))
}

/// Resolve a single barcode across every source, returning the first confident
/// match. Order: UPCitemdb (small daily quota — tried alone first); then the
/// unlimited community databases (Open Food/Products/Beauty/Pet-Food Facts,
/// Datakick, Brocade) run concurrently; then optional key-gated commercial
/// databases; then the AI fallback.
async fn lookup_chain(
    client: &reqwest::Client,
    barcode: &str,
    pool: &SqlitePool,
) -> Option<LookupHit> {
    // 1. UPCitemdb alone — it has a small daily quota, so don't spend a call on
    //    every barcode in the concurrent batch; a hit here skips everything else.
    if let Some(hit) = lookup_upcitemdb(client, barcode).await {
        return Some(hit);
    }

    // 2. Unlimited community databases — fired together, first hit by priority wins.
    let (food, products, beauty, petfood, datakick, brocade) = tokio::join!(
        lookup_openfacts(client, barcode, "world.openfoodfacts.org"),
        lookup_openfacts(client, barcode, "world.openproductsfacts.org"),
        lookup_openfacts(client, barcode, "world.openbeautyfacts.org"),
        lookup_openfacts(client, barcode, "world.openpetfoodfacts.org"),
        lookup_datakick(client, barcode),
        lookup_brocade(client, barcode),
    );
    if let Some(hit) = [food, products, beauty, petfood, datakick, brocade]
        .into_iter()
        .flatten()
        .next()
    {
        return Some(hit);
    }

    // 3. Optional commercial databases — only run if an API key is configured.
    //    EAN-Search first: best international/Gulf (EAN-13) coverage of the three.
    if let Some(hit) = lookup_eansearch(client, barcode, pool).await {
        return Some(hit);
    }
    if let Some(hit) = lookup_barcodelookup(client, barcode, pool).await {
        return Some(hit);
    }
    if let Some(hit) = lookup_upcdatabase(client, barcode, pool).await {
        return Some(hit);
    }
    if let Some(hit) = lookup_goupc(client, barcode, pool).await {
        return Some(hit);
    }

    // 4. AI fallback — last resort, infers the product from the number itself.
    if let Some((name, brand, category)) = lookup_ai(client, barcode, pool).await {
        let raw_json = serde_json::json!({
            "source": "ai_fallback",
            "product_name": name,
            "brand": brand,
            "category": category
        })
        .to_string();
        return Some(LookupHit {
            name,
            brand,
            category,
            image_url: None,
            raw_json,
        });
    }

    None
}

// ── Resolve command ───────────────────────────────────────────────────────────

/// Runs the multi-source lookup chain (`lookup_chain`) for every 'pending'
/// barcode, marking each 'found' or 'not_found'. Manager/owner only.
#[tauri::command]
pub async fn ghost_resolve(
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<ResolveResult> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;

    let pending: Vec<(String, String)> =
        sqlx::query_as("SELECT id, barcode FROM unknown_barcodes WHERE status = 'pending'")
            .fetch_all(&state.db)
            .await?;

    if pending.is_empty() {
        return Ok(ResolveResult {
            resolved: 0,
            not_found: 0,
        });
    }

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client build failed: {e}")))?;

    let mut resolved = 0i64;
    let mut not_found = 0i64;

    for (id, barcode) in &pending {
        if let Some(hit) = lookup_chain(&client, barcode, &state.db).await {
            sqlx::query(
                "UPDATE unknown_barcodes
                 SET status='found', product_name=?, brand=?, category=?, image_url=?, raw_json=?
                 WHERE id=?",
            )
            .bind(&hit.name)
            .bind(&hit.brand)
            .bind(&hit.category)
            .bind(&hit.image_url)
            .bind(&hit.raw_json)
            .bind(id)
            .execute(&state.db)
            .await?;
            resolved += 1;
        } else {
            sqlx::query("UPDATE unknown_barcodes SET status='not_found' WHERE id=?")
                .bind(id)
                .execute(&state.db)
                .await?;
            not_found += 1;
        }
    }

    Ok(ResolveResult {
        resolved,
        not_found,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn make_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");
        pool
    }

    /// Regression: `ghost_record` stores Unix-millis i64 into the TEXT-affinity
    /// first_seen_at/last_seen_at columns. `list_ghosts` must still decode them as
    /// i64 once a real row exists — before the CAST this failed with a sqlx decode
    /// error and surfaced as a generic "Something went wrong" to the cashier.
    #[tokio::test]
    async fn list_ghosts_decodes_text_stored_timestamps() {
        let pool = make_pool().await;
        let now: i64 = 1_718_000_000_000;
        sqlx::query(
            "INSERT INTO unknown_barcodes
               (id, barcode, scan_count, first_seen_at, last_seen_at, status,
                product_name, brand, category, image_url)
             VALUES (?, ?, 3, ?, ?, 'found', ?, ?, ?, ?)",
        )
        .bind("01JGHOST0000000000000001")
        .bind("5449000000996")
        .bind(now) // i64 bound into a TEXT column — mimics ghost_record
        .bind(now)
        .bind("Coca-Cola 330ml")
        .bind("Coca-Cola")
        .bind("Beverages")
        .bind("https://example.com/c.jpg")
        .execute(&pool)
        .await
        .expect("insert ghost row");

        let rows = list_ghosts(&pool)
            .await
            .expect("list_ghosts must decode the row");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].first_seen_at, now);
        assert_eq!(rows[0].last_seen_at, now);
        assert_eq!(rows[0].barcode, "5449000000996");
        assert_eq!(rows[0].brand.as_deref(), Some("Coca-Cola"));
    }
}
