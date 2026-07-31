//! One-click Cloudflare deployment for the public storefront.
//!
//! This is what makes "Connect Cloudflare" actually DO something: with only the
//! API token the admin already pasted, it provisions everything the storefront
//! needs — R2 bucket, the worker script (embedded in this binary as plain JS),
//! the HMAC publish secret, the workers.dev subdomain — and uploads the built
//! storefront SPA into R2 so the worker can serve it. No wrangler, no Node
//! toolchain, no manual steps on the shopkeeper's machine.
//!
//! Required token permissions (shown in the connect UI): Workers Scripts:Edit,
//! Workers R2 Storage:Edit, and the account-level read that comes with them.

use crate::errors::{AppError, AppResult};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;

const CF_API: &str = "https://api.cloudflare.com/client/v4";
pub const SCRIPT_NAME: &str = "zanpos-storefront";
pub const BUCKET_NAME: &str = "zanpos-catalog";
const WORKER_JS: &str = include_str!("worker_embedded.js");
const MAX_ASSET_BYTES: u64 = 5_000_000;
const MAX_ASSET_COUNT: usize = 200;

#[derive(Debug, Serialize)]
pub struct DeployReport {
    pub public_url: String,
    pub bucket_created: bool,
    pub script_uploaded: bool,
    pub assets_uploaded: u32,
    pub subdomain: String,
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build()
        .unwrap_or_default()
}

/// Parse a Cloudflare v4 envelope; Ok(result) on success, readable Err otherwise.
fn cf_result(body: &serde_json::Value, context: &str) -> AppResult<serde_json::Value> {
    if body.get("success").and_then(|v| v.as_bool()) == Some(true) {
        return Ok(body
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null));
    }
    let detail = body
        .get("errors")
        .and_then(|e| e.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let code = e.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
                    let msg = e.get("message").and_then(|m| m.as_str()).unwrap_or("");
                    if code == 10000 {
                        return Some(
                            "Reconnect using Create connection key and include Workers Scripts \
                             and Workers R2 Storage Write (code 10000)"
                                .into(),
                        );
                    }
                    if msg.is_empty() {
                        None
                    } else {
                        Some(format!("{msg} (code {code})"))
                    }
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown Cloudflare error".into());
    Err(AppError::Validation(format!(
        "Cloudflare {context}: {detail}"
    )))
}

fn cf_error_codes(body: &serde_json::Value) -> Vec<i64> {
    body.get("errors")
        .and_then(|e| e.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.get("code").and_then(|c| c.as_i64()))
                .collect()
        })
        .unwrap_or_default()
}

/// Create the R2 bucket; tolerate "already exists".
async fn ensure_bucket(token: &str, account_id: &str) -> AppResult<bool> {
    let resp = client()
        .post(format!("{CF_API}/accounts/{account_id}/r2/buckets"))
        .bearer_auth(token)
        .json(&serde_json::json!({ "name": BUCKET_NAME }))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Cloudflare unreachable: {e}")))?;
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    if body.get("success").and_then(|v| v.as_bool()) == Some(true) {
        return Ok(true);
    }
    // 10004: bucket already exists — fine, we own it.
    if cf_error_codes(&body).contains(&10004) {
        return Ok(false);
    }
    cf_result(&body, "R2 bucket creation failed").map(|_| false)
}

