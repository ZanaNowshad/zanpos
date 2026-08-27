//! Recording what a competitor charged, and summarising only what may be quoted.
//!
//! Two jobs that look like one. Recording is generous: every offer read from
//! every match is written down, including ones against unconfirmed pairings,
//! because a history that starts only after somebody confirms a match is a
//! history missing the weeks before they got round to it.
//!
//! Summarising is strict. [`market_summary`] joins through
//! [`TRUSTED_MATCH_SQL`], so an observation recorded against a guess cannot
//! reach a minimum, a median or a maximum no matter which caller asks. The
//! filter lives in the query for a reason: a caller that forgets to filter is
//! the one bug this feature cannot afford, and a caller cannot forget a `WHERE`
//! clause it never writes.

use crate::errors::AppResult;
use crate::price_intelligence::matching::{StoredMatch, TRUSTED_MATCH_SQL};
use crate::price_intelligence::SourceOffer;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use ulid::Ulid;

/// One retailer's price as recorded at a point in time.
#[derive(Debug, Clone, Serialize)]
pub struct Observation {
    pub retailer_name: String,
    pub price_minor: i64,
    pub in_stock: bool,
    pub source_url: Option<String>,
    pub observed_at: String,
}

/// What the market looks like for one product, from trusted matches only.
#[derive(Debug, Clone, Serialize, Default)]
pub struct MarketSummary {
    pub low_minor: Option<i64>,
    pub median_minor: Option<i64>,
    pub high_minor: Option<i64>,
    /// How many retailers the figures above are drawn from. Shown because a
    /// median of one is a price, not a market, and a manager should be able to
    /// tell the difference at a glance.
    pub retailer_count: usize,
    pub observed_at: Option<String>,
}

/// Write down every offer a listing carried.
pub async fn record(
    pool: &SqlitePool,
    stored: &StoredMatch,
    offers: &[SourceOffer],
) -> AppResult<usize> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut written = 0usize;
    for offer in offers {
        sqlx::query(
            "INSERT INTO price_observations
                (observation_id, match_id, retailer_name, price_minor, currency,
                 in_stock, source_url, observed_at)
             VALUES (?, ?, ?, ?, 'BHD', ?, ?, ?)",
        )
        .bind(Ulid::new().to_string())
        .bind(&stored.match_id)
        .bind(&offer.retailer)
        .bind(offer.price_minor)
        .bind(i64::from(offer.in_stock))
        .bind(&offer.url)
        .bind(&now)
        .execute(pool)
        .await?;
        written += 1;
    }
    Ok(written)
}

/// The most recent price per retailer, for trusted matches only.
///
/// One row per retailer rather than one per observation: a source refreshed
/// daily for a month would otherwise let a single shop dominate the median
/// thirty times over.
pub async fn latest_trusted_prices(
    pool: &SqlitePool,
    product_id: &str,
) -> AppResult<Vec<Observation>> {
    let sql = format!(
        "SELECT o.retailer_name, o.price_minor, o.in_stock, o.source_url, o.observed_at
           FROM price_observations o
           JOIN product_matches m ON m.match_id = o.match_id
          WHERE m.product_id = ?
            AND m.deleted_at IS NULL
            AND {TRUSTED_MATCH_SQL}
            AND o.in_stock = 1
            AND o.observed_at = (
                SELECT MAX(o2.observed_at)
                  FROM price_observations o2
                  JOIN product_matches m2 ON m2.match_id = o2.match_id
                 WHERE m2.product_id = m.product_id
                   AND o2.retailer_name = o.retailer_name
            )
          GROUP BY o.retailer_name
          ORDER BY o.price_minor ASC"
    );

    let rows = sqlx::query(&sql).bind(product_id).fetch_all(pool).await?;
    Ok(rows
        .iter()
        .map(|row| Observation {
            retailer_name: row.get("retailer_name"),
            price_minor: row.get("price_minor"),
            in_stock: row.get::<i64, _>("in_stock") == 1,
            source_url: row.get("source_url"),
            observed_at: row.get("observed_at"),
        })
        .collect())
}

/// Low, median and high across trusted retailers.
pub async fn market_summary(pool: &SqlitePool, product_id: &str) -> AppResult<MarketSummary> {
    let prices = latest_trusted_prices(pool, product_id).await?;
    if prices.is_empty() {
        return Ok(MarketSummary::default());
    }

    let mut sorted: Vec<i64> = prices.iter().map(|p| p.price_minor).collect();
    sorted.sort_unstable();
    // `money::median_minor`, not a second implementation. It already decides
    // what an even count means — round toward the cheaper observation, so a
    // suggested price is never above something actually seen on a shelf — and
    // has a test pinning that. Two medians in one feature is two answers.
    let median = crate::price_intelligence::money::median_minor(&sorted);

    Ok(MarketSummary {
        low_minor: sorted.first().copied(),
        median_minor: median,
        high_minor: sorted.last().copied(),
        retailer_count: sorted.len(),
        observed_at: prices.iter().map(|p| p.observed_at.clone()).max(),
    })
}

/// Every observation for a product over time, newest first, trusted only.
pub async fn history(
    pool: &SqlitePool,
    product_id: &str,
    limit: i64,
) -> AppResult<Vec<Observation>> {
    let sql = format!(
        "SELECT o.retailer_name, o.price_minor, o.in_stock, o.source_url, o.observed_at
           FROM price_observations o
           JOIN product_matches m ON m.match_id = o.match_id
          WHERE m.product_id = ? AND m.deleted_at IS NULL AND {TRUSTED_MATCH_SQL}
          ORDER BY o.observed_at DESC, o.retailer_name
          LIMIT ?"
    );
    let rows = sqlx::query(&sql)
        .bind(product_id)
        .bind(limit.clamp(1, 500))
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .map(|row| Observation {
            retailer_name: row.get("retailer_name"),
            price_minor: row.get("price_minor"),
            in_stock: row.get::<i64, _>("in_stock") == 1,
            source_url: row.get("source_url"),
            observed_at: row.get("observed_at"),
        })
        .collect())
}

#[cfg(test)]
mod tests;
