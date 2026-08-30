//! Scoring decides what is worth a person's attention, never what is true.

use super::*;
use crate::price_intelligence::pack::parse_pack;
use crate::price_intelligence::{SourceOffer, SourceProduct};

fn listing(name: &str, pack_text: Option<&str>, brand: Option<&str>) -> SourceProduct {
    SourceProduct {
        source_id: "akelny",
        key: "k1".into(),
        name: name.into(),
        brand: brand.map(str::to_string),
        gtin: None,
        category: None,
        pack: pack_text.and_then(parse_pack),
        pack_text: pack_text.map(str::to_string),
        url: "https://example.test/p".into(),
        offers: vec![SourceOffer {
            retailer: "Al Osra".into(),
            price_minor: 250,
            in_stock: true,
            url: None,
        }],
    }
}

#[test]
fn only_confirmed_and_barcode_matches_are_trusted() {
    assert!(MatchMethod::BarcodeExact.is_trusted());
    assert!(MatchMethod::OperatorConfirmed.is_trusted());
    assert!(!MatchMethod::FuzzyCandidate.is_trusted());
}

/// The SQL filter is the enforcement point, so its shape is pinned. A caller
/// cannot forget a WHERE clause it never writes, but only while this fragment
/// still names both halves of the rule.
#[test]
fn the_trusted_filter_covers_method_and_status() {
    assert!(TRUSTED_MATCH_SQL.contains("BARCODE_EXACT"));
    assert!(TRUSTED_MATCH_SQL.contains("OPERATOR_CONFIRMED"));
    assert!(!TRUSTED_MATCH_SQL.contains("FUZZY_CANDIDATE"));
    // Status matters as much as method: a confirmed match whose pack changed is
    // no longer evidence.
    assert!(TRUSTED_MATCH_SQL.contains("status = 'active'"));
}

#[test]
fn a_matching_name_scores_and_an_unrelated_one_does_not() {
    let ours = "Rainbow Evaporated Milk 160ml";
    assert!(
        score(
            ours,
            None,
            &listing("Rainbow Evaporated Milk 160ml", None, None)
        ) >= MIN_CONFIDENCE
    );
    assert_eq!(
        score(ours, None, &listing("Dettol Handwash 500ml", None, None)),
        0
    );
}

/// Pack size is part of product identity. Four bars and two bars are different
/// products at different prices and the names do not distinguish them.
#[test]
fn a_disagreeing_pack_size_is_penalised_hard() {
    let ours = "Dove Beauty Bar";
    let same = score(
        ours,
        parse_pack("4 x 90g").as_ref(),
        &listing("Dove Beauty Bar", Some("4 x 90g"), None),
    );
    let different = score(
        ours,
        parse_pack("4 x 90g").as_ref(),
        &listing("Dove Beauty Bar", Some("2 x 90g"), None),
    );

    assert!(same > different, "same={same} different={different}");
    // Disqualified outright, not merely ranked lower: the names are identical,
    // so anything short of zero still offers the wrong product.
    assert_eq!(different, 0, "a different pack size was still offered");
}

/// One side silent is no evidence either way — most sources print a size only
/// sometimes, and treating absence as disagreement would hide real matches.
#[test]
fn a_missing_pack_size_neither_helps_nor_hurts() {
    let ours = "Rainbow Evaporated Milk";
    let with = score(
        ours,
        parse_pack("160ml").as_ref(),
        &listing("Rainbow Evaporated Milk", None, None),
    );
    let without = score(ours, None, &listing("Rainbow Evaporated Milk", None, None));
    assert_eq!(with, without);
}

#[test]
fn ranking_drops_the_implausible_and_orders_the_rest() {
    let ranked = rank(
        "Rainbow Evaporated Milk 160ml",
        None,
        vec![
            listing("Dettol Handwash", None, None),
            listing("Rainbow Evaporated Milk 160ml", None, Some("Rainbow")),
            listing("Rainbow Milk", None, None),
        ],
    );

    assert_eq!(ranked.len(), 2, "an unrelated product was offered");
    assert!(ranked[0].confidence >= ranked[1].confidence);
    assert!(ranked[0].name.contains("Evaporated"));
}

/// A candidate carries its offers so confirming it can record a price
/// immediately — but it is still only a candidate until somebody says so.
#[test]
fn a_candidate_carries_its_offers_without_being_trusted() {
    let ranked = rank(
        "Rainbow Evaporated Milk",
        None,
        vec![listing("Rainbow Evaporated Milk", None, None)],
    );
    assert_eq!(ranked[0].offers.len(), 1);
    assert_eq!(ranked[0].offers[0].price_minor, 250);
}

// ── The tier that needs no human ─────────────────────────────────────────────

fn with_gtin(gtin: &str) -> SourceProduct {
    let mut listing = listing("Anything At All", None, None);
    listing.gtin = Some(gtin.into());
    listing
}

#[test]
fn a_shared_barcode_settles_it() {
    let ours = vec!["6291001234567".to_string()];
    assert!(barcode_match(&ours, &with_gtin("6291001234567")));
}

/// Sources print GTINs with spaces and hyphens, and a leading zero is dropped
/// as often as it is kept. None of that makes it a different product.
#[test]
fn punctuation_and_a_leading_zero_do_not_break_a_barcode_match() {
    let ours = vec!["06291001234567".to_string()];
    for printed in ["6291001234567", "629-100-1234567", "6291 0012 34567"] {
        assert!(barcode_match(&ours, &with_gtin(printed)), "{printed}");
    }
}

#[test]
fn a_different_barcode_is_not_a_match() {
    let ours = vec!["6291001234567".to_string()];
    assert!(!barcode_match(&ours, &with_gtin("6291009999999")));
}

/// Most Bahrain listings carry no barcode at all. Absence is not agreement.
#[test]
fn a_listing_without_a_barcode_never_matches_on_one() {
    let ours = vec!["6291001234567".to_string()];
    assert!(!barcode_match(
        &ours,
        &listing("Anything At All", None, None)
    ));
    assert!(!barcode_match(&[], &with_gtin("6291001234567")));
}

/// A short string is a stock code or a fragment, not a GTIN. Treating one as a
/// global identifier is how two unrelated products become "the same item".
#[test]
fn something_too_short_to_be_a_gtin_is_rejected() {
    assert!(!barcode_match(&["1234".to_string()], &with_gtin("1234")));
}
