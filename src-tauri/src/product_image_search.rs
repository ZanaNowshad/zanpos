//! Server-side product image discovery.
//!
//! The frontend CSP deliberately cannot make arbitrary web requests. Search is
//! therefore performed here, using barcode data from Open Food Facts first and
//! Bing Images' async result markup as the wider fallback. The small Bing
//! parser is inspired by the MIT-licensed `bing-image-urls` project:
//! https://github.com/ffreemt/bing-image-urls

use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::IpAddr;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProductImageSearchMode {
    Fetch,
    Change,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductImageSearchRequest {
    pub product_name: String,
    pub barcode: Option<String>,
    pub sku: Option<String>,
    pub category_name: Option<String>,
    pub current_image_url: Option<String>,
    pub mode: ProductImageSearchMode,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductImageSearchResult {
    pub image_url: String,
    pub source: String,
    pub search_query: String,
    pub alternate_count: usize,
}

#[derive(Debug, Deserialize)]
struct OpenFoodFactsResponse {
    status: Option<i64>,
    product: Option<OpenFoodFactsProduct>,
}

#[derive(Debug, Deserialize)]
struct OpenFoodFactsProduct {
    image_front_url: Option<String>,
    image_url: Option<String>,
    image_front_small_url: Option<String>,
}

fn clean_piece(value: Option<&str>, max_chars: usize) -> Option<String> {
    let compact = value?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(max_chars)
        .collect::<String>();
    (!compact.is_empty()).then_some(compact)
}

pub(crate) fn validate_search_identity(
    product_name: &str,
    barcode: Option<&str>,
) -> AppResult<(String, String)> {
    let product_name = clean_piece(Some(product_name), 100).ok_or_else(|| {
        AppError::Validation("Enter the product name before searching for an image".into())
    })?;
    let barcode = clean_piece(barcode, 32).ok_or_else(|| {
        AppError::Validation("Enter the product barcode before searching for an image".into())
    })?;
    Ok((product_name, barcode))
}

pub(crate) fn build_search_query(
    product_name: &str,
    barcode: Option<&str>,
    sku: Option<&str>,
    category_name: Option<&str>,
    mode: ProductImageSearchMode,
) -> String {
    let mut parts = Vec::new();
    if let Some(value) = clean_piece(barcode, 32) {
        parts.push(value);
    }
    if let Some(value) = clean_piece(Some(product_name), 100) {
        parts.push(value);
    }
    if mode == ProductImageSearchMode::Change {
        if let Some(value) = clean_piece(category_name, 50) {
            parts.push(value);
        }
        if let Some(value) = clean_piece(sku, 40) {
            parts.push(value);
        }
        parts.push("product packaging front".to_string());
    }
    parts.join(" ").chars().take(220).collect()
}

fn decode_html_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn is_public_image_url(value: &str) -> bool {
    if value.len() > 2_048 {
        return false;
    }
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host_lower = host.to_ascii_lowercase();
    if host_lower == "localhost"
        || host_lower.ends_with(".localhost")
        || host_lower.ends_with(".local")
    {
        return false;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => {
                !(v4.is_private()
                    || v4.is_loopback()
                    || v4.is_link_local()
                    || v4.is_unspecified()
                    || v4.is_broadcast()
                    || v4.is_documentation())
            }
            IpAddr::V6(v6) => {
                let first = v6.octets()[0];
                !(v6.is_loopback() || v6.is_unspecified() || (first & 0xfe) == 0xfc)
            }
        };
    }
    true
}

pub(crate) fn extract_bing_image_urls(html: &str, current: Option<&str>) -> Vec<String> {
    const MARKER: &str = "murl&quot;:&quot;";
    let mut remaining = html;
    let current = current.map(str::trim).filter(|value| !value.is_empty());
    let mut seen = HashSet::new();
    let mut urls = Vec::new();

    while let Some(start) = remaining.find(MARKER) {
        remaining = &remaining[start + MARKER.len()..];
        let Some(end) = remaining.find("&quot;") else {
            break;
        };
        let candidate = decode_html_entities(&remaining[..end]);
        remaining = &remaining[end + "&quot;".len()..];
        if current.is_some_and(|existing| existing == candidate)
            || !is_public_image_url(&candidate)
            || !seen.insert(candidate.clone())
        {
            continue;
        }
        urls.push(candidate);
    }
    urls
}

async fn open_food_facts_candidates(
    client: &reqwest::Client,
    barcode: &str,
    current: Option<&str>,
) -> Vec<String> {
    if !(8..=14).contains(&barcode.len()) || !barcode.bytes().all(|b| b.is_ascii_digit()) {
        return Vec::new();
    }
    let url = format!("https://world.openfoodfacts.org/api/v2/product/{barcode}");
    let response = match client
        .get(url)
        .query(&[("fields", "image_front_url,image_url,image_front_small_url")])
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => response,
        _ => return Vec::new(),
    };
    let Ok(body) = response.json::<OpenFoodFactsResponse>().await else {
        return Vec::new();
    };
    if body.status == Some(0) {
        return Vec::new();
    }
    let Some(product) = body.product else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    [
        product.image_front_url,
        product.image_url,
        product.image_front_small_url,
    ]
    .into_iter()
    .flatten()
    .filter(|url| {
        current.map_or(true, |existing| existing.trim() != url.trim())
            && is_public_image_url(url)
            && seen.insert(url.clone())
    })
    .collect()
}

pub async fn search_product_image(
    request: ProductImageSearchRequest,
) -> AppResult<ProductImageSearchResult> {
    // Product identity is deliberately strict: every external image lookup is
    // grounded by both the barcode and human-readable product name.
    let (product_name, barcode) =
        validate_search_identity(&request.product_name, request.barcode.as_deref())?;
    let query = build_search_query(
        &product_name,
        Some(&barcode),
        request.sku.as_deref(),
        request.category_name.as_deref(),
        request.mode,
    );
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(12))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) ZANPOS/2.0")
        .build()
        .map_err(|error| AppError::Internal(format!("image search client: {error}")))?;
    let current = request
        .current_image_url
        .as_deref()
        .filter(|value| value.starts_with("http://") || value.starts_with("https://"));

    if request.mode == ProductImageSearchMode::Fetch {
        let candidates = open_food_facts_candidates(&client, &barcode, current).await;
        if let Some(image_url) = candidates.first() {
            return Ok(ProductImageSearchResult {
                image_url: image_url.clone(),
                source: "Open Food Facts".into(),
                search_query: query,
                alternate_count: candidates.len().saturating_sub(1),
            });
        }
    }

    let response = client
        .get("https://www.bing.com/images/async")
        .query(&[
            ("q", query.as_str()),
            ("first", "0"),
            ("count", "20"),
            ("adlt", "on"),
            ("qft", ""),
        ])
        .send()
        .await
        .map_err(|error| AppError::Internal(format!("image search request failed: {error}")))?;
    if !response.status().is_success() {
        return Err(AppError::Internal(format!(
            "image search returned status {}",
            response.status()
        )));
    }
    let html = response
        .text()
        .await
        .map_err(|error| AppError::Internal(format!("image search response failed: {error}")))?;
    let candidates = extract_bing_image_urls(&html, current);
    let image_url = candidates
        .first()
        .cloned()
        .ok_or_else(|| AppError::NotFound("No suitable product image was found".into()))?;
    Ok(ProductImageSearchResult {
        image_url,
        source: "Bing Images".into(),
        search_query: query,
        alternate_count: candidates.len().saturating_sub(1),
    })
}
