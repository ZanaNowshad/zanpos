use super::*;

/// Captured pages, not hand-written markup. An adapter tested against invented
/// HTML only proves the test author's idea of what the source publishes.
const AKELNY_PRODUCT: &str = include_str!("../../../tests/fixtures/price/akelny_product.html");
const BP_PRODUCT: &str = include_str!("../../../tests/fixtures/price/bp_product.html");
const BP_SEARCH: &str = include_str!("../../../tests/fixtures/price/bp_search.html");

/// Akelny is an aggregator, so one page carries several retailers. Descending
/// into the AggregateOffer rather than taking its lowPrice summary is what
/// keeps each price attached to the shop charging it.
#[test]
fn an_aggregator_page_yields_one_offer_per_retailer() {
    let product = product_from_html(AKELNY_PRODUCT).expect("no Product node");

    assert_eq!(product.name, "Almarai UHT Premium Strawberry Milk");
    assert_eq!(
        product.sku.as_deref(),
        Some("f4bf784f-1089-4a77-bd71-30a7c719a18c")
    );
    assert_eq!(product.category.as_deref(), Some("Dairy & Eggs"));
    assert_eq!(product.offers.len(), 2);

    let helli = &product.offers[0];
    assert_eq!(helli.seller.as_deref(), Some("Al Helli"));
    assert_eq!(helli.price_raw, "1.300");
    assert_eq!(helli.currency.as_deref(), Some("BHD"));
    assert!(helli.in_stock);

    let tamimi = &product.offers[1];
    assert_eq!(tamimi.seller.as_deref(), Some("Tamimi Markets"));
    assert_eq!(tamimi.price_raw, "1.325");
}

/// The finding that changed the trust model: Bahrain Pharmacy publishes a real
/// barcode. That is what turns a match against our own 29,678 canonical GTINs
/// into an exact one instead of a name guess an operator has to vouch for.
#[test]
fn a_shop_page_yields_its_barcode_brand_and_stock_state() {
    let product = product_from_html(BP_PRODUCT).expect("no Product node");

    assert_eq!(product.name, "VASELINE LIP CARE MINT LIP BALM 4.8 G");
    assert_eq!(product.gtin.as_deref(), Some("8801619053256"));
    assert_eq!(product.brand.as_deref(), Some("VASELINE"));
    assert_eq!(product.sku.as_deref(), Some("41441"));
    assert_eq!(product.offers.len(), 1);

    let offer = &product.offers[0];
    // Price is nested in a priceSpecification here, not on the Offer itself.
    assert_eq!(offer.price_raw, "1.275");
    assert_eq!(offer.currency.as_deref(), Some("BHD"));
    assert_eq!(
        offer.seller.as_deref(),
        Some("Bahrain Pharmacy Online Store")
    );
    // The page says OutOfStock, and an out-of-stock price is not a shelf price.
    assert!(!offer.in_stock);
}

/// This page's only structured node is a CollectionPage whose `@type` is a
/// list. Reading a Product out of it would attach prices to a search result.
#[test]
fn a_search_page_with_no_product_node_yields_nothing() {
    assert!(product_from_html(BP_SEARCH).is_none());
    assert!(
        !script_blocks(BP_SEARCH).is_empty(),
        "fixture lost its ld+json"
    );
}

#[test]
fn a_product_wrapped_in_a_graph_is_still_found() {
    // The WordPress SEO shape: everything under @graph, Product last.
    let html = r#"<script type="application/ld+json">
      {"@context":"https://schema.org/","@graph":[
        {"@type":"WebPage","name":"page"},
        {"@type":"Product","name":"Dettol 500ml","offers":{"@type":"Offer","price":"1.900","priceCurrency":"BHD"}}
      ]}</script>"#;
    let product = product_from_html(html).unwrap();
    assert_eq!(product.name, "Dettol 500ml");
    assert_eq!(product.offers[0].price_raw, "1.900");
}

#[test]
fn a_bare_numeric_price_is_read_as_readily_as_a_string() {
    let html = r#"<script type="application/ld+json">
      {"@type":"Product","name":"X","offers":{"@type":"Offer","price":2.5,"priceCurrency":"BHD"}}</script>"#;
    assert_eq!(product_from_html(html).unwrap().offers[0].price_raw, "2.5");
}

/// Most shop pages only mention availability when the answer is no. Dropping
/// every silent offer would empty the comparison on half the sources.
#[test]
fn an_offer_that_says_nothing_about_stock_counts_as_in_stock() {
    let html = r#"<script type="application/ld+json">
      {"@type":"Product","name":"X","offers":{"@type":"Offer","price":"1.000"}}</script>"#;
    assert!(product_from_html(html).unwrap().offers[0].in_stock);
}

#[test]
fn a_barcode_that_is_not_a_barcode_is_dropped() {
    for bad in [
        "\"N/A\"",
        "\"123\"",
        "\"88016190532560000\"",
        "\"88A1619053256\"",
    ] {
        let html = format!(
            r#"<script type="application/ld+json">{{"@type":"Product","name":"X","gtin":{bad},"offers":{{"@type":"Offer","price":"1.000"}}}}</script>"#
        );
        assert!(
            product_from_html(&html).unwrap().gtin.is_none(),
            "kept {bad}"
        );
    }
}

#[test]
fn non_structured_script_tags_are_ignored() {
    let html = r#"<script>var ld = {"@type":"Product","name":"Not structured data"};</script>
      <script type="application/json">{"@type":"Product","name":"Also not"}</script>"#;
    assert!(script_blocks(html).is_empty());
    assert!(product_from_html(html).is_none());
}

#[test]
fn malformed_json_does_not_stop_a_later_valid_block() {
    let html = r#"<script type="application/ld+json">{ this is not json </script>
      <script type="application/ld+json">{"@type":"Product","name":"Second","offers":{"@type":"Offer","price":"0.500"}}</script>"#;
    assert_eq!(product_from_html(html).unwrap().name, "Second");
}
