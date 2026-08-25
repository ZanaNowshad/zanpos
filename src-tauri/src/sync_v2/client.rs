use crate::errors::{AppError, AppResult};
use crate::hub::HubChangeEvent;
use futures::StreamExt;
use reqwest::StatusCode;
use serde_json::Value;
use std::time::Duration;

/// Stable marker prefixed to transient (retryable) sync errors so the
/// worker can distinguish them from permanent failures without fragile substring
/// matching of OS-specific network error text.
pub const TRANSIENT_TAG: &str = "[TRANSIENT]";

#[derive(Clone)]
pub struct HttpSyncClient {
    pub base_url: String,
    pub key: String,
    http: reqwest::Client,
}

impl HttpSyncClient {
    pub fn new(url: &str, key: &str, device_id: Option<&str>) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(dev) = device_id {
            if let Ok(v) = reqwest::header::HeaderValue::from_str(dev) {
                headers.insert("X-Zanpos-Device", v);
            }
        }
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(30))
            .default_headers(headers)
            .build()
            .expect("reqwest client");
        Self {
            base_url: url.trim_end_matches('/').to_string(),
            key: key.to_string(),
            http,
        }
    }

    /// Push rows to a table via upsert, with exponential backoff retry for transient errors.
    /// Uses POST /rest/v1/{table} with Prefer: resolution=merge-duplicates.
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
                    last_err = AppError::Internal(format!(
                        "{TRANSIENT_TAG} Hub upsert error for {table}: {e}"
                    ));
                    tracing::warn!("upsert_rows {table}: network error attempt {attempt}: {e}");
                    continue;
                }
            };

            let status = resp.status();
            if status.is_success() || status == StatusCode::CONFLICT {
                return Ok(());
            } else if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
                let body = resp.text().await.unwrap_or_default();
                last_err = AppError::Internal(format!(
                    "{TRANSIENT_TAG} Upsert failed ({status}) for {table}: {body}"
                ));
                tracing::warn!("upsert_rows {table}: transient HTTP {status} attempt {attempt}");
                continue;
            } else {
                let body = resp.text().await.unwrap_or_default();
                return Err(AppError::Internal(format!(
                    "Upsert failed ({status}) for {table}: {body}"
                )));
            }
        }

        Err(last_err)
    }

    /// Pull rows updated after a timestamp, optionally excluding this device.
    /// If `exclude_device` is None, the origin_device_id filter is omitted.
    /// `page_limit` controls how many rows per request.
    /// `offset` skips N rows within the result set.
    /// `tiebreaker_col` adds a secondary sort column for deterministic ordering.
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
                    last_err = AppError::Internal(format!(
                        "{TRANSIENT_TAG} Hub pull error for {table}: {e}"
                    ));
                    tracing::warn!("pull_rows {table}: network error attempt {attempt}: {e}");
                    continue;
                }
            };

            let status = resp.status();
            if status.is_success() {
                let rows: Vec<Value> = resp.json().await.map_err(|e| {
                    AppError::Internal(format!("Pull parse error for {table}: {e}"))
                })?;
                return Ok(rows);
            } else if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
                let body = resp.text().await.unwrap_or_default();
                last_err = AppError::Internal(format!(
                    "{TRANSIENT_TAG} Pull failed ({status}) for {table}: {body}"
                ));
                tracing::warn!("pull_rows {table}: transient HTTP {status} attempt {attempt}");
                continue;
            } else {
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
            .map_err(|e| AppError::Internal(format!("Hub pull_branch error: {e}")))?;

        if resp.status().is_success() {
            let rows: Vec<Value> = resp
                .json()
                .await
                .map_err(|e| AppError::Internal(format!("pull_branch parse error: {e}")))?;
            rows.into_iter()
                .next()
                .ok_or_else(|| AppError::NotFound("No active branch in store".into()))
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(AppError::Internal(format!(
                "pull_branch failed ({status}): {body}"
            )))
        }
    }

    /// GET {base}/zanpos/info — auth + identity probe for join/test.
    pub async fn hub_info(&self) -> AppResult<HubInfo> {
        let resp = self
            .http
            .get(format!("{}/zanpos/info", self.base_url))
            .header("Authorization", format!("Bearer {}", self.key))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("{TRANSIENT_TAG} Hub connect error: {e}")))?;
        match resp.status() {
            s if s.is_success() => resp
                .json::<HubInfo>()
                .await
                .map_err(|e| AppError::Internal(format!("Hub info parse error: {e}"))),
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                Err(AppError::Validation("Wrong store token".into()))
            }
            s => Err(AppError::Internal(format!("Hub returned {s}"))),
        }
    }

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
                let body: Value = resp.json().await.map_err(|e| {
                    AppError::Internal(format!("Hub parity rows parse error: {e}"))
                })?;
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

    /// Keep an authenticated SSE connection open and dispatch hub table changes.
    /// The caller owns reconnection/backoff so stream closure never stops polling sync.
    pub async fn listen_hub_changes<F, Fut>(&self, mut on_event: F) -> AppResult<()>
    where
        F: FnMut(HubChangeEvent) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let response = self
            .http
            .get(format!("{}/zanpos/events", self.base_url))
            .header("Authorization", format!("Bearer {}", self.key))
            .header("Accept", "text/event-stream")
            .send()
            .await
            .map_err(|e| {
                AppError::Internal(format!("{TRANSIENT_TAG} Hub event stream error: {e}"))
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "Hub event stream returned {status}: {body}"
            )));
        }

        let mut bytes = response.bytes_stream();
        let mut buffer = String::new();
        while let Some(chunk) = bytes.next().await {
            let chunk = chunk.map_err(|e| {
                AppError::Internal(format!("{TRANSIENT_TAG} Hub event stream interrupted: {e}"))
            })?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            buffer = buffer.replace("\r\n", "\n");
            while let Some(end) = buffer.find("\n\n") {
                let frame = buffer[..end].to_string();
                buffer.drain(..end + 2);
                let data = frame
                    .lines()
                    .filter_map(|line| line.strip_prefix("data:"))
                    .map(str::trim_start)
                    .collect::<Vec<_>>()
                    .join("\n");
                if data.is_empty() {
                    continue;
                }
                if let Ok(event) = serde_json::from_str::<HubChangeEvent>(&data) {
                    on_event(event).await;
                }
            }
        }
        Err(AppError::Internal(format!(
            "{TRANSIENT_TAG} Hub event stream closed"
        )))
    }
}

/// GET {base}/zanpos/info response.
#[derive(serde::Deserialize, Debug)]
pub struct HubInfo {
    pub store_name: String,
    pub branch_id: String,
    pub hub_version: String,
    pub hub_time: String,
}

/// Sanitize an ISO-8601 timestamp for safe inclusion in a URL query parameter.
/// Two transformations:
/// 1. Truncate sub-second digits to ≤ 6 (microsecond precision).
/// 2. URL-encode '+' → '%2B'.
fn sanitize_timestamp_for_url(ts: &str) -> String {
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
    truncated.replace('+', "%2B")
}
