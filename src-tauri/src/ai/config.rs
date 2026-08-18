use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// All AI runtime parameters, loaded from app_config with sensible defaults.
/// Changes take effect on the next chat message — no restart needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiParams {
    pub anthropic_max_tokens: u32,
    pub openai_max_tokens: u32,
    pub temperature: f32,
    /// `low` | `medium` | `high` | `xhigh` | `max`. None omits the field and
    /// takes the API default (`high`). Ignored by models that predate effort.
    pub effort: Option<String>,
    pub max_turns: usize,
    pub context_window_chars: usize,
    pub connect_timeout_secs: u64,
    pub stream_timeout_secs: u64,
    pub action_expiry_minutes: i64,
    pub bulk_batch_size: usize,
    pub tool_result_max_chars: usize,
    pub turn_tool_results_max_chars: usize,
    pub confirm_non_destructive_actions: bool,
    pub sensitive_protection_level: String,
}

impl Default for AiParams {
    fn default() -> Self {
        Self {
            anthropic_max_tokens: 32_000,
            openai_max_tokens: 16_384,
            temperature: 0.0,
            effort: Some("high".into()),
            max_turns: 16,
            context_window_chars: 700_000,
            connect_timeout_secs: 10,
            stream_timeout_secs: 1800,
            action_expiry_minutes: 10,
            bulk_batch_size: 100,
            tool_result_max_chars: 24_000,
            turn_tool_results_max_chars: 64_000,
            confirm_non_destructive_actions: false,
            sensitive_protection_level: "standard".into(),
        }
    }
}

const AI_PARAM_DEFAULTS: &[(&str, &str)] = &[
    ("ai_anthropic_max_tokens", "32000"),
    ("ai_openai_max_tokens", "16384"),
    ("ai_temperature", "0.0"),
    ("ai_context_window_chars", "700000"),
    ("ai_effort", "high"),
    ("ai_connect_timeout_secs", "10"),
    ("ai_stream_timeout_secs", "1800"),
    ("ai_action_expiry_minutes", "10"),
    ("ai_max_turns", "16"),
    ("ai_bulk_batch_size", "100"),
    ("ai_tool_result_max_chars", "24000"),
    ("ai_turn_tool_results_max_chars", "64000"),
    ("ai_confirm_non_destructive_actions", "false"),
    ("ai_sensitive_protection_level", "standard"),
];

pub async fn load_ai_params(pool: &SqlitePool) -> AiParams {
    let defaults = AiParams::default();

    // Batch all runtime config reads into one query instead of individual SELECTs.
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
        anthropic_max_tokens: cfg("ai_anthropic_max_tokens", "32000")
            .parse::<u32>()
            .unwrap_or(defaults.anthropic_max_tokens)
            // Current models cap at 128k output. Streaming is always on here, so
            // the SDK timeout that forces a lower ceiling elsewhere does not apply.
            .clamp(1, 128_000),
        openai_max_tokens: cfg("ai_openai_max_tokens", "16384")
            .parse::<u32>()
            .unwrap_or(defaults.openai_max_tokens)
            .clamp(1, 128_000),
        temperature: cfg("ai_temperature", "0.0")
            .parse::<f32>()
            .unwrap_or(defaults.temperature)
            .clamp(0.0, 2.0),
        effort: {
            let raw = cfg("ai_effort", "high");
            let level = raw.trim().to_ascii_lowercase();
            // An unrecognised value would be a 400, so anything unknown falls
            // back to omitting the field and taking the API default.
            matches!(level.as_str(), "low" | "medium" | "high" | "xhigh" | "max").then_some(level)
        },
        max_turns: cfg("ai_max_turns", "16")
            .parse::<usize>()
            .unwrap_or(defaults.max_turns)
            .clamp(1, 1000),
        context_window_chars: cfg("ai_context_window_chars", "700000")
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
        tool_result_max_chars: cfg("ai_tool_result_max_chars", "24000")
            .parse::<usize>()
            .unwrap_or(defaults.tool_result_max_chars)
            .clamp(4_000, 100_000),
        turn_tool_results_max_chars: cfg("ai_turn_tool_results_max_chars", "64000")
            .parse::<usize>()
            .unwrap_or(defaults.turn_tool_results_max_chars)
            .clamp(8_000, 250_000),
        confirm_non_destructive_actions: matches!(
            cfg("ai_confirm_non_destructive_actions", "false").as_str(),
            "1" | "true"
        ),
        sensitive_protection_level: {
            let value = cfg("ai_sensitive_protection_level", "standard")
                .trim()
                .to_ascii_lowercase();
            if matches!(value.as_str(), "standard" | "enhanced" | "maximum") {
                value
            } else {
                "maximum".into()
            }
        },
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

    #[tokio::test]
    async fn tool_result_budgets_default_and_clamp_to_safe_ranges() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let defaults = load_ai_params(&pool).await;
        assert_eq!(defaults.tool_result_max_chars, 24_000);
        assert_eq!(defaults.turn_tool_results_max_chars, 64_000);

        sqlx::query("INSERT INTO app_config(key,value,updated_at) VALUES('ai_tool_result_max_chars','1',datetime('now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO app_config(key,value,updated_at) VALUES('ai_turn_tool_results_max_chars','999999',datetime('now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
            .execute(&pool)
            .await
            .unwrap();

        let clamped = load_ai_params(&pool).await;
        assert_eq!(clamped.tool_result_max_chars, 4_000);
        assert_eq!(clamped.turn_tool_results_max_chars, 250_000);

        sqlx::query("UPDATE app_config SET value='invalid' WHERE key IN ('ai_tool_result_max_chars','ai_turn_tool_results_max_chars')")
            .execute(&pool)
            .await
            .unwrap();
        let invalid = load_ai_params(&pool).await;
        assert_eq!(invalid.tool_result_max_chars, 24_000);
        assert_eq!(invalid.turn_tool_results_max_chars, 64_000);
    }

    #[tokio::test]
    async fn saved_temperature_is_loaded_by_the_batched_query() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO app_config(key,value,updated_at) VALUES('ai_temperature','1.25',datetime('now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(load_ai_params(&pool).await.temperature, 1.25);
    }

    #[tokio::test]
    async fn non_destructive_confirmation_toggle_defaults_off_and_loads_saved_value() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        assert!(!load_ai_params(&pool).await.confirm_non_destructive_actions);
        sqlx::query("INSERT INTO app_config(key,value,updated_at) VALUES('ai_confirm_non_destructive_actions','true',datetime('now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
            .execute(&pool)
            .await
            .unwrap();
        assert!(load_ai_params(&pool).await.confirm_non_destructive_actions);
    }
}
