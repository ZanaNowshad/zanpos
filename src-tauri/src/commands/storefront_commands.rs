use crate::commands::rbac;
use crate::errors::{AppError, AppResult};
use crate::storefront::catalog::build_catalog_snapshot;
use crate::storefront::publisher::{
    publish_catalog, snapshot_hash, upload_local_product_images, StorefrontPublisher,
};
use crate::{secure_store, AppState};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::time::Instant;
use tauri::State;

mod product_queries;
pub mod qr;
mod settings;

use product_queries::{product_by_id, products, products_page};
use settings::{
    cfg, load_settings, publisher, set_cfg, uses_cloudflare_connection_test, validate_settings,
    SECRET_KEY,
};

fn effective_dirty_count(
    product_dirty_count: i64,
    published_snapshot_hash: Option<&str>,
    current_snapshot_hash: &str,
) -> i64 {
    match published_snapshot_hash {
        Some(published) if published != current_snapshot_hash => product_dirty_count.max(1),
        _ => product_dirty_count,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorefrontSettings {
    pub enabled: bool,
    pub public_url: String,
    pub publish_url: String,
    pub whatsapp_number: String,
    pub locale: String,
    pub auto_publish: bool,
    /// Accepted on save for provisioning, never serialized back to the UI.
    #[serde(default, skip_serializing)]
    pub publish_secret: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StorefrontStatus {
    pub enabled: bool,
    pub connected: bool,
    pub last_release_at: Option<String>,
    pub last_release_id: Option<String>,
    pub published_product_count: i64,
    pub eligible_product_count: i64,
    pub dirty_product_count: i64,
    pub failed_product_count: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorefrontProduct {
    pub product_id: String,
    pub name: String,
    pub name_ar: Option<String>,
    pub description: Option<String>,
    pub description_ar: Option<String>,
    pub price_minor: i64,
    pub currency: String,
    pub image_url: Option<String>,
    pub published: bool,
    pub featured: bool,
    pub sort_order: i64,
    pub dirty: bool,
    pub publish_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StorefrontProductPage {
    pub items: Vec<StorefrontProduct>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Debug, Deserialize)]
pub struct StorefrontProductUpdate {
    pub published: Option<bool>,
    pub featured: Option<bool>,
    pub sort_order: Option<i64>,
    pub name_ar: Option<String>,
    pub description_ar: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PublishFailure {
    pub product_id: String,
    pub name: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct StorefrontPublishResult {
    pub success: bool,
    pub release_id: String,
    pub public_url: String,
    pub publish_url: String,
    pub published_count: i64,
    pub failed_count: i64,
    pub failures: Vec<PublishFailure>,
    pub published_at: String,
}

#[derive(Debug, Serialize)]
pub struct StorefrontConnectionResult {
    pub ok: bool,
    pub message: String,
    pub latency_ms: u128,
}

#[tauri::command]
pub async fn storefront_settings_get(
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<StorefrontSettings> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    load_settings(&state.db).await
}

#[tauri::command]
pub async fn storefront_settings_save(
    session_token: String,
    settings: StorefrontSettings,
    state: State<'_, AppState>,
) -> AppResult<StorefrontSettings> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    validate_settings(&settings)?;
    if let Some(secret) = settings.publish_secret.as_deref() {
        if secret.len() < 16 || secret.len() > 512 {
            return Err(AppError::Validation(
                "Publish secret must be between 16 and 512 characters".into(),
            ));
        }
        if !secure_store::set_secret(SECRET_KEY, secret) {
            return Err(AppError::Internal(
                "Could not save publish secret in OS credential store".into(),
            ));
        }
    }
    for (key, value) in [
        (
            "storefront_enabled",
            if settings.enabled { "1" } else { "0" },
        ),
        ("storefront_public_url", settings.public_url.trim()),
        (
            "storefront_publish_url",
            settings.publish_url.trim_end_matches('/'),
        ),
        (
            "storefront_whatsapp_number",
            settings.whatsapp_number.trim(),
        ),
        ("storefront_locale", settings.locale.as_str()),
        (
            "storefront_auto_publish",
            if settings.auto_publish { "1" } else { "0" },
        ),
    ] {
        set_cfg(&state.db, key, value).await?;
    }
    load_settings(&state.db).await
}

#[tauri::command]
pub async fn storefront_products_list(
    session_token: String,
    search: Option<String>,
    offset: Option<i64>,
    limit: Option<i64>,
    published_only: Option<bool>,
    state: State<'_, AppState>,
) -> AppResult<StorefrontProductPage> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    products_page(
        &state.db,
        search.as_deref(),
        offset.unwrap_or(0),
        limit.unwrap_or(25),
        published_only.unwrap_or(false),
    )
    .await
}

#[tauri::command]
pub async fn storefront_product_update(
    session_token: String,
    product_id: String,
    update: StorefrontProductUpdate,
    state: State<'_, AppState>,
) -> AppResult<StorefrontProduct> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    let current = product_by_id(&state.db, &product_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Eligible storefront product not found".into()))?;
    let name_ar = update.name_ar.as_deref().or(current.name_ar.as_deref());
    let description_ar = update
        .description_ar
        .as_deref()
        .or(current.description_ar.as_deref());
    sqlx::query(
        "INSERT INTO storefront_products
         (product_id,is_visible,name_ar,description_ar,featured,sort_order,updated_at)
         VALUES (?,?,?,?,?,?,?)
         ON CONFLICT(product_id) DO UPDATE SET
           is_visible=excluded.is_visible,name_ar=excluded.name_ar,
           description_ar=excluded.description_ar,featured=excluded.featured,
           sort_order=excluded.sort_order,updated_at=excluded.updated_at",
    )
    .bind(&product_id)
    .bind(update.published.unwrap_or(current.published) as i64)
    .bind(name_ar)
    .bind(description_ar)
    .bind(update.featured.unwrap_or(current.featured) as i64)
    .bind(update.sort_order.unwrap_or(current.sort_order))
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&state.db)
    .await?;
    product_by_id(&state.db, &product_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Storefront product not found".into()))
}

#[tauri::command]
pub async fn storefront_status(
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<StorefrontStatus> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    let settings = load_settings(&state.db).await?;
    let list = products(&state.db, None).await?;
    let release = sqlx::query(
        "SELECT release_id,published_at,snapshot_sha256 FROM storefront_releases
         WHERE status='published' ORDER BY version DESC LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?;
    let last_error = sqlx::query_scalar(
        "SELECT error FROM storefront_releases
         WHERE status='failed' ORDER BY version DESC LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .flatten();
    let mut current_snapshot = build_catalog_snapshot(&state.db).await?;
    current_snapshot.branch.phone = Some(settings.whatsapp_number.clone());
    let current_snapshot_hash = snapshot_hash(&current_snapshot)?;
    let published_snapshot_hash = release
        .as_ref()
        .map(|row| row.get::<String, _>("snapshot_sha256"));
    let dirty_product_count = effective_dirty_count(
        list.iter().filter(|product| product.dirty).count() as i64,
        published_snapshot_hash.as_deref(),
        &current_snapshot_hash,
    );
    Ok(StorefrontStatus {
        enabled: settings.enabled,
        connected: cfg(&state.db, "storefront_connected").await? == "1",
        last_release_at: release.as_ref().and_then(|row| row.get("published_at")),
        last_release_id: release.map(|row| row.get("release_id")),
        published_product_count: list.iter().filter(|product| product.published).count() as i64,
        eligible_product_count: list.len() as i64,
        dirty_product_count,
        failed_product_count: list
            .iter()
            .filter(|product| product.publish_error.is_some())
            .count() as i64,
        last_error,
    })
}

#[tauri::command]
pub async fn storefront_publish(
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<StorefrontPublishResult> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    let settings = load_settings(&state.db).await?;
    if !settings.enabled {
        return Err(AppError::Validation("Storefront is disabled".into()));
    }
    let visible: Vec<StorefrontProduct> = products(&state.db, None)
        .await?
        .into_iter()
        .filter(|product| product.published)
        .collect();
    let http_publisher = publisher(&settings)?;
    let image_failures = upload_local_product_images(&state.db, &http_publisher).await?;
    match publish_catalog(&state.db, &http_publisher, &settings.whatsapp_number).await {
        Ok(result) => {
            let failures: Vec<PublishFailure> = image_failures
                .into_iter()
                .map(|failure| PublishFailure {
                    product_id: failure.product_id,
                    name: failure.name,
                    message: failure.message,
                })
                .collect();
            crate::diagnostics::record_event(
                &state.db,
                "storefront_publish",
                Some(serde_json::json!({
                    "products": visible.len(),
                    "failed": failures.len(),
                })),
            )
            .await;
            Ok(StorefrontPublishResult {
                success: failures.is_empty(),
                release_id: result.release_id,
                public_url: settings.public_url,
                publish_url: settings.publish_url,
                published_count: visible.len() as i64,
                failed_count: failures.len() as i64,
                failures,
                published_at: result.published_at,
            })
        }
        Err(error) => Ok(StorefrontPublishResult {
            success: false,
            release_id: String::new(),
            public_url: settings.public_url,
            publish_url: settings.publish_url,
            published_count: 0,
            failed_count: visible.len() as i64,
            failures: visible
                .into_iter()
                .map(|product| PublishFailure {
                    product_id: product.product_id,
                    name: product.name,
                    message: error.user_message().into(),
                })
                .collect(),
            published_at: String::new(),
        }),
    }
}

#[tauri::command]
pub async fn storefront_connection_test(
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<StorefrontConnectionResult> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    let settings = load_settings(&state.db).await?;
    let started = Instant::now();
    let cloudflare_test = uses_cloudflare_connection_test(&settings.publish_url);
    let result = if cloudflare_test {
        crate::commands::cloudflare_commands::verify_stored_connection(&state.db).await
    } else {
        match publisher(&settings) {
            Ok(http_publisher) => http_publisher.test_connection().await,
            Err(error) => Err(error),
        }
    };
    let ok = result.is_ok();
    set_cfg(
        &state.db,
        "storefront_connected",
        if ok { "1" } else { "0" },
    )
    .await?;
    Ok(StorefrontConnectionResult {
        ok,
        message: match result {
            Ok(()) if cloudflare_test => {
                "Cloudflare is connected. Go live to create the customer URL.".into()
            }
            Ok(()) => "Connected to storefront".into(),
            Err(error) => error.user_message().into(),
        },
        latency_ms: started.elapsed().as_millis(),
    })
}

#[cfg(test)]
#[path = "storefront_commands_tests.rs"]
mod tests;
