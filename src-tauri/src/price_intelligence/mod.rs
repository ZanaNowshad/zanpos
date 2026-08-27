//! Bahrain competitor prices, for a shop deciding what to charge.
//!
//! ZANPOS already advertised three market-price tools to ZanAI. None of them
//! returned a price: each built a web-search query and handed back the snippets,
//! and the tool result said so outright. A manager asking what LuLu charges got
//! a list of links.
//!
//! What replaces it reads a competitor's own structured product data, verifies
//! it describes the same product, converts it to fils, and records the
//! observation so a price history builds up over weeks. The rules it is built
//! around, in order of how much trouble breaking them would cause:
//!
//! **A competitor's price never becomes ours.** Nothing here writes to
//! `product_prices`. The panel fills the price box in the product form and the
//! operator saves through `update_product_price`, with the RBAC, confirmation
//! and audit trail that already exist. The existing provenance rules enforce the
//! same thing for the AI: `is_external_content_tool` lists these tools, so a
//! request that has read an external price has had every mutation tool stripped
//! from it before it could use one.
//!
//! **An unverified match affects nothing.** Sources name products; they mostly
//! do not carry barcodes. A name-and-size guess is shown as a candidate and
//! feeds no total, no median and no suggestion until an operator says it is the
//! right product — after which the pairing is remembered and every later refresh
//! is exact. See [`matching`].
//!
//! **We are a guest on someone else's server.** One shop's pricing decisions do
//! not justify crawling a small site's whole catalogue. Only products an
//! operator asked about or put on the watchlist are ever fetched, one at a time,
//! at a pace that backs off the moment a source objects. See [`http`].

pub mod http;
pub mod jsonld;
pub mod matching;
pub mod observe;
pub mod money;
pub mod pack;
pub mod route;
pub mod service;
pub mod sources;

use pack::PackSize;

/// One retailer's price for one product, as read from a source.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceOffer {
    /// The shop charging this. On an aggregator that is a third party — "Al
    /// Helli" read from Akelny — which is why it is stored per offer rather
    /// than taken from the source.
    pub retailer: String,
    pub price_minor: i64,
    pub in_stock: bool,
    pub url: Option<String>,
}

/// A product as one source describes it, with every price it lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceProduct {
    pub source_id: &'static str,
    /// Stable for the life of the listing — Akelny's UUID, Bahrain Pharmacy's
    /// numeric id, a slug where a source offers nothing better. This is what a
    /// confirmed match is stored against, so the pairing survives a rename.
    pub key: String,
    pub name: String,
    pub brand: Option<String>,
    pub gtin: Option<String>,
    pub category: Option<String>,
    pub pack: Option<PackSize>,
    /// What the source actually printed, kept verbatim. A confirmed match is
    /// suspended when this changes, because a pack going from 4x90g to 2x90g is
    /// a different product wearing the same name.
    pub pack_text: Option<String>,
    pub url: String,
    pub offers: Vec<SourceOffer>,
}

impl SourceProduct {
    /// Offers worth quoting. An out-of-stock listing keeps its last price on the
    /// page long after the shelf has changed, so it is not a price anyone can
    /// go and pay today.
    pub fn sellable_offers(&self) -> impl Iterator<Item = &SourceOffer> {
        self.offers.iter().filter(|offer| offer.in_stock)
    }
}
