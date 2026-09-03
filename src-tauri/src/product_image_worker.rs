//! Fills in product images on its own, slowly.
//!
//! The catalogue used to get pictures only when a manager pressed a button —
//! once per product, or once per page. That is fine for a shop adding a dozen
//! items; it is not how 28,000 products get illustrated, and a till showing a
//! grey box for most of the basket is harder to scan than one showing the
//! product.
//!
//! So this runs quietly in the background and does the same lookup the button
//! does. Two things shape it:
//!
//! **Newest first.** A product created a minute ago is the one someone is
//! waiting on, so it jumps the queue ahead of the historical backfill. That is
//! what makes "a new product gets an image by itself" true without a separate
//! code path for creation — there is one mechanism and the ordering does the
//! rest.
//!
//! **One at a time, at a pace it works out for itself.** The lookup goes out to
//! Open Food Facts and then to Bing, neither of which is ours, and firing
//! thousands of requests in a burst is how a shop's connection gets throttled
//! and the feature stops working for everyone in the building. Rather than
//! guess a safe rate, it starts at just over a second, speeds up while the
//! provider keeps answering and backs off hard the moment one stops. A
//! 28,000-product catalogue fills in overnight rather than over two days, and
//! a provider that objects is heard the first time it says so.

use std::sync::Arc;
use std::time::Duration;

use sqlx::{Row, SqlitePool};

use crate::commands::admin_commands::persist_product_image;
use crate::product_image_search::{
    search_product_image, ProductImageSearchMode, ProductImageSearchRequest,
};

/// The gap between lookups, adjusted as it goes.
///
/// A fixed six seconds was the first guess, made before anything was known
/// about how the providers would react. Measured against a real 28,000-product
/// catalogue the answer was that every single lookup succeeded — Open Food
/// Facts answers on the barcode and the Bing fallback was never needed. Six
/// seconds would have spent two days on work the provider was happy to serve
/// in nine hours.
///
/// So it starts brisk and slows down only when told to: success pulls the gap
/// toward the floor, a failure doubles it. A provider that starts refusing
/// backs this off to half a minute within a few tries, on its own, without
/// anyone having to guess a safe number in advance.
const MIN_INTERVAL: Duration = Duration::from_millis(1200);
const MAX_INTERVAL: Duration = Duration::from_secs(30);

/// How long to wait when there is nothing to do before asking again. Long
/// enough to cost nothing, short enough that a product added at the counter
/// gets its picture while the manager is still on the page.
const IDLE_INTERVAL: Duration = Duration::from_secs(30);

/// Attempts before a product is left alone. Most failures are "this product is
/// not in any public database" — true for unbranded and local goods — and no
/// amount of retrying changes that.
const MAX_ATTEMPTS: i64 = 4;

/// Backoff per attempt: ten minutes, an hour, six hours. A transient network
/// failure clears on the first step; a genuinely unfindable product walks out
/// to the end and stops.
fn backoff_minutes(attempts: i64) -> i64 {
    match attempts {
        0 | 1 => 10,
        2 => 60,
        _ => 360,
    }
}

/// What one pass did, which is what sets the pace for the next.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// Found a picture and saved it. Go a little faster.
    Filled,
    /// The lookup or the save failed. Back off.
    Failed,
    /// Nothing to do, or switched off.
    Idle,
}

/// A product waiting for a picture, with everything the lookup needs.
struct Candidate {
    product_id: String,
    name: String,
    barcode: Option<String>,
    sku: Option<String>,
    category_name: Option<String>,
    attempts: i64,
}

pub struct ProductImageWorker {
    db: SqlitePool,
}

impl ProductImageWorker {
    pub fn new(db: SqlitePool) -> Arc<Self> {
        Arc::new(Self { db })
    }

    pub fn spawn(worker: Arc<Self>) {
        tauri::async_runtime::spawn(async move {
            let mut interval = MIN_INTERVAL;
            loop {
                match worker.run_once().await {
                    Ok(Outcome::Filled) => {
                        // Eased back toward the floor rather than snapped to
                        // it, so one lucky lookup after a run of refusals does
                        // not immediately resume full speed.
                        interval = MIN_INTERVAL.max(interval.mul_f32(0.7));
                        tokio::time::sleep(interval).await;
                    }
                    Ok(Outcome::Failed) => {
                        interval = MAX_INTERVAL.min(interval * 2 + Duration::from_secs(1));
                        tokio::time::sleep(interval).await;
                    }
                    Ok(Outcome::Idle) => tokio::time::sleep(IDLE_INTERVAL).await,
                    Err(error) => {
                        // A database error here means something is wrong that
                        // retrying fast will not fix, and this is the least
                        // important thing the till is doing.
                        tracing::debug!("product image worker: {error}");
                        tokio::time::sleep(IDLE_INTERVAL).await;
                    }
                }
            }
        });
    }

    /// Whether the shop wants this running at all.
    ///
    /// On by default — the whole point is that nobody has to ask for it. But a
    /// background job that makes thousands of outbound requests over a shop's
    /// connection, unattended, needs a way to be stopped by someone who does
    /// not want it: a metered link, a network that objects to the traffic, or
    /// simply a manager who would rather choose the pictures themselves.
    async fn enabled(&self) -> bool {
        let value: Option<String> = sqlx::query_scalar(
            "SELECT value FROM app_config WHERE key = 'product_image_autofetch'",
        )
        .fetch_optional(&self.db)
        .await
        .ok()
        .flatten();
        !matches!(value.as_deref(), Some("0") | Some("false") | Some("off"))
    }

