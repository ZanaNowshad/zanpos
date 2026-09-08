//! Storefront settings: the `app_config` reads and writes that hold them, the
//! validation that runs before a save, and the publisher those settings build.
//!
//! Split out of `storefront_commands.rs` when that file passed the project's
//! 500-line limit. The limit is a proxy for keeping functions small: the release
//! profile builds with `lto = "fat"` and `codegen-units = 1`, which inlines
//! aggressively across module boundaries and has already produced one
//! release-only stack overflow that `cargo check` could not see.

use super::StorefrontSettings;
use crate::errors::{AppError, AppResult};
use crate::secure_store;
use crate::storefront::publisher::HttpPublisher;
use sqlx::SqlitePool;

/// Credential-manager entry holding the publish secret.
pub(super) const SECRET_KEY: &str = "storefront_publish_secret";

pub(super) async fn cfg(pool: &SqlitePool, key: &str) -> AppResult<String> {
    Ok(
        sqlx::query_scalar("SELECT value FROM app_config WHERE key=?")
            .bind(key)
            .fetch_optional(pool)
            .await?
            .unwrap_or_default(),
    )
}

pub(super) async fn set_cfg(pool: &SqlitePool, key: &str, value: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO app_config(key,value,updated_at) VALUES (?,?,?)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn load_settings(pool: &SqlitePool) -> AppResult<StorefrontSettings> {
    let locale = cfg(pool, "storefront_locale").await?;
    Ok(StorefrontSettings {
        enabled: matches!(
            cfg(pool, "storefront_enabled").await?.as_str(),
            "1" | "true"
        ),
        public_url: cfg(pool, "storefront_public_url").await?,
        publish_url: cfg(pool, "storefront_publish_url").await?,
        whatsapp_number: cfg(pool, "storefront_whatsapp_number").await?,
        locale: if locale.is_empty() {
            "en-ar".into()
        } else {
            locale
        },
        auto_publish: matches!(
            cfg(pool, "storefront_auto_publish").await?.as_str(),
            "1" | "true"
        ),
        publish_secret: None,
    })
}

pub(super) fn validate_settings(settings: &StorefrontSettings) -> AppResult<()> {
    if !matches!(settings.locale.as_str(), "en" | "ar" | "en-ar") {
        return Err(AppError::Validation("Invalid storefront locale".into()));
    }
    if !settings.public_url.is_empty()
        && !settings
            .public_url
            .to_ascii_lowercase()
            .starts_with("https://")
    {
        return Err(AppError::Validation(
            "Public storefront URL must use HTTPS".into(),
        ));
    }
    if settings.enabled && settings.publish_url.is_empty() {
        return Err(AppError::Validation(
            "Publish URL is required when storefront is enabled".into(),
        ));
    }
    if !settings.publish_url.is_empty() {
        HttpPublisher::new(&settings.publish_url, "validation-secret")?;
    }
    let phone = settings.whatsapp_number.trim_start_matches('+');
    if !phone.is_empty()
        && (phone.len() < 7 || phone.len() > 15 || !phone.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(AppError::Validation(
            "WhatsApp number must contain 7 to 15 digits".into(),
        ));
    }
    if settings.enabled && phone.is_empty() {
        return Err(AppError::Validation(
            "WhatsApp number is required when storefront is enabled".into(),
        ));
    }
    Ok(())
}

pub(super) fn publisher(settings: &StorefrontSettings) -> AppResult<HttpPublisher> {
    let secret = secure_store::get_secret(SECRET_KEY).ok_or_else(|| {
        AppError::Validation("Storefront publish secret is not configured".into())
    })?;
    HttpPublisher::new(&settings.publish_url, &secret)
}

pub(super) fn uses_cloudflare_connection_test(publish_url: &str) -> bool {
    publish_url.trim().is_empty()
}
