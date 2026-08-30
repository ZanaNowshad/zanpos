//! Bahrain Pharmacy — cosmetics, skincare, personal care, baby, health.
//!
//! The category Akelny is weakest on, and the source that turned out to matter
//! most for trust: its product pages publish a real `gtin`. Against a catalogue
//! holding 29,678 canonical barcodes that means a match here can be *exact* —
//! resolved by barcode, needing nobody's confirmation — where every other source
//! so far only offers a name to guess from.
//!
//! Plain WooCommerce underneath, so discovery is its own search
//! (`?s=…&post_type=product`) and the results page links products by a canonical
//! URL. The results page itself carries no product structured data — only a
//! `CollectionPage` — so each candidate is opened for its own.

use std::time::Duration;

use super::{product_from_page, SearchQuery, MAX_CANDIDATES};
use crate::errors::AppResult;
use crate::price_intelligence::{http, SourceProduct};

const SOURCE_ID: &str = "bahrain_pharmacy";
const PRODUCT_PREFIX: &str = "https://bahrainpharmacy.com/store/product/";

/// The site publishes no robots.txt at all, so nothing is asked of us and our
/// own floor is what applies. Being unpoliced is not a reason to go faster.
const CRAWL_DELAY: Option<Duration> = None;

/// WooCommerce's own search. `post_type=product` keeps blog posts out of the
/// results, which is the difference between three candidates and thirty.
fn search_url(term: &str) -> String {
    let encoded: String = term
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c.to_string(),
            ' ' => "+".to_string(),
            other => other
                .to_string()
                .bytes()
                .map(|b| format!("%{b:02X}"))
                .collect(),
        })
        .collect();
    format!("https://bahrainpharmacy.com/store/?s={encoded}&post_type=product")
}

/// Product slugs from a results page, in the order they appear.
///
/// The canonical product URL is the only stable thing on the page — the markup
/// around it is a theme's business and changes with it. Duplicates are expected:
/// each card links its product from the image, the title and the overlay.
pub fn slugs_from_results(html: &str) -> Vec<String> {
    let mut slugs: Vec<String> = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find(PRODUCT_PREFIX) {
        let after = &rest[start + PRODUCT_PREFIX.len()..];
        let end = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .unwrap_or(after.len());
        let slug = &after[..end];
        if !slug.is_empty() && !slugs.iter().any(|seen| seen == slug) {
            slugs.push(slug.to_string());
        }
        rest = &after[end.max(1)..];
    }
    slugs
}

pub async fn search(query: &SearchQuery) -> AppResult<Vec<SourceProduct>> {
    // WooCommerce search is an AND over words, so a full product name usually
    // returns nothing. The first few words carry the brand and the article,
    // which is what actually narrows it.
    let tokens = query.tokens();
    if tokens.is_empty() {
        return Ok(Vec::new());
    }
    let term = tokens.iter().take(3).cloned().collect::<Vec<_>>().join(" ");

    let results = http::fetch_text(&search_url(&term), CRAWL_DELAY).await?;
    let slugs = slugs_from_results(&results);

    let mut products = Vec::new();
    for slug in slugs.into_iter().take(MAX_CANDIDATES) {
        let url = format!("{PRODUCT_PREFIX}{slug}/");
        let Ok(html) = http::fetch_text(&url, CRAWL_DELAY).await else {
            continue;
        };
        // Size lives inside the name here, so there is no separate line to read.
        if let Some(product) = product_from_page(SOURCE_ID, &url, &html, Some(slug), None) {
            products.push(product);
        }
    }
    Ok(products)
}
