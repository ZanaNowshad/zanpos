use crate::errors::{AppError, AppResult};
use crate::storefront::catalog::{build_catalog_snapshot, CatalogSnapshot};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;
use sqlx::SqlitePool;
use std::{future::Future, pin::Pin, time::Duration};

type HmacSha256 = Hmac<Sha256>;
const MAX_LOCAL_IMAGE_BYTES: usize = 8_000_000;
pub type PublishFuture<'a> = Pin<Box<dyn Future<Output = AppResult<()>> + Send + 'a>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboundRelease {
    pub release_id: String,
    pub version: i64,
    pub snapshot_sha256: String,
    pub snapshot: CatalogSnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishResult {
    pub release_id: String,
    pub version: i64,
    pub snapshot_sha256: String,
    pub published_at: String,
}

#[derive(Debug)]
pub struct ImageUploadFailure {
    pub product_id: String,
    pub name: String,
    pub message: String,
}

pub fn detect_image_media_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && matches!(&bytes[8..12], b"avif" | b"avis")
    {
        Some("image/avif")
    } else {
        None
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkerCatalog {
    version: String,
    updated_at: String,
    store: WorkerStore,
    currency: WorkerCurrency,
    categories: Vec<WorkerCategory>,
    products: Vec<WorkerProduct>,
}

#[derive(Serialize)]
struct Localized {
    en: String,
    ar: String,
}

#[derive(Serialize)]
struct WorkerStore {
    name: Localized,
    phone: String,
}

#[derive(Serialize)]
struct WorkerCurrency {
    code: String,
    decimals: u8,
}

#[derive(Serialize)]
struct WorkerCategory {
    id: String,
    name: Localized,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkerProduct {
    id: String,
    category_id: String,
    name: Localized,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<Localized>,
    price_minor: i64,
    available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<String>,
    quantity_decimals: u8,
}

pub trait StorefrontPublisher: Send + Sync {
    fn publish<'a>(&'a self, release: &'a OutboundRelease) -> PublishFuture<'a>;
    fn test_connection<'a>(&'a self) -> PublishFuture<'a>;
}

pub fn sign_request(
    secret: &[u8],
    method: &str,
    path: &str,
    timestamp: &str,
    body: &[u8],
) -> String {
    let body_hash = hex::encode(Sha256::digest(body));
    let canonical = format!(
        "{}\n{}\n{}\n{}",
        timestamp,
        method.to_ascii_uppercase(),
        path,
        body_hash
    );
    let mut mac =
        HmacSha256::new_from_slice(secret).expect("HMAC-SHA256 accepts keys of any length");
    mac.update(canonical.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub fn snapshot_hash(snapshot: &CatalogSnapshot) -> AppResult<String> {
    let mut stable = snapshot.clone();
    stable.generated_at.clear();
    let bytes = serde_json::to_vec(&stable)
        .map_err(|e| AppError::Internal(format!("Serialize storefront snapshot: {e}")))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

#[derive(Clone)]
pub struct HttpPublisher {
    base_url: String,
    secret: Vec<u8>,
    client: reqwest::Client,
}

impl HttpPublisher {
    pub fn new(base_url: &str, secret: &str) -> AppResult<Self> {
        let base_url = base_url.trim().trim_end_matches('/');
        let parsed = reqwest::Url::parse(base_url)
            .map_err(|_| AppError::Validation("Publish URL is invalid".into()))?;
        let local_http = parsed.scheme() == "http"
            && matches!(parsed.host_str(), Some("localhost" | "127.0.0.1"));
        if parsed.scheme() != "https" && !local_http {
            return Err(AppError::Validation(
                "Publish URL must use HTTPS (HTTP is allowed only for localhost)".into(),
            ));
        }
        if parsed.path() != "/" || parsed.query().is_some() || parsed.fragment().is_some() {
            return Err(AppError::Validation(
                "Publish URL must be an origin without a path, query, or fragment".into(),
            ));
        }
        if secret.len() < 16 || secret.len() > 512 {
            return Err(AppError::Validation(
                "Publish secret must be between 16 and 512 characters".into(),
            ));
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| AppError::Internal(format!("Build storefront client: {e}")))?;
        Ok(Self {
            base_url: base_url.into(),
            secret: secret.as_bytes().to_vec(),
            client,
        })
    }

    async fn signed_send(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Vec<u8>,
        content_type: &str,
        idempotency_key: &str,
        accept_not_found: bool,
    ) -> AppResult<()> {
        let timestamp = chrono::Utc::now().timestamp().to_string();
        let signature = sign_request(&self.secret, method.as_str(), path, &timestamp, &body);
        let response = self
            .client
            .request(method, format!("{}{}", self.base_url, path))
            .header("Content-Type", content_type)
            .header("x-zanpos-timestamp", timestamp)
            .header("x-zanpos-signature", signature)
            .header("x-idempotency-key", idempotency_key)
            .body(body)
            .send()
            .await
            .map_err(|e| AppError::Validation(format!("Storefront connection failed: {e}")))?;
        if !(response.status().is_success()
            || accept_not_found && response.status() == reqwest::StatusCode::NOT_FOUND)
        {
            return Err(AppError::Validation(format!(
                "Storefront returned HTTP {}",
                response.status()
            )));
        }
        Ok(())
    }

    pub async fn upload_image(
        &self,
        image_id: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> AppResult<String> {
        let path = format!("/api/publish/images/{image_id}");
        self.signed_send(
            reqwest::Method::PUT,
            &path,
            body,
            content_type,
            &format!("image-{image_id}"),
            false,
        )
        .await?;
        Ok(format!("/api/images/{image_id}"))
    }
}

fn worker_catalog(release: &OutboundRelease) -> WorkerCatalog {
    let snapshot = &release.snapshot;
    WorkerCatalog {
        version: release.version.to_string(),
        updated_at: snapshot.generated_at.clone(),
        store: WorkerStore {
            name: Localized {
                en: snapshot.branch.name.clone(),
                ar: snapshot.branch.name.clone(),
            },
            phone: snapshot.branch.phone.clone().unwrap_or_default(),
        },
        currency: WorkerCurrency {
            code: snapshot.currency.clone(),
            decimals: snapshot.currency_decimals,
        },
        categories: snapshot
            .categories
            .iter()
            .map(|category| WorkerCategory {
                id: category.id.clone(),
                name: Localized {
                    en: category.name.clone(),
                    ar: category
                        .name_ar
                        .clone()
                        .unwrap_or_else(|| category.name.clone()),
                },
            })
            .collect(),
        products: snapshot
            .products
            .iter()
            .map(|product| WorkerProduct {
                id: product.id.clone(),
                category_id: product.category_id.clone(),
                name: Localized {
                    en: product.name.clone(),
                    ar: product
                        .name_ar
                        .clone()
                        .unwrap_or_else(|| product.name.clone()),
                },
                description: product.description.as_ref().map(|description| Localized {
                    en: description.clone(),
                    ar: product
                        .description_ar
                        .clone()
                        .unwrap_or_else(|| description.clone()),
                }),
                price_minor: product.price_minor,
                available: product.availability == "available",
                image_url: product.image_url.clone(),
                quantity_decimals: product.quantity_decimals,
            })
            .collect(),
    }
}

impl StorefrontPublisher for HttpPublisher {
    fn publish<'a>(&'a self, release: &'a OutboundRelease) -> PublishFuture<'a> {
        Box::pin(async move {
            let body = serde_json::to_vec(&worker_catalog(release))
                .map_err(|e| AppError::Internal(format!("Serialize catalog: {e}")))?;
            let catalog_path = format!("/api/publish/catalog/{}", release.version);
            self.signed_send(
                reqwest::Method::PUT,
                &catalog_path,
                body,
                "application/json",
                &format!("catalog-{}", release.release_id),
                false,
            )
            .await?;
            let commit_path = format!("/api/publish/releases/{}/commit", release.version);
            self.signed_send(
                reqwest::Method::POST,
                &commit_path,
                b"{}".to_vec(),
                "application/json",
                &format!("commit-{}", release.release_id),
                false,
            )
            .await
        })
    }

    fn test_connection<'a>(&'a self) -> PublishFuture<'a> {
        Box::pin(async move {
            self.signed_send(
                reqwest::Method::POST,
                "/api/publish/releases/connection-test/commit",
                b"{}".to_vec(),
                "application/json",
                "connection-test",
                true,
            )
            .await
        })
    }
}

pub async fn upload_local_product_images(
    pool: &SqlitePool,
    publisher: &HttpPublisher,
) -> AppResult<Vec<ImageUploadFailure>> {
    let rows = sqlx::query(
        "SELECT p.product_id,p.name,p.image_path,sp.public_image_url
         FROM products p
         JOIN storefront_products sp ON sp.product_id=p.product_id AND sp.is_visible=1
         WHERE p.is_active=1 AND p.deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await?;
    let mut failures = Vec::new();
    for row in rows {
        let product_id: String = row.get("product_id");
        let name: String = row.get("name");
        let image_path: Option<String> = row.get("image_path");
        let Some(image_path) = image_path.filter(|path| !path.trim().is_empty()) else {
            continue;
        };
        let outcome = async {
            let bytes = tokio::fs::read(&image_path)
                .await
                .map_err(|_| AppError::Validation("Product image file is unavailable".into()))?;
            if bytes.is_empty() || bytes.len() > MAX_LOCAL_IMAGE_BYTES {
                return Err(AppError::Validation(
                    "Product image must be between 1 byte and 8 MB".into(),
                ));
            }
            let content_type = detect_image_media_type(&bytes).ok_or_else(|| {
                AppError::Validation("Product image must be PNG, JPEG, WebP, or AVIF".into())
            })?;
            let image_id = hex::encode(Sha256::digest(&bytes));
            let public_url = format!("/api/images/{image_id}");
            let current: Option<String> = row.get("public_image_url");
            if current.as_deref() != Some(public_url.as_str()) {
                publisher
                    .upload_image(&image_id, content_type, bytes)
                    .await?;
            }
            Ok::<String, AppError>(public_url)
        }
        .await;
        match outcome {
            Ok(public_url) => {
                sqlx::query(
                    "UPDATE storefront_products
                     SET public_image_url=?,publish_error=NULL,updated_at=?
                     WHERE product_id=?",
                )
                .bind(public_url)
                .bind(chrono::Utc::now().to_rfc3339())
                .bind(&product_id)
                .execute(pool)
                .await?;
            }
            Err(error) => {
                let message = error.user_message().to_string();
                sqlx::query(
                    "UPDATE storefront_products SET publish_error=?,updated_at=? WHERE product_id=?",
                )
                .bind(&message)
                .bind(chrono::Utc::now().to_rfc3339())
                .bind(&product_id)
                .execute(pool)
                .await?;
                failures.push(ImageUploadFailure {
                    product_id,
                    name,
                    message,
                });
            }
        }
    }
    Ok(failures)
}

pub async fn publish_catalog(
    pool: &SqlitePool,
    publisher: &dyn StorefrontPublisher,
    phone: &str,
) -> AppResult<PublishResult> {
    let mut snapshot = build_catalog_snapshot(pool).await?;
    snapshot.branch.phone = Some(phone.to_string());
    let snapshot_sha256 = snapshot_hash(&snapshot)?;
    let snapshot_json = serde_json::to_string(&snapshot)
        .map_err(|e| AppError::Internal(format!("Serialize storefront snapshot: {e}")))?;
    let version: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(version),0)+1 FROM storefront_releases")
            .fetch_one(pool)
            .await?;
    let release_id = ulid::Ulid::new().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO storefront_releases
         (release_id,version,status,snapshot_json,snapshot_sha256,created_at)
         VALUES (?,?,'staged',?,?,?)",
    )
    .bind(&release_id)
    .bind(version)
    .bind(&snapshot_json)
    .bind(&snapshot_sha256)
    .bind(&created_at)
    .execute(pool)
    .await?;

    let release = OutboundRelease {
        release_id: release_id.clone(),
        version,
        snapshot_sha256: snapshot_sha256.clone(),
        snapshot,
    };
    if let Err(error) = publisher.publish(&release).await {
        sqlx::query("UPDATE storefront_releases SET status='failed', error=? WHERE release_id=?")
            .bind(error.to_string())
            .bind(&release_id)
            .execute(pool)
            .await?;
        return Err(error);
    }

    let published_at = chrono::Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE storefront_releases
         SET status='published', published_at=?, error=NULL WHERE release_id=?",
    )
    .bind(&published_at)
    .bind(&release_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE storefront_products
         SET last_published_hash=?
         WHERE is_visible=1",
    )
    .bind(&snapshot_sha256)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(PublishResult {
        release_id,
        version,
        snapshot_sha256,
        published_at,
    })
}

#[cfg(test)]
#[path = "publisher_tests.rs"]
mod tests;
