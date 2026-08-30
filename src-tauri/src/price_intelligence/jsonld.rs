//! Reading a competitor's own structured description of a product.
//!
//! The plan assumed this layer would be CSS selectors over hand-written
//! adapters. It turned out both live sources publish `schema.org/Product` as
//! JSON-LD, which is a far better thing to depend on: it is the data the site
//! maintains deliberately for search engines, so it survives the redesigns that
//! break selectors, and it carries fields the rendered page does not — Bahrain
//! Pharmacy puts a real `gtin` there, which is what lets a match against our own
//! barcodes be exact rather than a guess.
//!
//! That also means no HTML parser and no new dependency. The scan below finds
//! the script blocks; `serde_json` does the rest.
//!
//! The two shapes seen in the wild differ enough to matter:
//!
//! ```text
//! Akelny            offers: AggregateOffer { offers: [ Offer { price, seller } ] }
//! Bahrain Pharmacy  offers: [ Offer { priceSpecification: [ { price } ], seller } ]
//! ```
//!
//! so price is looked for in both places rather than one.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonLdOffer {
    /// Who is actually selling at this price. On an aggregator this is the
    /// retailer; on a shop's own site it is the shop.
    pub seller: Option<String>,
    pub price_raw: String,
    pub currency: Option<String>,
    pub in_stock: bool,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonLdProduct {
    pub name: String,
    /// Whatever the source calls this product forever: Akelny's UUID, Bahrain
    /// Pharmacy's numeric id. Stored as the match key so a confirmed pairing
    /// survives the product being renamed.
    pub sku: Option<String>,
    pub gtin: Option<String>,
    pub brand: Option<String>,
    pub category: Option<String>,
    pub url: Option<String>,
    pub offers: Vec<JsonLdOffer>,
}

/// Pull every `application/ld+json` block out of a page.
///
/// Hand-rolled rather than regex or a DOM: the shape is fixed, and this keeps
/// the dependency list where it is on a release profile that is tuned for size.
pub fn script_blocks(html: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<script") {
        let after = &rest[start..];
        let Some(open_end) = after.find('>') else {
            break;
        };
        let attributes = &after[..open_end];
        let Some(close) = after[open_end..].find("</script") else {
            break;
        };
        let body = &after[open_end + 1..open_end + close];
        if attributes.contains("application/ld+json") {
            blocks.push(body.trim());
        }
        rest = &after[open_end + close..];
    }
    blocks
}

/// The first `Product` node on the page, wherever the source nests it.
pub fn product_from_html(html: &str) -> Option<JsonLdProduct> {
    script_blocks(html)
        .into_iter()
        .filter_map(|block| serde_json::from_str::<Value>(block).ok())
        .find_map(|value| find_product(&value))
}

fn find_product(value: &Value) -> Option<JsonLdProduct> {
    match value {
        // A page may ship one node, a bare array, or the `@graph` wrapper that
        // WordPress SEO plugins emit. All three occur across these sources.
        Value::Array(items) => items.iter().find_map(find_product),
        Value::Object(object) => {
            if let Some(graph) = object.get("@graph") {
                if let Some(found) = find_product(graph) {
                    return Some(found);
                }
            }
            has_type(object.get("@type"), "Product").then(|| read_product(object))?
        }
        _ => None,
    }
}

/// `@type` is sometimes a string and sometimes a list — Bahrain Pharmacy's
/// search page uses `["CollectionPage","SearchResultsPage"]`.
fn has_type(value: Option<&Value>, wanted: &str) -> bool {
    match value {
        Some(Value::String(text)) => text.eq_ignore_ascii_case(wanted),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .any(|text| text.eq_ignore_ascii_case(wanted)),
        _ => false,
    }
}

fn read_product(object: &serde_json::Map<String, Value>) -> Option<JsonLdProduct> {
    let name = text(object.get("name"))?;
    Some(JsonLdProduct {
        name,
        sku: text(object.get("sku")),
        gtin: first_gtin(object),
        brand: brand_name(object.get("brand")),
        category: text(object.get("category")),
        url: text(object.get("url")),
        offers: read_offers(object.get("offers")),
    })
}

/// schema.org spells the barcode five ways depending on its length.
fn first_gtin(object: &serde_json::Map<String, Value>) -> Option<String> {
    ["gtin", "gtin13", "gtin14", "gtin12", "gtin8"]
        .iter()
        .find_map(|key| text(object.get(*key)))
        .filter(|value| {
            value.chars().all(|c| c.is_ascii_digit()) && (8..=14).contains(&value.len())
        })
}

fn brand_name(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.trim().to_string()),
        Value::Array(items) => items.iter().find_map(|item| brand_name(Some(item))),
        Value::Object(object) => text(object.get("name")),
        _ => None,
    }
}

fn read_offers(value: Option<&Value>) -> Vec<JsonLdOffer> {
    let mut offers = Vec::new();
    collect_offers(value, &mut offers);
    offers
}

fn collect_offers(value: Option<&Value>, out: &mut Vec<JsonLdOffer>) {
    match value {
        Some(Value::Array(items)) => {
            for item in items {
                collect_offers(Some(item), out);
            }
        }
        Some(Value::Object(object)) => {
            // An AggregateOffer's own lowPrice/highPrice are a summary of the
            // list inside it. Descending into the list keeps each retailer's
            // price attached to that retailer, which is the whole point.
            if let Some(nested) = object.get("offers") {
                collect_offers(Some(nested), out);
                return;
            }
            if let Some(offer) = read_offer(object) {
                out.push(offer);
            }
        }
        _ => {}
    }
}

fn read_offer(object: &serde_json::Map<String, Value>) -> Option<JsonLdOffer> {
    let (price_raw, currency) = offer_price(object)?;
    Some(JsonLdOffer {
        seller: object.get("seller").and_then(|seller| match seller {
            Value::String(name) => Some(name.trim().to_string()),
            Value::Object(map) => text(map.get("name")),
            _ => None,
        }),
        price_raw,
        currency,
        // Absent availability is treated as in stock: most shop pages only say
        // so when the answer is no, and dropping every silent offer would empty
        // the comparison.
        in_stock: match text(object.get("availability")) {
            None => true,
            Some(value) => !value.to_lowercase().contains("outofstock"),
        },
        url: text(object.get("url")),
    })
}

/// Price sits directly on the Offer, or inside a `priceSpecification` that is
/// itself either an object or a list. Both live sources use a different one.
fn offer_price(object: &serde_json::Map<String, Value>) -> Option<(String, Option<String>)> {
    if let Some(price) = number_or_text(object.get("price")) {
        return Some((price, text(object.get("priceCurrency"))));
    }
    let specification = object.get("priceSpecification")?;
    let candidates: Vec<&Value> = match specification {
        Value::Array(items) => items.iter().collect(),
        other => vec![other],
    };
    candidates.into_iter().find_map(|candidate| {
        let map = candidate.as_object()?;
        let price = number_or_text(map.get("price"))?;
        Some((price, text(map.get("priceCurrency"))))
    })
}

fn text(value: Option<&Value>) -> Option<String> {
    let trimmed = value?.as_str()?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Prices arrive as `"1.300"` from both sources today, but schema.org permits a
/// bare number and a source is free to switch without warning.
fn number_or_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