    /// One product. The outcome sets the pace for the next one.
    async fn run_once(&self) -> Result<Outcome, sqlx::Error> {
        if !self.enabled().await {
            return Ok(Outcome::Idle);
        }
        let Some(candidate) = self.next_candidate().await? else {
            return Ok(Outcome::Idle);
        };

        let request = ProductImageSearchRequest {
            product_name: candidate.name.clone(),
            barcode: candidate.barcode.clone(),
            sku: candidate.sku.clone(),
            category_name: candidate.category_name.clone(),
            current_image_url: None,
            mode: ProductImageSearchMode::Fetch,
        };

        match search_product_image(request).await {
            // A search-engine guess is not good enough to apply unattended.
            //
            // Only the Open Food Facts path looks the barcode up directly and so
            // knows it has *this* product. The Bing fallback returns whichever
            // image ranked first for a text query, with nothing checking that it
            // depicts the product — and this worker walks the whole catalogue
            // without anyone watching, so a bad guess becomes a wrong picture on
            // a shelf label, repeated across hundreds of products.
            //
            // Recorded as a failure so the existing backoff applies and the
            // product is retried later: Open Food Facts gains entries over time,
            // and a barcode absent today may resolve next month. The manual
            // buttons still offer the fallback, because a person can look at the
            // picture and judge it.
            Ok(result) if !result.barcode_verified => {
                self.record_failure(
                    &candidate,
                    "no barcode-verified image found (search-engine guesses are not applied automatically)",
                )
                .await?;
                return Ok(Outcome::Failed);
            }
            Ok(result) => {
                /* persist_product_image is the same function the manual button
                goes through, so an image found here is validated, audited
                and queued for sync exactly like one a manager chose. */
                match persist_product_image(&self.db, &candidate.product_id, &result.image_url)
                    .await
                {
                    Ok(_) => {
                        tracing::debug!(
                            "product image: {} <- {} ({})",
                            candidate.name,
                            result.image_url,
                            result.source
                        );
                        self.record_resolved(&candidate.product_id).await?;
                    }
                    // The product was deleted, or the URL failed validation.
                    // Either way this product is not worth asking about again
                    // right now.
                    Err(error) => {
                        self.record_failure(&candidate, &error.to_string()).await?;
                        return Ok(Outcome::Failed);
                    }
                }
            }
            Err(error) => {
                self.record_failure(&candidate, &error.to_string()).await?;
                return Ok(Outcome::Failed);
            }
        }
        Ok(Outcome::Filled)
    }

    /// The next product that needs a picture.
    ///
    /// The catalogue is the work list rather than a queue table: anything with
    /// no image and a barcode to search on qualifies, so a product whose image
    /// is later cleared by hand comes back round on its own and nothing has to
    /// remember to enqueue it.
    async fn next_candidate(&self) -> Result<Option<Candidate>, sqlx::Error> {
        let now = chrono::Utc::now().to_rfc3339();
        let row = sqlx::query(
            "SELECT p.product_id, p.name, p.barcode, p.sku, c.name AS category_name,
                    COALESCE(a.attempts, 0) AS attempts
             FROM products p
             LEFT JOIN categories c ON c.category_id = p.category_id
             LEFT JOIN product_image_attempts a ON a.product_id = p.product_id
             WHERE p.deleted_at IS NULL
               AND (p.image_path IS NULL OR TRIM(p.image_path) = '')
               AND p.barcode IS NOT NULL AND TRIM(p.barcode) <> ''
               AND COALESCE(a.attempts, 0) < ?
               AND (a.next_attempt_at IS NULL OR a.next_attempt_at <= ?)
             -- Newest first: the product someone just added is the one being
             -- waited on. The historical backfill fills in behind it.
             ORDER BY p.created_at DESC
             LIMIT 1",
        )
        .bind(MAX_ATTEMPTS)
        .bind(&now)
        .fetch_optional(&self.db)
        .await?;

        Ok(row.map(|row| Candidate {
            product_id: row.get("product_id"),
            name: row.get("name"),
            barcode: row.get("barcode"),
            sku: row.get("sku"),
            category_name: row.get("category_name"),
            attempts: row.get("attempts"),
        }))
    }

    async fn record_failure(&self, candidate: &Candidate, error: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now();
        let next = now + chrono::Duration::minutes(backoff_minutes(candidate.attempts));
        sqlx::query(
            "INSERT INTO product_image_attempts
               (product_id, attempts, next_attempt_at, last_error, created_at, updated_at)
             VALUES (?, 1, ?, ?, ?, ?)
             ON CONFLICT(product_id) DO UPDATE SET
               attempts        = attempts + 1,
               next_attempt_at = excluded.next_attempt_at,
               last_error      = excluded.last_error,
               updated_at      = excluded.updated_at",
        )
        .bind(&candidate.product_id)
        .bind(next.to_rfc3339())
        // Truncated: this is a breadcrumb for a manager, not a stack trace, and
        // a provider that returns an HTML error page should not put a page of
        // it in the catalogue database.
        .bind(error.chars().take(200).collect::<String>())
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn record_resolved(&self, product_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO product_image_attempts
               (product_id, attempts, next_attempt_at, resolved_at, created_at, updated_at)
             VALUES (?, 1, ?, ?, ?, ?)
             ON CONFLICT(product_id) DO UPDATE SET
               attempts    = attempts + 1,
               resolved_at = excluded.resolved_at,
               last_error  = NULL,
               updated_at  = excluded.updated_at",
        )
        .bind(product_id)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
