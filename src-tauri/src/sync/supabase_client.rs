use crate::errors::{AppError, AppResult};
use reqwest::{Client, StatusCode};
use serde_json::json;
use tracing;

// ── Client ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SupabaseClient {
    pub base_url: String,
    pub service_key: String,
    http: Client,
}

impl SupabaseClient {
    pub fn new(base_url: String, service_key: String) -> Self {
        let http = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            service_key,
            http,
        }
    }

    // ── Validation ───────────────────────────────────────────────────────────

    pub async fn validate(&self) -> AppResult<()> {
        let resp = self
            .http
            .get(format!("{}/rest/v1/", self.base_url))
            .header("apikey", &self.service_key)
            .header("Authorization", format!("Bearer {}", self.service_key))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Supabase connect error: {e}")))?;

        if resp.status().is_success() || resp.status() == StatusCode::NOT_FOUND {
            Ok(())
        } else if resp.status() == StatusCode::UNAUTHORIZED
            || resp.status() == StatusCode::FORBIDDEN
        {
            Err(AppError::Internal("Invalid Supabase service role key".into()))
        } else {
            Err(AppError::Internal(format!(
                "Supabase validation failed: {}",
                resp.status()
            )))
        }
    }

    // ── One-time schema migration via Management API ─────────────────────────

    pub async fn migrate(&self, pat: &str, project_ref: &str, sql: &str) -> AppResult<()> {
        let url = format!("https://api.supabase.com/v1/projects/{project_ref}/database/query");

        // Strip ALL comment lines BEFORE splitting by ; — prevents
        // semicolons inside comments from creating bogus statement boundaries.
        let clean_sql: String = sql
            .lines()
            .filter(|l| !l.trim().starts_with("--"))
            .collect::<Vec<_>>()
            .join("\n");

        let statements: Vec<String> = clean_sql
            .split(';')
            .map(|s| s.trim().replace('\r', ""))
            .filter(|s| !s.is_empty())
            .map(|s| format!("{s};"))
            .collect();

        for (i, stmt) in statements.iter().enumerate() {
            tracing::info!("Migration [{}/{}]: {:.80}...", i + 1, statements.len(), stmt);

            // Proactive throttle: 100 ms gap between statements keeps the
            // Supabase Management API well under its rate limit even for large
            // schemas (~120 statements = ~12 s, acceptable for one-time setup).
            if i > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            // Retry up to 4× on 429 with exponential back-off (0.5 s, 1 s, 2 s, 4 s).
            let mut attempt = 0u32;
            let result: AppResult<()> = loop {
                let resp = self
                    .http
                    .post(&url)
                    .header("Authorization", format!("Bearer {pat}"))
                    .header("Content-Type", "application/json")
                    .json(&json!({ "query": stmt }))
                    .send()
                    .await
                    .map_err(|e| AppError::Internal(format!(
                        "Migration HTTP error on statement {}: {e}", i + 1
                    )))?;

                let status = resp.status();

                if status == StatusCode::TOO_MANY_REQUESTS {
                    attempt += 1;
                    if attempt > 4 {
                        break Err(AppError::Internal(format!(
                            "Migration statement {} rate-limited after {attempt} retries", i + 1
                        )));
                    }
                    let wait_ms = 500u64 * (1u64 << (attempt - 1)); // 500 ms, 1 s, 2 s, 4 s
                    tracing::warn!(
                        "Rate-limited on migration statement {}. Waiting {wait_ms} ms (retry {attempt}/4)…",
                        i + 1
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                    continue;
                }

                if !status.is_success() {
                    let body = resp.text().await.unwrap_or_default();
                    let body_lower = body.to_lowercase();
                    if body_lower.contains("already exists") || body_lower.contains("does not exist") {
                        break Ok(());   // idempotent — schema already applied, skip
                    }
                    break Err(AppError::Internal(format!(
                        "Migration statement {} failed: {body}", i + 1
                    )));
                }

                break Ok(());
            };
            result?;
        }
        Ok(())
    }

    // ── Branch registry helpers ──────────────────────────────────────────────

    pub async fn pull_branch(&self) -> AppResult<Option<serde_json::Value>> {
        let url = format!(
            "{}/rest/v1/branches?is_active=eq.true&order=created_at.asc&limit=1",
            self.base_url
        );
        let resp = self
            .http
            .get(&url)
            .header("apikey", &self.service_key)
            .header("Authorization", format!("Bearer {}", self.service_key))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Supabase pull_branch error: {e}")))?;

        if resp.status().is_success() {
            let rows: Vec<serde_json::Value> = resp
                .json()
                .await
                .map_err(|e| AppError::Internal(format!("pull_branch parse error: {e}")))?;
            Ok(rows.into_iter().next())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(AppError::Internal(format!(
                "pull_branch failed ({status}): {body}"
            )))
        }
    }

    pub async fn upsert_branch(&self, branch: &serde_json::Value) -> AppResult<()> {
        let url = format!("{}/rest/v1/branches", self.base_url);
        let resp = self
            .http
            .post(&url)
            .header("apikey", &self.service_key)
            .header("Authorization", format!("Bearer {}", self.service_key))
            .header("Content-Type", "application/json")
            .header("Prefer", "resolution=merge-duplicates")
            .json(branch)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Supabase upsert_branch error: {e}")))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(AppError::Internal(format!(
                "upsert_branch failed ({status}): {body}"
            )))
        }
    }
}

// ── Extract project ref from Supabase URL ───────────────────────────────────

pub fn extract_project_ref(url: &str) -> Option<String> {
    let url = url.trim_end_matches('/');
    if url.is_empty() {
        return None;
    }
    let after_scheme = match url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    {
        Some(s) => s,
        None => return Some(url.to_string()),
    };
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
