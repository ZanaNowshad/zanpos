use super::*;

fn pack(text: &str) -> PackSize {
    parse_pack(text).unwrap_or_else(|| panic!("no size read from {text:?}"))
}

/// Every string here was copied off a live Akelny or Bahrain Pharmacy page.
#[test]
fn the_size_strings_the_live_sources_actually_publish() {
    // Akelny's dedicated size line.
    let milk = pack("6x200ml");
    assert_eq!(milk.dimension, Dimension::Volume);
    assert_eq!(milk.multiplier, 6);
    assert_eq!(milk.each_milli, 200_000);
    assert_eq!(milk.total_milli(), 1_200_000);
    assert_eq!(milk.describe(), "6x200ml");

    // Bahrain Pharmacy states size inside the product name.
    let balm = pack("VASELINE LIP CARE MINT LIP BALM 4.8 G");
    assert_eq!(balm.dimension, Dimension::Mass);
    assert_eq!(balm.each_milli, 4_800);
    assert_eq!(balm.multiplier, 1);

    assert_eq!(pack("PONDS TALC D/F 300GM").each_milli, 300_000);
    assert_eq!(pack("VASELINE HAND CREAM + ANTI-BAC 75 ML").each_milli, 75_000);
    assert_eq!(pack("JOHNSON COSMETIC PADS 80 S").dimension, Dimension::Count);
    assert_eq!(pack("VICHY NORMADERM DEEP CLEANSING GEL 200 ML").each_milli, 200_000);
}

/// `0.10OZ 3 G` is one tube labelled twice. The metric figure is the declared
/// one on everything sold here, so reading the ounces would compare a 3g balm
/// against a 2.8g one and call them different products.
#[test]
fn a_name_carrying_both_units_is_read_as_the_metric_one() {
    let both = pack("VASELINE LIP CARE KISSING RED 0.10OZ 3 G");
    assert_eq!(both.dimension, Dimension::Mass);
    assert_eq!(both.each_milli, 3_000);

    let ounces_only = pack("Some Import 0.10OZ");
    assert_eq!(ounces_only.dimension, Dimension::Mass);
    assert_eq!(ounces_only.each_milli, 2_834);
}

#[test]
fn decimal_amounts_are_exact_rather_than_nearly_right() {
    assert_eq!(pack("4.8 G").each_milli, 4_800);
    assert_eq!(pack("1.5 L").each_milli, 1_500_000);
    assert_eq!(pack("0.5kg").each_milli, 500_000);
}

/// 8.995 per kilo and 1.250 per punnet are both real prices for the same food,
/// and neither says anything about the other. Comparing them is the single
/// easiest way to report a competitor as wildly cheaper than they are.
#[test]
fn loose_goods_are_never_comparable_to_a_fixed_pack() {
    let loose = pack("Smoked Pepperoni [Per Kg]");
    assert!(loose.loose);
    assert_eq!(loose.dimension, Dimension::Mass);
    assert_eq!(loose.describe(), "per kg");

    let fixed = pack("Pepperoni 250g");
    assert!(!loose.comparable_to(&fixed));
    assert!(!fixed.comparable_to(&loose));
    // Two per-kilo items compare fine: the rate is the price.
    assert!(loose.comparable_to(&pack("Beef Mince [Per Kg]")));
}

#[test]
fn sizes_in_different_dimensions_never_compare() {
    assert!(!pack("500g").comparable_to(&pack("500ml")));
    assert!(!pack("80 S").comparable_to(&pack("80g")));
}

/// Wide enough for an ounce/gram rounding difference on the same tube, far too
/// narrow to let a smaller carton pass as a larger one.
#[test]
fn the_tolerance_absorbs_unit_conversion_but_not_a_different_carton() {
    assert!(pack("3 G").comparable_to(&pack("0.10OZ")));
    assert!(!pack("200ml").comparable_to(&pack("250ml")));
    assert!(!pack("1L").comparable_to(&pack("1.5L")));
    // Same total by a different route is still the same amount of product.
    assert!(pack("6x200ml").comparable_to(&pack("1.2L")));
}

#[test]
fn a_multiplier_is_only_read_when_it_leads_the_measurement() {
    assert_eq!(pack("4 x 90g").multiplier, 4);
    assert_eq!(pack("12X1L").multiplier, 12);
    assert_eq!(pack("4 \u{d7} 90g").multiplier, 4);
    // The x in a brand or a trailing "x 2 pack" must not become a multiplier.
    assert_eq!(pack("Xtra Bleach 500ml").multiplier, 1);
    assert_eq!(pack("Dettol 500ml").multiplier, 1);
}

/// Refusing is the right answer. A size that cannot be read leaves the match
/// name-only, which the trust model already refuses to trust unattended.
#[test]
fn text_with_no_size_returns_nothing_rather_than_guessing() {
    for text in ["", "Coca-Cola", "Fresh Bread", "Assorted Chocolates", "Item 5"] {
        assert!(parse_pack(text).is_none(), "invented a size for {text:?}");
    }
}
