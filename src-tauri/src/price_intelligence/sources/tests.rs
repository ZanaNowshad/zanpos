use super::*;
use crate::price_intelligence::pack::Dimension;

const AKELNY_PRODUCT: &str = include_str!("../../../tests/fixtures/price/akelny_product.html");
const AKELNY_SITEMAP: &str = include_str!("../../../tests/fixtures/price/akelny_sitemap.xml");
const BP_PRODUCT: &str = include_str!("../../../tests/fixtures/price/bp_product.html");
const BP_SEARCH: &str = include_str!("../../../tests/fixtures/price/bp_search.html");

fn query(name: &str) -> SearchQuery {
    SearchQuery { name: name.into() }
}

// ── Reading a page into a listing ────────────────────────────────────────────

/// One Akelny page is six shops' worth of prices, which is the whole reason
/// this source is worth more than any single storefront.
#[test]
fn an_akelny_page_becomes_one_listing_carrying_every_retailer() {
    let pack_text = akelny::pack_text_from_page(AKELNY_PRODUCT);
    assert_eq!(pack_text.as_deref(), Some("6x200ml"));

    let product = product_from_page(
        "akelny",
        "https://akelny.net/bh/products/almarai-uht-premium-strawberry-milk",
        AKELNY_PRODUCT,
        Some("almarai-uht-premium-strawberry-milk".into()),
        pack_text,
    )
    .expect("no listing built");

    assert_eq!(product.name, "Almarai UHT Premium Strawberry Milk");
    assert_eq!(product.category.as_deref(), Some("Dairy & Eggs"));
    assert_eq!(product.offers.len(), 2);
    assert_eq!(product.offers[0].retailer, "Al Helli");
    assert_eq!(product.offers[0].price_minor, 1300);
    assert_eq!(product.offers[1].retailer, "Tamimi Markets");
    assert_eq!(product.offers[1].price_minor, 1325);

    let size = product.pack.expect("no size read");
    assert_eq!(size.dimension, Dimension::Volume);
    assert_eq!(size.multiplier, 6);
    assert_eq!(size.total_milli(), 1_200_000);
}

/// The finding that made `BARCODE_EXACT` a live path rather than a theoretical
/// one: this source publishes a real barcode, so a match against our own
/// catalogue needs nobody's confirmation.
#[test]
fn a_bahrain_pharmacy_page_carries_the_barcode_that_makes_a_match_exact() {
    let product = product_from_page(
        "bahrain_pharmacy",
        "https://bahrainpharmacy.com/store/product/vaseline-lip-care-mint-lip-balm-4-8-g/",
        BP_PRODUCT,
        Some("vaseline-lip-care-mint-lip-balm-4-8-g".into()),
        None,
    )
    .expect("no listing built");

    assert_eq!(product.gtin.as_deref(), Some("8801619053256"));
    assert_eq!(product.brand.as_deref(), Some("VASELINE"));
    assert_eq!(product.offers[0].price_minor, 1275);
    // Size is stated inside the name here, and is still read.
    assert_eq!(product.pack.unwrap().each_milli, 4_800);
    // The page says out of stock, and that has to survive to the caller: a
    // price nobody can go and pay is not a price to compare against.
    assert!(!product.offers[0].in_stock);
    assert_eq!(product.sellable_offers().count(), 0);
}

/// A page with no price is not a listing. Keeping it would put a retailer in
/// the comparison with nothing to compare.
#[test]
fn a_page_with_no_readable_price_yields_no_listing() {
    let html = r#"<script type="application/ld+json">
        {"@type":"Product","name":"Mystery","offers":{"@type":"Offer","price":"ask in store"}}</script>"#;
    assert!(product_from_page("generic", "https://x.test/p/1", html, None, None).is_none());
    assert!(
        product_from_page("generic", "https://x.test/p/1", "<html></html>", None, None).is_none()
    );
}

/// One unreadable retailer on a multi-retailer page is a gap; one invented
/// number is a wrong answer nothing downstream can distinguish from a right one.
#[test]
fn an_unparseable_retailer_is_dropped_without_losing_the_others() {
    let html = r#"<script type="application/ld+json">{"@type":"Product","name":"X","offers":{
        "@type":"AggregateOffer","offers":[
          {"@type":"Offer","price":"1.300","priceCurrency":"BHD","seller":{"name":"Good"}},
          {"@type":"Offer","price":"12.5000","priceCurrency":"BHD","seller":{"name":"Fourth decimal"}},
          {"@type":"Offer","price":"9.990","priceCurrency":"SAR","seller":{"name":"Wrong currency"}}
        ]}}</script>"#;
    let product = product_from_page("akelny", "https://x.test/p", html, None, None).unwrap();

    assert_eq!(product.offers.len(), 1);
    assert_eq!(product.offers[0].retailer, "Good");
}

