//! What a manager asking "what does everyone else charge for this" actually runs.
//!
//! The steps, and why they are in this order:
//!
//! 1. **Reuse what is already confirmed.** A pairing an operator has already
//!    approved is fetched by its stored key — exact, no scoring, no guessing.
//!    Asking a source to search again for a product we can already address is
//!    both slower and less accurate.
//! 2. **Route before searching.** A tin of evaporated milk has no business
//!    being looked up at a pharmacy. [`route`] narrows to sources that cover the
//!    product's category, so a lookup costs one or two requests rather than one
//!    per registered source.
//! 3. **Score what comes back, and stop there.** Anything unconfirmed is
//!    returned as a candidate for a person to accept or dismiss. It is recorded,
//!    so history accumulates from the first sighting, but [`observe`] will not
//!    quote it.
//!
//! Nothing in this module writes to `product_prices`. The panel fills the price
//! box in the product form and the operator saves through
//! `update_product_price`, keeping the RBAC, confirmation and audit trail that
//! already exist. There is deliberately no second path to a selling price.

use crate::errors::{AppError, AppResult};
use crate::price_intelligence::matching::{self, Candidate, StoredMatch};
use crate::price_intelligence::observe::{self, MarketSummary, Observation};
use crate::price_intelligence::pack::parse_pack;
use crate::price_intelligence::route;
use crate::price_intelligence::sources::{Adapter, SearchQuery};
use serde::Serialize;
use sqlx::{Row, SqlitePool};

/// Everything the product panel needs to draw itself.
#[derive(Debug, Clone, Serialize, Default)]
pub struct MarketPriceReport {
    pub product_id: String,
    pub product_name: String,
    /// Prices that may be quoted: one per retailer, from confirmed matches.
    pub trusted: Vec<Observation>,
    pub summary: MarketSummary,
    /// Listings that look right but nobody has approved. Kept in a separate
    /// field, not merged into `trusted` with a flag, because a single list with
    /// a boolean is one careless `.map()` away from being summed together.
    pub candidates: Vec<Candidate>,
    /// Sources that could not be consulted and why, so a thin result reads as
    /// "we could not look" rather than "nobody else sells this".
    pub unavailable: Vec<SourceStatus>,
    /// Whether this product is on the refresh watchlist. Carried in the report
    /// so the panel's toggle shows the state that exists rather than the one it
    /// last set — a checkbox that reads back false on every reopen would have
    /// people re-adding a product that was already tracked.
    pub tracked: bool,
}

/// Is this product on the refresh watchlist right now.
pub async fn is_tracked(pool: &SqlitePool, product_id: &str) -> AppResult<bool> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM price_watchlist
                        WHERE product_id = ? AND deleted_at IS NULL)",
    )
    .bind(product_id)
    .fetch_one(pool)
    .await?
        == 1)
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceStatus {
    pub source_id: String,
    pub name: String,
    pub status: String,
    pub reason: Option<String>,
    pub fallback_source_id: Option<String>,
}

struct RegisteredSource {
    source_id: String,
    name: String,
    base_url: String,
    categories: Vec<String>,
    status: String,
    status_reason: Option<String>,
    fallback: Option<String>,
}

async fn registered_sources(pool: &SqlitePool) -> AppResult<Vec<RegisteredSource>> {
    let rows = sqlx::query(
        "SELECT source_id, name, base_url, categories_json, status, status_reason,
                fallback_source_id
           FROM price_sources
          WHERE enabled = 1
          ORDER BY source_id",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| {
            let categories_json: String = row.get("categories_json");
            RegisteredSource {
                source_id: row.get("source_id"),
                name: row.get("name"),
                base_url: row.get("base_url"),
                categories: serde_json::from_str(&categories_json).unwrap_or_default(),
                status: row.get("status"),
                status_reason: row.get("status_reason"),
                fallback: row.get("fallback_source_id"),
            }
        })
        .collect())
}

struct ProductFacts {
    name: String,
    category: Option<String>,
    barcodes: Vec<String>,
}

