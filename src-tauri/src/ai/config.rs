use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// All AI runtime parameters, loaded from app_config with sensible defaults.
/// Changes take effect on the next chat message — no restart needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiParams {
    pub anthropic_max_tokens: u32,
    pub openai_max_tokens: u32,
    pub temperature: f32,
    pub max_turns: usize,
    pub context_window_chars: usize,
    pub connect_timeout_secs: u64,
    pub stream_timeout_secs: u64,
    pub action_expiry_minutes: i64,
    pub bulk_batch_size: usize,
}

impl Default for AiParams {
    fn default() -> Self {
        Self {
            anthropic_max_tokens: 8096,
            openai_max_tokens: 8192,
            temperature: 0.0,
            max_turns: 8,
            context_window_chars: 320_000,
            connect_timeout_secs: 10,
            stream_timeout_secs: 1800,
            action_expiry_minutes: 10,
            bulk_batch_size: 100,
        }
    }
}

const AI_PARAM_DEFAULTS: &[(&str, &str)] = &[
    ("ai_anthropic_max_tokens", "8096"),
    ("ai_openai_max_tokens", "8192"),
    ("ai_context_window_chars", "320000"),
    ("ai_connect_timeout_secs", "10"),
    ("ai_stream_timeout_secs", "1800"),
    ("ai_action_expiry_minutes", "10"),
    ("ai_max_turns", "8"),
    ("ai_bulk_batch_size", "100"),
];

pub async fn load_ai_params(pool: &SqlitePool) -> AiParams {
    let defaults = AiParams::default();

    // Batch all 11 config reads into one query instead of 11 individual SELECTs.
    use std::collections::HashMap;
    let keys: Vec<&str> = AI_PARAM_DEFAULTS.iter().map(|(k, _)| *k).collect();
    let placeholders: Vec<String> = (0..keys.len()).map(|i| format!("?{}", i + 1)).collect();
    let sql = format!(
        "SELECT key, value FROM app_config WHERE key IN ({})",
        placeholders.join(",")
    );
    let mut q = sqlx::query_as::<_, (String, String)>(&sql);
    for k in &keys {
        q = q.bind(k);
    }
    let configs: HashMap<String, String> = q
        .fetch_all(pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect();

    let cfg = |key: &str, default: &str| -> String {
        configs
            .get(key)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    };

    AiParams {
        anthropic_max_tokens: cfg("ai_anthropic_max_tokens", "8096")
            .parse::<u32>()
            .unwrap_or(defaults.anthropic_max_tokens)
            .clamp(1, 32_000),
        openai_max_tokens: cfg("ai_openai_max_tokens", "8192")
            .parse::<u32>()
            .unwrap_or(defaults.openai_max_tokens)
            .clamp(1, 128_000),
        temperature: cfg("ai_temperature", "0.0")
            .parse::<f32>()
            .unwrap_or(defaults.temperature)
            .clamp(0.0, 2.0),
        max_turns: cfg("ai_max_turns", "50")
            .parse::<usize>()
            .unwrap_or(defaults.max_turns)
            .clamp(1, 1000),
        context_window_chars: cfg("ai_context_window_chars", "320000")
            .parse::<usize>()
            .unwrap_or(defaults.context_window_chars)
            .clamp(1_000, 1_000_000),
        connect_timeout_secs: cfg("ai_connect_timeout_secs", "10")
            .parse::<u64>()
            .unwrap_or(defaults.connect_timeout_secs)
            .clamp(1, 60),
        stream_timeout_secs: cfg("ai_stream_timeout_secs", "1800")
            .parse::<u64>()
            .unwrap_or(defaults.stream_timeout_secs)
            .clamp(300, 1800),
        action_expiry_minutes: cfg("ai_action_expiry_minutes", "10")
            .parse::<i64>()
            .unwrap_or(defaults.action_expiry_minutes)
            .clamp(1, 1440),
        bulk_batch_size: cfg("ai_bulk_batch_size", "100")
            .parse::<usize>()
            .unwrap_or(defaults.bulk_batch_size)
            .clamp(1, 500),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn stream_timeout_cannot_reintroduce_short_failures_or_exceed_client_ceiling() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let legacy_chat_timeout: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM app_config WHERE key='ai_chat_timeout_secs'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(legacy_chat_timeout, 0);
        sqlx::query("INSERT INTO app_config(key,value,updated_at) VALUES('ai_stream_timeout_secs','120',datetime('now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(load_ai_params(&pool).await.stream_timeout_secs, 300);
        sqlx::query("UPDATE app_config SET value='99999' WHERE key='ai_stream_timeout_secs'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(load_ai_params(&pool).await.stream_timeout_secs, 1_800);
    }
}
