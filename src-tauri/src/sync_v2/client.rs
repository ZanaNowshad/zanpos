use crate::errors::{AppError, AppResult};
use reqwest::StatusCode;
use serde_json::Value;
use std::time::Duration;

/// Stable marker prefixed to transient (retryable) sync errors so the
/// worker can distinguish them from permanent failures without fragile substring
/// matching of OS-specific network error text.
pub const TRANSIENT_TAG: &str = "[TRANSIENT]";

#[derive(Clone)]
pub struct SupabaseClient {
    pub base_url: String,
    pub key: String,
    http: reqwest::Client,
}

impl SupabaseClient {
    pub fn new(url: &str, key: &str) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client");
        Self {
            base_url: url.trim_end_matches('/').to_string(),
            key: key.to_string(),
            http,
        }
    }

    /// Test connectivity and auth by hitting /rest/v1/.
    /// 404 on /rest/v1/ is normal for Supabase — it means auth passed.
    pub async fn validate(&self) -> AppResult<()> {
        let resp = self
            .http
            .get(format!("{}/rest/v1/", self.base_url))
            .header("apikey", &self.key)
            .header("Authorization", format!("Bearer {}", self.key))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Supabase connect error: {e}")))?;

        if resp.status().is_success() || resp.status() == StatusCode::NOT_FOUND {
            Ok(())
        } else if resp.status() == StatusCode::UNAUTHORIZED
            || resp.status() == StatusCode::FORBIDDEN
        {
            Err(AppError::Internal(
                "Invalid Supabase service role key".into(),
            ))
        } else {
            Err(AppError::Internal(format!(
                "Supabase validation failed: {}",
                resp.status()
            )))
        }
    }

    /// Push rows to a table via upsert, with exponential backoff retry for transient errors.
    /// Uses POST /rest/v1/{table} with Prefer: resolution=merge-duplicates.
    /// Columns are inferred from the JSON object keys.
    /// Retries up to MAX_HTTP_RETRIES times on 429 / 5xx / network errors,
    /// with wait = 2^attempt * BASE_RETRY_MS, capped at MAX_RETRY_DELAY_MS.
    pub async fn upsert_rows(&self, table: &str, rows: &[Value]) -> AppResult<()> {
        const MAX_HTTP_RETRIES: u32 = 5;
        const BASE_RETRY_MS: u64 = 200;
        const MAX_RETRY_DELAY_MS: u64 = 30_000;

        if rows.is_empty() {
            return Ok(());
        }

        let url = format!("{}/rest/v1/{}", self.base_url, table);
        let mut last_err: AppError = AppError::Internal("upsert: no attempts made".into());

        for attempt in 0..=MAX_HTTP_RETRIES {
            if attempt > 0 {
                let wait_ms = (BASE_RETRY_MS * (1u64 << attempt)).min(MAX_RETRY_DELAY_MS);
                tokio::time::sleep(Duration::from_millis(wait_ms)).await;
            }

            let send_result = self
                .http
                .post(&url)
                .header("apikey", &self.key)
                .header("Authorization", format!("Bearer {}", self.key))
                .header("Content-Type", "application/json")
                .header("Prefer", "resolution=merge-duplicates")
                .json(rows)
                .send()
                .await;

            let resp = match send_result {
                Ok(r) => r,
                Err(e) => {
                    // Network-level error (connect, timeout, DNS, TLS) — always transient
                    last_err = AppError::Internal(format!(
                        "{TRANSIENT_TAG} Supabase upsert error for {table}: {e}"
                    ));
                    tracing::warn!("upsert_rows {table}: network error attempt {attempt}: {e}");
                    continue; // retry
                }
            };

            let status = resp.status();
            if status.is_success() || status == StatusCode::CONFLICT {
                // 200/201: success. 409: duplicate — treat as success.
                return Ok(());
            } else if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
                // 5xx / 429: server-transient — retry
                let body = resp.text().await.unwrap_or_default();
                last_err = AppError::Internal(format!(
                    "{TRANSIENT_TAG} Upsert failed ({status}) for {table}: {body}"
                ));
                tracing::warn!("upsert_rows {table}: transient HTTP {status} attempt {attempt}");
                continue; // retry
            } else {
                // 4xx (except 429): permanent error — do NOT retry
                let body = resp.text().await.unwrap_or_default();
                return Err(AppError::Internal(format!(
                    "Upsert failed ({status}) for {table}: {body}"
                )));
            }
        }

        Err(last_err)
    }

    /// Pull rows updated after a timestamp, optionally excluding this device.
    /// If `exclude_device` is None, the origin_device_id filter is omitted
    /// (for tables that lack that column, e.g. branches, app_config).
    /// `page_limit` controls how many rows per request (caller passes PULL_PAGE_LIMIT).
    /// `offset` skips N rows within the result set (used for paginating through
    /// rows that share the same `updated_at` timestamp).
    /// `tiebreaker_col` adds a secondary sort column for deterministic ordering
    /// when offset pagination is used.
    /// Retries up to MAX_HTTP_RETRIES times on 429 / 5xx / network errors.
    pub async fn pull_rows(
        &self,
        table: &str,
        since: &str,
        exclude_device: Option<&str>,
        page_limit: usize,
        offset: usize,
        tiebreaker_col: Option<&str>,
    ) -> AppResult<Vec<Value>> {
        const MAX_HTTP_RETRIES: u32 = 5;
        const BASE_RETRY_MS: u64 = 200;
        const MAX_RETRY_DELAY_MS: u64 = 30_000;

        let since_safe = sanitize_timestamp_for_url(since);
        let order_clause = if let Some(tb) = tiebreaker_col {
            format!("updated_at.asc,{tb}.asc")
        } else {
            "updated_at.asc".to_string()
        };
        let mut query = format!(
            "updated_at=gt.{}&order={}&limit={}",
            since_safe, order_clause, page_limit
        );
        if offset > 0 {
            query.push_str(&format!("&offset={}", offset));
        }
        if let Some(dev) = exclude_device {
            query.push_str(&format!("&origin_device_id=neq.{}", dev));
        }
        let url = format!("{}/rest/v1/{}?{}", self.base_url, table, query);

        let mut last_err: AppError = AppError::Internal("pull: no attempts made".into());

        for attempt in 0..=MAX_HTTP_RETRIES {
            if attempt > 0 {
                let wait_ms = (BASE_RETRY_MS * (1u64 << attempt)).min(MAX_RETRY_DELAY_MS);
                tokio::time::sleep(Duration::from_millis(wait_ms)).await;
            }

            let send_result = self
                .http
                .get(&url)
                .header("apikey", &self.key)
                .header("Authorization", format!("Bearer {}", self.key))
                .header("Accept", "application/json")
                .send()
                .await;

            let resp = match send_result {
                Ok(r) => r,
                Err(e) => {
                    // Network-level error (connect, timeout, DNS, TLS) — always transient
                    last_err = AppError::Internal(format!(
                        "{TRANSIENT_TAG} Supabase pull error for {table}: {e}"
                    ));
                    tracing::warn!("pull_rows {table}: network error attempt {attempt}: {e}");
                    continue; // retry
                }
            };

            let status = resp.status();
            if status.is_success() {
                let rows: Vec<Value> = resp
                    .json()
                    .await
                    .map_err(|e| AppError::Internal(format!("Pull parse error for {table}: {e}")))?;
                return Ok(rows);
            } else if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
                // 5xx / 429: transient — retry
                let body = resp.text().await.unwrap_or_default();
                last_err = AppError::Internal(format!(
                    "{TRANSIENT_TAG} Pull failed ({status}) for {table}: {body}"
                ));
                tracing::warn!("pull_rows {table}: transient HTTP {status} attempt {attempt}");
                continue; // retry
            } else {
                // 4xx: permanent error — do NOT retry
                let body = resp.text().await.unwrap_or_default();
                return Err(AppError::Internal(format!(
                    "Pull failed ({status}) for {table}: {body}"
                )));
            }
        }

        Err(last_err)
    }

    /// Pull a single branch record for join-store.
    /// GET /rest/v1/branches?is_active=eq.1&order=created_at.asc&limit=1
    pub async fn pull_branch(&self) -> AppResult<Value> {
        let url = format!(
            "{}/rest/v1/branches?is_active=eq.true&order=created_at.asc&limit=1",
            self.base_url
        );
        let resp = self
            .http
            .get(&url)
            .header("apikey", &self.key)
            .header("Authorization", format!("Bearer {}", self.key))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Supabase pull_branch error: {e}")))?;

        if resp.status().is_success() {
            let rows: Vec<Value> = resp
                .json()
                .await
                .map_err(|e| AppError::Internal(format!("pull_branch parse error: {e}")))?;
            rows.into_iter()
                .next()
                .ok_or_else(|| AppError::NotFound("No active branch in central registry".into()))
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(AppError::Internal(format!(
                "pull_branch failed ({status}): {body}"
            )))
        }
    }

    /// Run central schema migration via Supabase Management API.
    /// POST to api.supabase.com/v1/projects/{ref}/database/query
    pub async fn migrate_schema(&self, pat: &str, schema_sql: &str) -> AppResult<()> {
        let project_ref = extract_project_ref(&self.base_url).ok_or_else(|| {
            AppError::Internal(format!(
                "Cannot extract project ref from base_url: {}",
                self.base_url
            ))
        })?;

        let url = format!(
            "https://api.supabase.com/v1/projects/{}/database/query",
            project_ref
        );

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", pat))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({ "query": schema_sql }))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Migration HTTP error: {e}")))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(AppError::Internal(format!("Migration failed: {body}")))
        }
    }
}