/// Upload the embedded worker as a module script with the R2 binding attached.
async fn upload_script(token: &str, account_id: &str) -> AppResult<()> {
    let metadata = serde_json::json!({
        "main_module": "index.mjs",
        "compatibility_date": "2026-07-01",
        "bindings": [
            { "type": "r2_bucket", "name": "CATALOG", "bucket_name": BUCKET_NAME }
        ]
    })
    .to_string();

    // Hand-rolled multipart (reqwest's multipart feature is not enabled).
    let boundary = format!("zanposboundary{}", ulid::Ulid::new());
    let mut body = Vec::with_capacity(WORKER_JS.len() + metadata.len() + 512);
    body.extend_from_slice(format!(
        "--{boundary}\r\ncontent-disposition: form-data; name=\"metadata\"\r\ncontent-type: application/json\r\n\r\n{metadata}\r\n"
    ).as_bytes());
    body.extend_from_slice(format!(
        "--{boundary}\r\ncontent-disposition: form-data; name=\"index.mjs\"; filename=\"index.mjs\"\r\ncontent-type: application/javascript+module\r\n\r\n"
    ).as_bytes());
    body.extend_from_slice(WORKER_JS.as_bytes());
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let resp = client()
        .put(format!(
            "{CF_API}/accounts/{account_id}/workers/scripts/{SCRIPT_NAME}"
        ))
        .bearer_auth(token)
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Cloudflare unreachable: {e}")))?;
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    cf_result(&body, "worker upload failed").map(|_| ())
}

/// Store the HMAC publish secret on the worker (re-applied after every upload).
async fn put_secret(token: &str, account_id: &str, secret: &str) -> AppResult<()> {
    let resp = client()
        .put(format!(
            "{CF_API}/accounts/{account_id}/workers/scripts/{SCRIPT_NAME}/secrets"
        ))
        .bearer_auth(token)
        .json(&serde_json::json!({
            "name": "PUBLISH_SECRET",
            "text": secret,
            "type": "secret_text"
        }))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Cloudflare unreachable: {e}")))?;
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    cf_result(&body, "setting the publish secret failed").map(|_| ())
}

