use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::errors::{AppError, AppResult};

// ── Row types returned by pull ─────────────────────────────────────────────────

// Fields marked allow(dead_code) are deserialized from Supabase pull responses
// and will be used in Phase 3 conflict-resolution logic.
#[derive(Debug, Deserialize, Clone)]
pub struct SyncEventRow {
    pub global_sequence: i64,
    #[allow(dead_code)]
    pub device_id:       String,
    #[allow(dead_code)]
    pub branch_id:       String,
    pub entity_type:     String,
    #[allow(dead_code)]
    pub entity_id:       String,
    #[allow(dead_code)]
    pub operation:       String,
    pub payload_json:    Value,
    #[allow(dead_code)]
    pub idempotency_key: String,
    #[allow(dead_code)]
    pub local_sequence:  i64,
    #[allow(dead_code)]
    pub created_at:      String,
}

// ── Local sync_queue row (subset needed for push) ─────────────────────────────

#[derive(Debug, Serialize)]
pub struct PushEvent {
    pub entity_type:     String,
    pub operation:       String,
    pub payload_json:    Value,
    pub idempotency_key: String,
}

// ── Client ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SupabaseClient {
    pub base_url:    String,
    pub service_key: String,
    http: Client,
}

impl SupabaseClient {
    pub fn new(base_url: String, service_key: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            service_key,
            http: Client::new(),
        }
    }

    // ── Validation ─────────────────────────────────────────────────────────────

    pub async fn validate(&self) -> AppResult<()> {
        let resp = self.http
            .get(format!("{}/rest/v1/", self.base_url))
            .header("apikey", &self.service_key)
            .header("Authorization", format!("Bearer {}", self.service_key))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Supabase connect error: {e}")))?;

        if resp.status().is_success() || resp.status() == StatusCode::NOT_FOUND {
            // 404 on /rest/v1/ is normal for Supabase — it means auth passed
            Ok(())
        } else if resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN {
            Err(AppError::Internal("Invalid Supabase service role key".into()))
        } else {
            Err(AppError::Internal(format!("Supabase validation failed: {}", resp.status())))
        }
    }

    // ── One-time schema migration via Management API ───────────────────────────

    pub async fn migrate(&self, pat: &str, project_ref: &str, sql: &str) -> AppResult<()> {
        let url = format!("https://api.supabase.com/v1/projects/{project_ref}/database/query");

        let resp = self.http
            .post(&url)
            .header("Authorization", format!("Bearer {pat}"))
            .header("Content-Type", "application/json")
            .json(&json!({ "query": sql }))
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

    // ── Push one event via RPC ─────────────────────────────────────────────────

    pub async fn push_event(&self, event: &PushEvent) -> AppResult<()> {
        let body = json!({
            "p_entity_type": event.entity_type,
            "p_operation":   event.operation,
            "p_payload":     event.payload_json,
            "p_idem_key":    event.idempotency_key,
        });

        let resp = self.http
            .post(format!("{}/rest/v1/rpc/apply_sync_event", self.base_url))
            .header("apikey", &self.service_key)
            .header("Authorization", format!("Bearer {}", self.service_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Supabase push error: {e}")))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(AppError::Internal(format!("Push failed ({status}): {body}")))
        }
    }

    // ── Pull events since watermark, excluding this device ────────────────────

    pub async fn pull_events(
        &self,
        since_seq: i64,
        exclude_device: &str,
    ) -> AppResult<Vec<SyncEventRow>> {
        let url = format!(
            "{}/rest/v1/sync_events?global_sequence=gt.{}&device_id=neq.{}&order=global_sequence.asc&limit=100",
            self.base_url, since_seq, exclude_device
        );

        let resp = self.http
            .get(&url)
            .header("apikey", &self.service_key)
            .header("Authorization", format!("Bearer {}", self.service_key))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Supabase pull error: {e}")))?;

        if resp.status().is_success() {
            let rows: Vec<SyncEventRow> = resp.json().await
                .map_err(|e| AppError::Internal(format!("Pull parse error: {e}")))?;
            Ok(rows)
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(AppError::Internal(format!("Pull failed ({status}): {body}")))
        }
    }

    // ── Branch registry helpers ────────────────────────────────────────────────

    /// Fetch the first active branch from the central `branches` table.
    /// Returns `None` if the table exists but has no rows.
    /// Returns an error if the request fails or the table doesn't exist.
    pub async fn pull_branch(&self) -> AppResult<Option<serde_json::Value>> {
        let url = format!(
            "{}/rest/v1/branches?is_active=eq.true&order=created_at.asc&limit=1",
            self.base_url
        );
        let resp = self.http
            .get(&url)
            .header("apikey", &self.service_key)
            .header("Authorization", format!("Bearer {}", self.service_key))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Supabase pull_branch error: {e}")))?;

        if resp.status().is_success() {
            let rows: Vec<serde_json::Value> = resp.json().await
                .map_err(|e| AppError::Internal(format!("pull_branch parse error: {e}")))?;
            Ok(rows.into_iter().next())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(AppError::Internal(format!("pull_branch failed ({status}): {body}")))
        }
    }

    /// Upsert a branch record into the central `branches` table.
    /// Used when a new store completes setup with Supabase configured.
    pub async fn upsert_branch(&self, branch: &serde_json::Value) -> AppResult<()> {
        let url = format!("{}/rest/v1/branches", self.base_url);
        let resp = self.http
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
            Err(AppError::Internal(format!("upsert_branch failed ({status}): {body}")))
        }
    }
} // end impl SupabaseClient

// ── Extract project ref from Supabase URL ─────────────────────────────────────

/// Extracts the project ref from a Supabase URL or returns the input as-is when
/// it is already a bare project ref (e.g. "abcdefgh").
///
/// Examples:
/// * `"https://xyz.supabase.co"` → `Some("xyz")`
/// * `"xyz"` (bare ref) → `Some("xyz")`
pub fn extract_project_ref(url: &str) -> Option<String> {
    let url = url.trim_end_matches('/');
    if url.is_empty() { return None; }
    // Strip scheme; if there's no scheme treat the whole string as the project ref.
    let after_scheme = match url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) {
        Some(s) => s,
        None => return Some(url.to_string()),
    };
    // First component before '.'
    let ref_part = after_scheme.split('.').next()?;
    if ref_part.is_empty() { None } else { Some(ref_part.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_project_ref() {
        assert_eq!(extract_project_ref("https://xyz.supabase.co"), Some("xyz".into()));
        assert_eq!(extract_project_ref("https://xyz.supabase.co/"), Some("xyz".into()));
        assert_eq!(extract_project_ref("https://abcdefgh.supabase.co"), Some("abcdefgh".into()));
        assert_eq!(extract_project_ref("not-a-url"), Some("not-a-url".into()));
    }
}