// ── Discovery ────────────────────────────────────────────────────────────────

#[test]
fn the_sitemap_yields_product_slugs_and_nothing_else() {
    let slugs = akelny::slugs_from_sitemap(AKELNY_SITEMAP);

    assert_eq!(slugs.len(), 200, "fixture is the trimmed 200-entry copy");
    assert!(slugs.contains(&"sprite-carbonated-soft-drink-glass-bottle-250ml".to_string()));
    assert!(slugs.iter().all(|slug| !slug.contains('/')));
}

/// Akelny has no search endpoint and its `/api` is robots-disallowed, so the
/// name is matched against slugs locally. That costs the site one sitemap a day
/// instead of a request per lookup.
#[test]
fn a_product_name_finds_its_slug_without_a_search_endpoint() {
    let slugs = akelny::slugs_from_sitemap(AKELNY_SITEMAP);
    let ranked = akelny::rank_slugs(&slugs, &tokenize("Almarai Processed Cream Cheese"));

    assert!(
        ranked
            .iter()
            .any(|slug| slug.starts_with("almarai-processed-cream-cheese")),
        "{ranked:?}"
    );
    assert!(ranked.len() <= MAX_CANDIDATES);
}

/// Half the words of the name have to appear, or "Almarai Fresh Milk" matches
/// every Almarai listing on the site and three arbitrary ones get opened.
#[test]
fn a_single_shared_word_is_not_a_match() {
    let slugs = vec![
        "heinz-tomato-ketchup-00fec4".to_string(),
        "orange-navel-500g".to_string(),
    ];
    assert!(akelny::rank_slugs(&slugs, &tokenize("Heinz Tomato Ketchup")).len() == 1);
    // "Orange" alone against a two-word name is one hit out of two — the
    // threshold's edge, and deliberately not enough on its own elsewhere.
    assert!(akelny::rank_slugs(&slugs, &tokenize("Samsung Galaxy Ultra Phone")).is_empty());
}

#[test]
fn woocommerce_results_yield_each_product_once() {
    let slugs = bahrain_pharmacy::slugs_from_results(BP_SEARCH);

    assert_eq!(slugs.len(), 16, "{slugs:?}");
    assert_eq!(slugs[0], "vaseline-hand-cream-anti-bac-75-ml");
    // Each card links its product three times; the list must not.
    let mut sorted = slugs.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), slugs.len());
}

#[test]
fn the_generic_adapter_keeps_only_same_host_product_links() {
    let html = r#"
        <a href="https://shop.test/product/one">one</a>
        <a href="https://shop.test/product/one">one again</a>
        <a href="https://shop.test/category/drinks">a category</a>
        <a href="https://facebook.com/product/spam">another host</a>
        <a href="/product/relative">relative</a>"#;
    let links = generic::candidate_links(html, "shop.test");

    assert_eq!(links, vec!["https://shop.test/product/one"]);
}

/// Without a search template there is no way in, and guessing at `/search?q=`
/// means knocking on doors that were never there.
#[tokio::test]
async fn a_generic_source_with_no_search_template_says_so() {
    let error = generic::search("https://shop.test", &query("Dettol"))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains(generic::QUERY_PLACEHOLDER), "{error}");
}

// ── Routing to an adapter ────────────────────────────────────────────────────

/// A source that silently returned nothing would read as "nobody else sells
/// this". LuLu has to be visibly unsupported instead.
#[tokio::test]
async fn the_unsupported_source_never_fetches_and_never_errors() {
    let adapter = Adapter::for_source("lulu_bh");
    assert_eq!(adapter, Adapter::UnsupportedDirectAccess);
    assert!(adapter
        .search("https://gcc.luluhypermarket.com/en-bh", &query("Milk"))
        .await
        .unwrap()
        .is_empty());
}

#[test]
fn an_unknown_source_falls_back_to_the_structured_data_adapter() {
    assert_eq!(Adapter::for_source("akelny"), Adapter::Akelny);
    assert_eq!(
        Adapter::for_source("bahrain_pharmacy"),
        Adapter::BahrainPharmacy
    );
    assert_eq!(Adapter::for_source("some_new_shop"), Adapter::Generic);
}

// ── Tokenising ───────────────────────────────────────────────────────────────

/// "500ml" appears in a third of the catalogue. Ranking on it ranks packaging.
#[test]
fn sizes_and_filler_words_are_not_matched_on() {
    let tokens = tokenize("Almarai Fresh Milk 500ml with the Bottle Pack");

    assert!(tokens.contains(&"almarai".to_string()));
    assert!(tokens.contains(&"fresh".to_string()));
    assert!(!tokens.contains(&"the".to_string()));
    assert!(!tokens.contains(&"pack".to_string()));
    assert!(!tokens.contains(&"bottle".to_string()));
}
