//! What the hub thinks, so this terminal can compare rather than assume.
//!
//! A second `impl HttpSyncClient` rather than a separate type: these are the
//! same client, and splitting the struct would mean two connections and two
//! auth headers to keep in step. Rust allows inherent impls to live in any
//! module of the crate, so the grouping is by what the methods are for.
//!
//! All four are read-only diagnostics. They answer "does the hub agree with us",
//! which is a different job from moving rows (`upsert_rows`, `pull_rows`) or
//! staying connected (`heartbeat`, `listen_hub_changes`) — and the reason they
//! were worth separating is that a diagnostic must never be able to write.

use super::client::HttpSyncClient;
use super::client::TRANSIENT_TAG;
use crate::errors::{AppError, AppResult};
use serde_json::Value;

impl HttpSyncClient {
    /// GET {base}/zanpos/health — authenticated hub-wide health report.
    pub async fn hub_health(
        &self,
    ) -> AppResult<crate::commands::system_health_commands::SystemHealthReport> {
        let resp = self
            .http
            .get(format!("{}/zanpos/health", self.base_url))
            .header("Authorization", format!("Bearer {}", self.key))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("{TRANSIENT_TAG} Hub health error: {e}")))?;
        match resp.status() {
            s if s.is_success() => resp
                .json::<crate::commands::system_health_commands::SystemHealthReport>()
                .await
                .map_err(|e| AppError::Internal(format!("Hub health parse error: {e}"))),
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                Err(AppError::Validation("Wrong store token".into()))
            }
            s => {
                let body = resp.text().await.unwrap_or_default();
                Err(AppError::Internal(format!(
                    "Hub health returned {s}: {body}"
                )))
            }
        }
    }

    /// GET {base}/zanpos/parity — bucket digests, or the rows inside one bucket.
    ///
    /// `Ok(None)` when the hub does not know the route. A shop upgrades its
    /// terminals one at a time, so a newer terminal meeting an older hub is the
    /// normal case, and it should report "the hub cannot answer this yet"
    /// rather than an error that reads like a sync failure.
    pub async fn hub_parity(
        &self,
        table: &str,
        buckets: u32,
        bucket: Option<u32>,
    ) -> AppResult<Option<serde_json::Value>> {
        let mut url = format!(
            "{}/zanpos/parity?table={table}&buckets={buckets}",
            self.base_url
        );
        if let Some(bucket) = bucket {
            url.push_str(&format!("&bucket={bucket}"));
        }
        let resp = self
            .http
            .get(url)
            .header("Authorization", format!("Bearer {}", self.key))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("{TRANSIENT_TAG} Hub parity error: {e}")))?;
        match resp.status() {
            s if s.is_success() => resp
                .json::<serde_json::Value>()
                .await
                .map(Some)
                .map_err(|e| AppError::Internal(format!("Hub parity parse error: {e}"))),
            reqwest::StatusCode::NOT_FOUND => Ok(None),
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                Err(AppError::Validation("Wrong store token".into()))
            }
            s => Err(AppError::Internal(format!("Hub parity failed: {s}"))),
        }
    }

    /// POST {base}/zanpos/parity/rows — the rows behind specific primary keys.
    ///
    /// The repair fetch: given the keys parity found missing, ask for exactly
    /// those and nothing else. `Ok(None)` means an older hub that has no such
    /// route, which is a reason to stop rather than an error to report.
    pub async fn hub_parity_rows(
        &self,
        table: &str,
        pks: &[String],
    ) -> AppResult<Option<Vec<Value>>> {
        if pks.is_empty() {
            return Ok(Some(Vec::new()));
        }
        let resp = self
            .http
            .post(format!("{}/zanpos/parity/rows", self.base_url))
            .header("Authorization", format!("Bearer {}", self.key))
            .json(&serde_json::json!({ "table": table, "pks": pks }))
            .send()
            .await
            .map_err(|e| {
                AppError::Internal(format!("{TRANSIENT_TAG} Hub parity rows error: {e}"))
            })?;
        match resp.status() {
            s if s.is_success() => {
                let body: Value = resp
                    .json()
                    .await
                    .map_err(|e| AppError::Internal(format!("Hub parity rows parse error: {e}")))?;
                Ok(Some(
                    body.get("rows")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                ))
            }
            reqwest::StatusCode::NOT_FOUND => Ok(None),
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                Err(AppError::Validation("Wrong store token".into()))
            }
            s => {
                let body = resp.text().await.unwrap_or_default();
                Err(AppError::Internal(format!(
                    "Hub parity rows failed ({s}): {body}"
                )))
            }
        }
    }

    /// GET {base}/zanpos/consistency — authenticated hub table counts/checksums.
    pub async fn hub_consistency(
        &self,
    ) -> AppResult<crate::sync_v2::consistency::ConsistencySnapshot> {
        let resp = self
            .http
            .get(format!("{}/zanpos/consistency", self.base_url))
            .header("Authorization", format!("Bearer {}", self.key))
            .send()
            .await
            .map_err(|e| {
                AppError::Internal(format!("{TRANSIENT_TAG} Hub consistency error: {e}"))
            })?;
        match resp.status() {
            s if s.is_success() => resp
                .json::<crate::sync_v2::consistency::ConsistencySnapshot>()
                .await
                .map_err(|e| AppError::Internal(format!("Hub consistency parse error: {e}"))),
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                Err(AppError::Validation("Wrong store token".into()))
            }
            s => {
                let body = resp.text().await.unwrap_or_default();
                Err(AppError::Internal(format!(
                    "Hub consistency returned {s}: {body}"
                )))
            }
        }
    }
}
