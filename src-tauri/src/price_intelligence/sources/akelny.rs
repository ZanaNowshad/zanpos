//! Akelny — one adapter, six Bahrain retailers.
//!
//! Worth more than any single shop's site: it is already a price comparison
//! service, so a product page carries Al Osra, LuLu, Al Helli, Tamimi, Ramez and
//! Hypermax side by side, in three-decimal dinars, with per-shop stock status.
//! It is also how LuLu grocery prices reach ZANPOS at all, LuLu's own storefront
//! being closed to non-browser clients.
//!
//! Two constraints shape the adapter, both set by the site rather than by us:
//!
//! **There is no search endpoint.** `/bh/search?q=` is a 404, and `/api` — which
//! is what the page's own search and its price-history chart call — is
//! disallowed by robots.txt. Discovery therefore runs off the published sitemap,
//! whose entries are `/bh/products/<slug>` and whose slugs are made from the
//! product name. Matching a name against 2,379 slugs locally costs the site one
//! request a day instead of one per lookup, which is a better deal for them than
//! a search endpoint would have been.
//!
//! **Its price history is behind that same `/api`.** So ZANPOS keeps its own, one
//! observation at a time. That is the arrangement the design wanted regardless:
//! a history assembled from what we actually saw is one we can explain.

use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::RwLock;

use super::{product_from_page, tokenize, SearchQuery, MAX_CANDIDATES};
use crate::errors::AppResult;
use crate::price_intelligence::{http, SourceProduct};

const SOURCE_ID: &str = "akelny";
const SITEMAP: &str = "https://akelny.net/product-sitemap/bh-unified-0.xml";
const PRODUCT_PREFIX: &str = "https://akelny.net/bh/products/";

/// Akelny publishes no Crawl-delay, so our own floor applies.
const CRAWL_DELAY: Option<Duration> = None;

/// How long a downloaded slug index stays usable.
///
/// The sitemap's own `lastmod` stamps move over weeks, and a listing added this
/// morning is not worth a 1.3 MB download to discover. A day keeps the index
/// current enough to price with and costs the site one request.
const INDEX_TTL: Duration = Duration::from_secs(24 * 60 * 60);

struct SlugIndex {
    slugs: Vec<String>,
    fetched_at: std::time::Instant,
}

fn index() -> &'static RwLock<Option<SlugIndex>> {
    static INDEX: OnceLock<RwLock<Option<SlugIndex>>> = OnceLock::new();
    INDEX.get_or_init(|| RwLock::new(None))
}

/// Slugs from a sitemap document.
///
/// Deliberately not an XML parser: the only thing wanted is the path after a
/// known prefix, and the document is a flat list of `<loc>` elements.
pub fn slugs_from_sitemap(xml: &str) -> Vec<String> {
    let mut slugs = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<loc>") {
        let after = &rest[start + 5..];
        let Some(end) = after.find("</loc>") else {
            break;
        };
        let url = after[..end].trim();
        if let Some(slug) = url.strip_prefix(PRODUCT_PREFIX) {
            let slug = slug.trim_end_matches('/');
            if !slug.is_empty() {
                slugs.push(slug.to_string());
            }
        }
        rest = &after[end..];
    }
    slugs
}

async fn slug_index() -> AppResult<Vec<String>> {
    if let Some(cached) = index().read().await.as_ref() {
        if cached.fetched_at.elapsed() < INDEX_TTL {
            return Ok(cached.slugs.clone());
        }
    }
    let xml = http::fetch_text(SITEMAP, CRAWL_DELAY).await?;
    let slugs = slugs_from_sitemap(&xml);
    *index().write().await = Some(SlugIndex {
        slugs: slugs.clone(),
        fetched_at: std::time::Instant::now(),
    });
    Ok(slugs)
}

/// Rank slugs against a product name.
///
/// Slugs are the product name with the punctuation removed, so token overlap is
/// a good signal. It is scored as a fraction of *our* tokens rather than a raw
/// count, so a slug that happens to be long does not outrank one that actually
/// says the same thing.
pub fn rank_slugs(slugs: &[String], query_tokens: &[String]) -> Vec<String> {
    if query_tokens.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(usize, &String)> = slugs
        .iter()
        .filter_map(|slug| {
            let slug_tokens = tokenize(&slug.replace('-', " "));
            let hits = query_tokens
                .iter()
                .filter(|token| slug_tokens.iter().any(|candidate| candidate == *token))
                .count();
            // Half the words of the product name have to appear, or "Almarai
            // Fresh Milk" matches every Almarai listing on the site.
            (hits * 2 >= query_tokens.len() && hits > 0).then_some((hits, slug))
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.len().cmp(&b.1.len())));
    scored
        .into_iter()
        .take(MAX_CANDIDATES)
        .map(|(_, slug)| slug.clone())
        .collect()
}

/// The size line, which the structured data leaves out.
///
/// It sits in a styled `div` immediately after the `h1`. Anchored on the
/// heading rather than on a class name, because the class names here are
/// generated and the ordering is not. Returning None is fine — a listing whose
/// size cannot be read simply cannot be auto-trusted on size.
pub fn pack_text_from_page(html: &str) -> Option<String> {
    let heading = html.find("<h1")?;
    let after_heading = html[heading..].find("</h1>")? + heading + 5;
    let rest = &html[after_heading..];
    let open = rest.find("<div")?;
    let text_start = rest[open..].find('>')? + open + 1;
    let text_end = rest[text_start..].find('<')? + text_start;
    let text = rest[text_start..text_end].trim();
    (!text.is_empty() && text.len() <= 60).then(|| text.to_string())
}

pub async fn search(query: &SearchQuery) -> AppResult<Vec<SourceProduct>> {
    let slugs = slug_index().await?;
    let ranked = rank_slugs(&slugs, &query.tokens());

    let mut products = Vec::new();
    for slug in ranked {
        let url = format!("{PRODUCT_PREFIX}{slug}");
        // One unreachable listing must not lose the others: a source is useful
        // when it answers about most things, not only when it answers about all.
        let Ok(html) = http::fetch_text(&url, CRAWL_DELAY).await else {
            continue;
        };
        let pack_text = pack_text_from_page(&html);
        if let Some(product) =
            product_from_page(SOURCE_ID, &url, &html, Some(slug.clone()), pack_text)
        {
            products.push(product);
        }
    }
    Ok(products)
}