/// The account-wide workers.dev subdomain (e.g. "myshop" → *.myshop.workers.dev).
async fn account_subdomain(token: &str, account_id: &str) -> AppResult<String> {
    let resp = client()
        .get(format!("{CF_API}/accounts/{account_id}/workers/subdomain"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Cloudflare unreachable: {e}")))?;
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    let result = cf_result(&body, "reading the workers.dev subdomain failed")?;
    result
        .get("subdomain")
        .and_then(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .ok_or_else(|| AppError::Validation(
            "This Cloudflare account has no workers.dev subdomain yet. Open dash.cloudflare.com → Workers & Pages once and choose a subdomain name, then deploy again.".into(),
        ))
}

/// Route the script onto its workers.dev URL.
async fn enable_subdomain(token: &str, account_id: &str) -> AppResult<()> {
    let resp = client()
        .post(format!(
            "{CF_API}/accounts/{account_id}/workers/scripts/{SCRIPT_NAME}/subdomain"
        ))
        .bearer_auth(token)
        .json(&serde_json::json!({ "enabled": true, "previews_enabled": false }))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Cloudflare unreachable: {e}")))?;
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    cf_result(&body, "enabling the public URL failed").map(|_| ())
}

fn content_type_for(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        _ => "application/octet-stream",
    }
}

/// Locate the built storefront SPA. Installed builds carry it as a bundled
/// resource ("storefront-dist"); dev builds fall back to ../storefront/dist.
pub fn locate_spa_dist(resource_dir: Option<PathBuf>) -> AppResult<PathBuf> {
    if let Some(dir) = resource_dir {
        let bundled = dir.join("storefront-dist");
        if bundled.join("index.html").is_file() {
            return Ok(bundled);
        }
        // Tauri maps ../ resources under _up_/ — accept that layout too.
        let up = dir.join("_up_").join("storefront").join("dist");
        if up.join("index.html").is_file() {
            return Ok(up);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../storefront/dist");
    if dev.join("index.html").is_file() {
        return Ok(dev);
    }
    Err(AppError::Validation(
        "The storefront web app build was not found. Rebuild ZANPOS (the installer bundles it) or run `npm run build` inside the storefront folder.".into(),
    ))
}

fn collect_assets(root: &PathBuf) -> AppResult<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .map_err(|e| AppError::Internal(format!("Read storefront build: {e}")))?
        {
            let entry =
                entry.map_err(|e| AppError::Internal(format!("Read storefront build: {e}")))?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .map_err(|_| AppError::Internal("Asset path escapes build dir".into()))?
                .to_string_lossy()
                .replace('\\', "/");
            // Only safe web asset paths; skip anything odd rather than failing.
            if rel.contains("..")
                || !rel
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c))
            {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if size == 0 || size > MAX_ASSET_BYTES {
                continue;
            }
            out.push((rel, path));
        }
    }
    if out.is_empty() {
        return Err(AppError::Validation(
            "The storefront build folder is empty".into(),
        ));
    }
    if out.len() > MAX_ASSET_COUNT {
        return Err(AppError::Validation(format!(
            "The storefront build has too many files ({}); expected at most {MAX_ASSET_COUNT}",
            out.len()
        )));
    }
    // index.html LAST: readers only see the new site once every hashed asset it
    // references is already in place (poor-man's atomic site release).
    out.sort_by_key(|(rel, _)| rel == "index.html");
    Ok(out)
}

/// Upload every SPA file into R2 under `site/**` via the R2 objects API.
async fn upload_assets(token: &str, account_id: &str, dist: &PathBuf) -> AppResult<u32> {
    let files = collect_assets(dist)?;
    let mut uploaded = 0u32;
    for (rel, path) in files {
        let bytes = std::fs::read(&path)
            .map_err(|e| AppError::Internal(format!("Read asset {rel}: {e}")))?;
        let key = format!("site/{rel}");
        let encoded: String = key
            .split('/')
            .map(urlencoding_lite)
            .collect::<Vec<_>>()
            .join("/");
        let resp = client()
            .put(format!(
                "{CF_API}/accounts/{account_id}/r2/buckets/{BUCKET_NAME}/objects/{encoded}"
            ))
            .bearer_auth(token)
            .header("content-type", content_type_for(&path))
            .body(bytes)
            .send()
            .await
            .map_err(|e| {
                AppError::Internal(format!("Cloudflare unreachable while uploading {rel}: {e}"))
            })?;
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        cf_result(&body, &format!("uploading {rel} failed"))?;
        uploaded += 1;
    }
    Ok(uploaded)
}

fn urlencoding_lite(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Full provisioning run. Idempotent: safe to re-run to update the worker/site.
pub async fn deploy(
    token: &str,
    account_id: &str,
    publish_secret: &str,
    resource_dir: Option<PathBuf>,
) -> AppResult<DeployReport> {
    let dist = locate_spa_dist(resource_dir)?;
    let bucket_created = ensure_bucket(token, account_id).await?;
    upload_script(token, account_id).await?;
    put_secret(token, account_id, publish_secret).await?;
    let subdomain = account_subdomain(token, account_id).await?;
    enable_subdomain(token, account_id).await?;
    let assets_uploaded = upload_assets(token, account_id, &dist).await?;
    Ok(DeployReport {
        public_url: format!("https://{SCRIPT_NAME}.{subdomain}.workers.dev"),
        bucket_created,
        script_uploaded: true,
        assets_uploaded,
        subdomain,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_types_cover_spa_outputs() {
        assert_eq!(
            content_type_for(std::path::Path::new("index.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            content_type_for(std::path::Path::new("assets/app.3f2a.js")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            content_type_for(std::path::Path::new("assets/app.css")),
            "text/css; charset=utf-8"
        );
    }

    #[test]
    fn url_encoding_keeps_slashes_out_of_segments() {
        assert_eq!(urlencoding_lite("app 1.js"), "app%201.js");
        assert_eq!(urlencoding_lite("índex"), "%C3%ADndex");
    }

    #[test]
    fn cf_error_parsing_surfaces_messages() {
        let body = serde_json::json!({
            "success": false,
            "errors": [{ "code": 10000, "message": "Authentication error" }]
        });
        let err = cf_result(&body, "test").unwrap_err();
        let detail = format!("{err:?}");
        assert!(detail.contains("Reconnect"));
        assert!(detail.contains("Workers R2 Storage Write"));
        assert!(detail.contains("code 10000"));
    }
}
