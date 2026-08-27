//! The sources, and what they all have in common.
//!
//! Dispatch is an enum rather than `dyn Trait`. The design sketched a
//! `BahrainPriceSource` trait, and the intent behind it — that adding a store
//! should not mean rewriting the engine — is kept; but an async trait object
//! needs a boxing crate to exist at all, and the set of adapters is compiled in
//! either way. What actually makes a source cheap to add is that its *coverage*
//! lives in a `price_sources` row rather than in code, so an operator can retune
//! which store gets asked about what without a rebuild, and that most storefronts
//! need no new arm here at all: both live sources publish `schema.org/Product`,
//! and [`generic`] reads that from anyone who does.
//!
//! Every adapter does the same two-step, because neither source will hand over
//! a product in one request: find candidate listings, then read each one. The
//! finding differs — Akelny has no search endpoint and is discovered through its
//! sitemap, Bahrain Pharmacy has WooCommerce search — and the reading is shared.

pub mod akelny;
pub mod bahrain_pharmacy;
pub mod generic;

use crate::errors::AppResult;
use crate::price_intelligence::{jsonld, money, pack, SourceOffer, SourceProduct};

/// How many listings from one source are worth opening for a single lookup.
///
/// Each one is a request to somebody else's server. Three covers the case that
/// matters — the right product is rarely fourth in a name-ordered list — without
/// turning one operator's curiosity into a small crawl.
pub const MAX_CANDIDATES: usize = 3;

/// What we know about the product being priced, in the form a source can use.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub name: String,
}

impl SearchQuery {
    /// Words worth matching on. Sizes and packaging words are dropped: "500ml"
    /// appears in a third of the catalogue and matching on it ranks by
    /// packaging rather than by product.
    pub fn tokens(&self) -> Vec<String> {
        tokenize(&self.name)
    }
}

const NOISE: &[&str] = &[
    "the", "and", "with", "for", "pack", "packet", "bottle", "can", "tin", "box", "jar", "pcs",
    "pc", "piece", "pieces", "size", "new", "offer", "each", "per",
];

pub fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() > 2)
        // A bare number is a size, a year or a count; it ranks packaging.
        .filter(|word| !word.chars().all(|c| c.is_ascii_digit()))
        .filter(|word| !NOISE.contains(word))
        .map(|word| word.to_string())
        .collect()
}

/// Turn a page into a listing, or explain why it is not one.
///
/// Shared by every adapter, because the shape of a competitor's structured data
/// is the source's business and the shape of ours is not theirs to decide.
pub fn product_from_page(
    source_id: &'static str,
    url: &str,
    html: &str,
    key: Option<String>,
    pack_text: Option<String>,
) -> Option<SourceProduct> {
    let parsed = jsonld::product_from_html(html)?;

    let offers: Vec<SourceOffer> = parsed
        .offers
        .iter()
        .filter_map(|offer| {
            // A price that will not parse is dropped rather than guessed at.
            // One unreadable retailer is a gap; one invented number is a wrong
            // answer nobody downstream can distinguish from a right one.
            let price_minor = match &offer.currency {
                Some(currency) => money::parse_bhd(&format!("{} {}", offer.price_raw, currency)),
                None => money::parse_bhd(&offer.price_raw),
            }
            .ok()?;
            Some(SourceOffer {
                retailer: offer
                    .seller
                    .clone()
                    .unwrap_or_else(|| source_id.replace('_', " ")),
                price_minor,
                in_stock: offer.in_stock,
                url: offer.url.clone(),
            })
        })
        .collect();

    if offers.is_empty() {
        return None;
    }

    // Size can come from a dedicated line on the page or from the name. Both
    // are tried, because Akelny prints it separately and Bahrain Pharmacy
    // spells it inside the name.
    let pack = pack_text
        .as_deref()
        .and_then(pack::parse_pack)
        .or_else(|| pack::parse_pack(&parsed.name));

    Some(SourceProduct {
        source_id,
        key: key
            .or_else(|| parsed.sku.clone())
            .unwrap_or_else(|| url.to_string()),
        name: parsed.name,
        brand: parsed.brand,
        gtin: parsed.gtin,
        category: parsed.category,
        pack,
        pack_text,
        url: url.to_string(),
        offers,
    })
}

/// Which adapter answers for a registered source id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adapter {
    Akelny,
    BahrainPharmacy,
    /// A storefront that publishes structured data and needs no bespoke code.
    Generic,
    /// Registered so the UI can explain the gap, never fetched. LuLu answers
    /// 403 to every non-browser client including its own sitemap; a source that
    /// silently returned nothing would read as "nobody else sells this".
    UnsupportedDirectAccess,
}

impl Adapter {
    pub fn for_source(source_id: &str) -> Self {
        match source_id {
            "akelny" => Self::Akelny,
            "bahrain_pharmacy" => Self::BahrainPharmacy,
            "lulu_bh" => Self::UnsupportedDirectAccess,
            _ => Self::Generic,
        }
    }

    pub async fn search(
        &self,
        base_url: &str,
        query: &SearchQuery,
    ) -> AppResult<Vec<SourceProduct>> {
        match self {
            Self::Akelny => akelny::search(query).await,
            Self::BahrainPharmacy => bahrain_pharmacy::search(query).await,
            Self::Generic => generic::search(base_url, query).await,
            Self::UnsupportedDirectAccess => Ok(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests;
