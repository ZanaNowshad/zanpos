use crate::errors::{AppError, AppResult};
use reqwest::StatusCode;
use serde_json::Value;

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

    /// Push rows to a table via upsert.
    /// Uses POST /rest/v1/{table} with Prefer: resolution=merge-duplicates.
    /// Columns are inferred from the JSON object keys.
    pub async fn upsert_rows(&self, table: &str, rows: &[Value]) -> AppResult<()> {
        if rows.is_empty() {
            return Ok(());
        }

        let url = format!("{}/rest/v1/{}", self.base_url, table);

        let resp = self
            .http
            .post(&url)
            .header("apikey", &self.key)
            .header("Authorization", format!("Bearer {}", self.key))
            .header("Content-Type", "application/json")
            .header("Prefer", "resolution=merge-duplicates")
            .json(rows)
            .send()
            .await
            .map_err(|e| {
                AppError::Internal(format!(
                    "{TRANSIENT_TAG} Supabase upsert error for {table}: {e}"
                ))
            })?;

        let status = resp.status();
        if status.is_success() || status == StatusCode::CONFLICT {
            // 200/201: success. 409: duplicate — treat as success.
            Ok(())
        } else if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
            // 5xx / 429: server-transient — tag for retry.
            let body = resp.text().await.unwrap_or_default();
            Err(AppError::Internal(format!(
                "{TRANSIENT_TAG} Upsert failed ({status}) for {table}: {body}"
            )))
        } else {
            // 4xx: permanent error.
            let body = resp.text().await.unwrap_or_default();
            Err(AppError::Internal(format!(
                "Upsert failed ({status}) for {table}: {body}"
            )))
        }
    }

    /// Pull rows updated after a timestamp, optionally excluding this device.
    /// If `exclude_device` is None, the origin_device_id filter is omitted
    /// (for tables that lack that column, e.g. branches, app_config).
    pub async fn pull_rows(
        &self,
        table: &str,
        since: &str,
        exclude_device: Option<&str>,
    ) -> AppResult<Vec<Value>> {
        let mut query = format!(
            "updated_at=gt.{}&order=updated_at.asc&limit=100",
            since
        );
        if let Some(dev) = exclude_device {
            query.push_str(&format!("&origin_device_id=neq.{}", dev));
        }
        let url = format!("{}/rest/v1/{}?{}", self.base_url, table, query);

        let resp = self
            .http
            .get(&url)
            .header("apikey", &self.key)
            .header("Authorization", format!("Bearer {}", self.key))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| {
                AppError::Internal(format!(
                    "{TRANSIENT_TAG} Supabase pull error for {table}: {e}"
                ))
            })?;

        if resp.status().is_success() {
            let rows: Vec<Value> = resp
                .json()
                .await
                .map_err(|e| AppError::Internal(format!("Pull parse error for {table}: {e}")))?;
            Ok(rows)
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let transient =
                status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS;
            let tag = if transient { TRANSIENT_TAG } else { "" };
            Err(AppError::Internal(format!(
                "{tag} Pull failed ({status}) for {table}: {body}"
            )))
        }
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