/// Extracts the project ref from a Supabase URL or returns the input as-is when
/// it is already a bare project ref (e.g. "abcdefgh").
///
/// Examples:
/// * `"https://xyz.supabase.co"` -> `Some("xyz")`
/// * `"xyz"` (bare ref) -> `Some("xyz")`
fn extract_project_ref(url: &str) -> Option<String> {
    let url = url.trim_end_matches('/');
    if url.is_empty() {
        return None;
    }
    // Strip scheme; if there's no scheme treat the whole string as the project ref.
    let after_scheme = match url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    {
        Some(s) => s,
        None => return Some(url.to_string()),
    };
    // First component before '.'
    let ref_part = after_scheme.split('.').next()?;
    if ref_part.is_empty() {
        None
    } else {
        Some(ref_part.to_string())
    }
}

/// Sanitize an ISO-8601 timestamp for safe inclusion in a Supabase URL query parameter.
///
/// Two transformations:
/// 1. Truncate sub-second digits to ≤ 6 (microsecond precision).
///    PostgREST rejects nanosecond-precision timestamps (9 digits) in URL parameters.
/// 2. URL-encode '+' → '%2B'.
///    A bare '+' in a query string is decoded as a space (application/x-www-form-urlencoded),
///    so the timezone offset `+00:00` becomes ` 00:00`, causing a parse failure in PostgREST.
fn sanitize_timestamp_for_url(ts: &str) -> String {
    // Step 1 — truncate sub-microsecond digits
    let truncated = if let Some(dot) = ts.find('.') {
        let rest = &ts[dot + 1..];
        let digits_end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        if digits_end > 6 {
            format!("{}.{}{}", &ts[..dot], &rest[..6], &rest[digits_end..])
        } else {
            ts.to_string()
        }
    } else {
        ts.to_string()
    };
    // Step 2 — encode '+' so it survives query-string parsing as a literal plus
    truncated.replace('+', "%2B")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_project_ref() {
        assert_eq!(
            extract_project_ref("https://xyz.supabase.co"),
            Some("xyz".into())
        );
        assert_eq!(
            extract_project_ref("https://xyz.supabase.co/"),
            Some("xyz".into())
        );
        assert_eq!(
            extract_project_ref("https://abcdefgh.supabase.co"),
            Some("abcdefgh".into())
        );
        assert_eq!(extract_project_ref("not-a-url"), Some("not-a-url".into()));
    }
}