async fn product_facts(pool: &SqlitePool, product_id: &str) -> AppResult<ProductFacts> {
    let row = sqlx::query(
        "SELECT p.name, c.name AS category_name
           FROM products p
           LEFT JOIN categories c ON c.category_id = p.category_id
          WHERE p.product_id = ? AND p.deleted_at IS NULL",
    )
    .bind(product_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Product {product_id} not found")))?;

    let barcodes: Vec<String> = sqlx::query_scalar(
        "SELECT barcode FROM product_barcodes WHERE product_id = ? AND deleted_at IS NULL",
    )
    .bind(product_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    Ok(ProductFacts {
        name: row.get("name"),
        category: row.get("category_name"),
        barcodes,
    })
}

/// Refresh one product's market prices and return what may be shown.
pub async fn search(pool: &SqlitePool, product_id: &str) -> AppResult<MarketPriceReport> {
    let facts = product_facts(pool, product_id).await?;
    let sources = registered_sources(pool).await?;
    let our_pack = parse_pack(&facts.name);

    let mut unavailable = Vec::new();
    for source in &sources {
        if source.status != "ok" {
            unavailable.push(SourceStatus {
                source_id: source.source_id.clone(),
                name: source.name.clone(),
                status: source.status.clone(),
                reason: source.status_reason.clone(),
                fallback_source_id: source.fallback.clone(),
            });
        }
    }

    let existing = matching::for_product(pool, product_id).await?;
    // No barcodes here: no Bahrain source offers a barcode search, so carrying
    // them into the query would be a field every adapter ignores. Where a
    // listing does publish a GTIN, `matching::barcode_match` compares it against
    // ours directly — which is the path that actually settles an exact match.
    let query = SearchQuery {
        name: facts.name.clone(),
    };

    // Confirmed pairings first, addressed by their stored key.
    let mut refreshed_sources: Vec<String> = Vec::new();
    for stored in existing.iter().filter(|m| m.is_trusted()) {
        let Some(source) = sources.iter().find(|s| s.source_id == stored.source_id) else {
            continue;
        };
        if source.status != "ok" {
            continue;
        }
        refreshed_sources.push(stored.source_id.clone());
        if let Err(error) = refresh_one(pool, source, stored, &query).await {
            tracing::warn!(
                source = %stored.source_id,
                %error,
                "market price refresh failed for a confirmed match"
            );
        }
    }

    // Then look for anything new, only in sources that cover this category and
    // that we have not already refreshed above.
    let routes = route::routes_for(&facts.name, facts.category.as_deref());
    let registered: Vec<(String, Vec<String>)> = sources
        .iter()
        .filter(|s| s.status == "ok" && !refreshed_sources.contains(&s.source_id))
        .map(|s| (s.source_id.clone(), s.categories.clone()))
        .collect();

    let mut candidates: Vec<Candidate> = Vec::new();
    for source_id in route::sources_for(&routes, &registered) {
        let Some(source) = sources.iter().find(|s| s.source_id == source_id) else {
            continue;
        };
        let adapter = Adapter::for_source(source_id);
        match adapter.search(&source.base_url, &query).await {
            Ok(listings) => {
                // A GTIN agreeing on both sides is not a guess, so it is filed
                // as BARCODE_EXACT and starts counting immediately rather than
                // waiting in the candidate list for somebody to approve what
                // the barcode already settled.
                let (exact, rest): (Vec<_>, Vec<_>) = listings
                    .into_iter()
                    .partition(|listing| matching::barcode_match(&facts.barcodes, listing));
                for listing in exact {
                    if let Err(error) =
                        auto_confirm_by_barcode(pool, product_id, &facts, &listing).await
                    {
                        tracing::warn!(%error, "barcode-exact match could not be recorded");
                    }
                }
                for candidate in matching::rank(&facts.name, our_pack.as_ref(), rest) {
                    // A pairing the operator already rejected must not come
                    // back on the next refresh wearing the same name.
                    let rejected = existing.iter().any(|m| {
                        m.source_id == candidate.source_id
                            && m.source_product_key == candidate.source_product_key
                            && m.status == "rejected"
                    });
                    if !rejected {
                        candidates.push(candidate);
                    }
                }
            }
            Err(error) => {
                tracing::warn!(source = %source_id, %error, "market price source failed");
                unavailable.push(SourceStatus {
                    source_id: source.source_id.clone(),
                    name: source.name.clone(),
                    status: "unreachable".into(),
                    reason: Some(error.to_string()),
                    fallback_source_id: source.fallback.clone(),
                });
            }
        }
    }

    Ok(MarketPriceReport {
        product_id: product_id.to_string(),
        product_name: facts.name,
        trusted: observe::latest_trusted_prices(pool, product_id).await?,
        summary: observe::market_summary(pool, product_id).await?,
        candidates,
        unavailable,
        tracked: is_tracked(pool, product_id).await?,
    })
}

/// File a barcode agreement as an exact match and record its prices.
async fn auto_confirm_by_barcode(
    pool: &SqlitePool,
    product_id: &str,
    facts: &ProductFacts,
    listing: &crate::price_intelligence::SourceProduct,
) -> AppResult<()> {
    let branch_id = crate::db::helpers::active_branch_id(pool)
        .await
        .unwrap_or_default();
    let candidate = matching::Candidate {
        source_id: listing.source_id.to_string(),
        source_product_key: listing.key.clone(),
        name: listing.name.clone(),
        pack_text: listing.pack_text.clone(),
        url: listing.url.clone(),
        confidence: 100,
        offers: listing.sellable_offers().cloned().collect(),
    };
    let match_id = matching::confirm_by_barcode(pool, product_id, &branch_id, &candidate).await?;
    let _ = facts;
    if let Some(stored) = matching::for_product(pool, product_id)
        .await?
        .into_iter()
        .find(|m| m.match_id == match_id)
    {
        observe::record(pool, &stored, &candidate.offers).await?;
    }
    Ok(())
}

/// Re-read one confirmed listing and record what it now costs.
async fn refresh_one(
    pool: &SqlitePool,
    source: &RegisteredSource,
    stored: &StoredMatch,
    query: &SearchQuery,
) -> AppResult<()> {
    let adapter = Adapter::for_source(&stored.source_id);
    let listings = adapter.search(&source.base_url, query).await?;
    let Some(listing) = listings
        .into_iter()
        .find(|l| l.key == stored.source_product_key)
    else {
        // The listing is gone. Not an error — products are delisted — but the
        // match stops being refreshable and says so rather than silently
        // holding a stale price forever.
        return Ok(());
    };

    if matching::suspend_if_pack_changed(pool, stored, &listing).await? {
        tracing::info!(
            match_id = %stored.match_id,
            "confirmed match suspended: the listing's pack size changed"
        );
        return Ok(());
    }

    let offers: Vec<_> = listing.sellable_offers().cloned().collect();
    observe::record(pool, stored, &offers).await?;
    Ok(())
}

/// Record that a candidate is the right product, then price against it.
pub async fn confirm_match(
    pool: &SqlitePool,
    product_id: &str,
    branch_id: &str,
    actor_user_id: &str,
    candidate: &Candidate,
) -> AppResult<MarketPriceReport> {
    let match_id = matching::confirm(pool, product_id, branch_id, actor_user_id, candidate).await?;

    // The offers already in hand are recorded immediately, so confirming makes
    // the panel useful now rather than after the next refresh cycle.
    if let Some(stored) = matching::for_product(pool, product_id)
        .await?
        .into_iter()
        .find(|m| m.match_id == match_id)
    {
        observe::record(pool, &stored, &candidate.offers).await?;
    }

    Ok(MarketPriceReport {
        product_id: product_id.to_string(),
        product_name: product_facts(pool, product_id).await?.name,
        trusted: observe::latest_trusted_prices(pool, product_id).await?,
        summary: observe::market_summary(pool, product_id).await?,
        candidates: Vec::new(),
        unavailable: Vec::new(),
        tracked: is_tracked(pool, product_id).await?,
    })
}

/// What is already known, without touching the network.
///
/// The panel opens with this so a product form is never waiting on somebody
/// else's server to render.
pub async fn cached_report(pool: &SqlitePool, product_id: &str) -> AppResult<MarketPriceReport> {
    Ok(MarketPriceReport {
        product_id: product_id.to_string(),
        product_name: product_facts(pool, product_id).await?.name,
        trusted: observe::latest_trusted_prices(pool, product_id).await?,
        summary: observe::market_summary(pool, product_id).await?,
        candidates: Vec::new(),
        unavailable: Vec::new(),
        tracked: is_tracked(pool, product_id).await?,
    })
}

/// Every source and its current state, including the ones we cannot reach.
pub async fn source_statuses(pool: &SqlitePool) -> AppResult<Vec<SourceStatus>> {
    Ok(registered_sources(pool)
        .await?
        .into_iter()
        .map(|s| SourceStatus {
            source_id: s.source_id,
            name: s.name,
            status: s.status,
            reason: s.status_reason,
            fallback_source_id: s.fallback,
        })
        .collect())
}

#[cfg(test)]
mod tests;
