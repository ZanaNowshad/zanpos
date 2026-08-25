//! Any storefront that describes its own products properly.
//!
//! This is what makes a new Bahrain retailer a row in `price_sources` rather
//! than a rewrite. Shopify, WooCommerce, Magento and most bespoke storefronts
//! publish `schema.org/Product` because search engines reward it — both live
//! sources do — so an adapter that reads only that covers a long tail nobody
//! has to write code for.
//!
//! It cannot invent a search endpoint, though. A source registered as generic
//! must supply a URL template saying how to search it, in its `base_url`:
//!
//! ```text
//! https://example.bh/search?q={query}
//! ```
//!
//! Without the placeholder there is no way in, and the adapter says so instead
//! of guessing at a path and knocking on doors that were never there.

use std::time::Duration;

use super::{product_from_page, SearchQuery, MAX_CANDIDATES};
use crate::errors::{AppError, AppResult};
use crate::price_intelligence::{http, SourceProduct};

const SOURCE_ID: &str = "generic";
const CRAWL_DELAY: Option<Duration> = None;

pub const QUERY_PLACEHOLDER: &str = "{query}";

fn encode(term: &str) -> String {
    term.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c.to_string(),
            ' ' => "+".to_string(),
            other => other
                .to_string()
                .bytes()
                .map(|b| format!("%{b:02X}"))
                .collect(),
        })
        .collect()
}

/// Absolute product links on the same host as the results page.
///
/// A results page links to a great many things — categories, the basket, social
/// accounts. Keeping only same-host links whose path looks like a product page
/// is a coarse filter, but the page that follows either yields a `Product` node
/// or is discarded, so a wrong guess here costs one request and nothing else.
pub fn candidate_links(html: &str, host: &str) -> Vec<String> {
    let mut links: Vec<String> = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("href=\"") {
        let after = &rest[start + 6..];
        let Some(end) = after.find('"') else { break };
        let href = &after[..end];
        rest = &after[end..];

        if !href.starts_with("https://") || !href.contains(host) {
            continue;
        }
        let looks_like_product = ["/product/", "/products/", "/p/", "/item/"]
            .iter()
            .any(|marker| href.contains(marker));
        if looks_like_product && !links.iter().any(|seen| seen == href) {
            links.push(href.to_string());
        }
    }
    links
}

pub async fn search(base_url: &str, query: &SearchQuery) -> AppResult<Vec<SourceProduct>> {
    if !base_url.contains(QUERY_PLACEHOLDER) {
        return Err(AppError::Validation(format!(
            "source has no search template: set base_url to a URL containing {QUERY_PLACEHOLDER}"
        )));
    }
    let tokens = query.tokens();
    if tokens.is_empty() {
        return Ok(Vec::new());
    }
    let term = tokens.iter().take(3).cloned().collect::<Vec<_>>().join(" ");
    let url = base_url.replace(QUERY_PLACEHOLDER, &encode(&term));

    let host = reqwest::Url::parse(&url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .unwrap_or_default();

    let results = http::fetch_text(&url, CRAWL_DELAY).await?;

    let mut products = Vec::new();
    for link in candidate_links(&results, &host).into_iter().take(MAX_CANDIDATES) {
        let Ok(html) = http::fetch_text(&link, CRAWL_DELAY).await else {
            continue;
        };
        if let Some(product) = product_from_page(SOURCE_ID, &link, &html, None, None) {
            products.push(product);
        }
    }
    Ok(products)
}
