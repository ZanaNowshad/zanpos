use super::*;

#[test]
fn the_shapes_the_two_live_sources_actually_publish_all_parse() {
    // Akelny's structured data: a bare three-decimal string.
    assert_eq!(parse_bhd("1.300").unwrap(), 1300);
    // Akelny's rendered page.
    assert_eq!(parse_bhd("0.550 BD").unwrap(), 550);
    // Bahrain Pharmacy's priceSpecification.
    assert_eq!(parse_bhd("1.375").unwrap(), 1375);
    // Their category listing.
    assert_eq!(parse_bhd("15.614 BHD").unwrap(), 15614);
    // Al Hawaj, for when that adapter lands — four figures with a separator.
    assert_eq!(parse_bhd("BHD 1,234.500").unwrap(), 1_234_500);
}

#[test]
fn short_decimals_mean_what_a_shelf_label_means() {
    assert_eq!(parse_bhd("1").unwrap(), 1000);
    assert_eq!(parse_bhd("1.5").unwrap(), 1500);
    assert_eq!(parse_bhd("1.30").unwrap(), 1300);
    assert_eq!(parse_bhd("0.075").unwrap(), 75);
    assert_eq!(parse_bhd(".5").unwrap(), 500);
}

/// The whole reason this rejects rather than rounds: a fourth decimal means we
/// are reading a unit price, another currency, or the wrong element entirely.
/// Rounding would file a plausible wrong number in the price history, and
/// nothing downstream could tell it from a real observation ever again.
#[test]
fn a_fourth_decimal_is_refused_rather_than_rounded() {
    let error = parse_bhd("1.3755").unwrap_err().to_string();
    assert!(error.contains("more than 3 decimals"), "{error}");
    assert!(parse_bhd("0.4999").is_err());
}

#[test]
fn a_price_in_another_currency_is_refused_not_read_as_dinars() {
    // Quoting a riyal as a dinar understates a competitor about tenfold.
    for raw in ["12.50 SAR", "AED 40.00", "$3.99", "12.500 KWD"] {
        assert!(parse_bhd(raw).is_err(), "accepted {raw}");
    }
}

#[test]
fn junk_and_impossible_values_never_become_a_price() {
    for raw in [
        "",
        "   ",
        "BHD",
        "-1.500",
        "0.000",
        "1.2.3",
        "1O.500",
        "999999.999",
    ] {
        assert!(parse_bhd(raw).is_err(), "accepted {raw:?}");
    }
}

#[test]
fn formatting_round_trips_through_the_parser() {
    for minor in [1, 75, 550, 1300, 1375, 15614, 42500, 1_234_500] {
        assert_eq!(parse_bhd(&format_bhd(minor)).unwrap(), minor, "{minor}");
    }
    assert_eq!(format_bhd(1300), "1.300");
    assert_eq!(format_bhd(75), "0.075");
}

/// A single clearance price on a discontinued line drags a mean below every
/// shelf in the country. A manager pricing against that mean undercuts a sale
/// that already ended.
#[test]
fn the_middle_is_a_median_so_one_clearance_price_cannot_move_it() {
    assert_eq!(median_minor(&[1300, 1325, 1400]), Some(1325));
    assert_eq!(median_minor(&[250, 1300, 1325, 1400]), Some(1312));
    assert_eq!(median_minor(&[1300]), Some(1300));
    assert_eq!(median_minor(&[]), None);

    let with_clearance = [1300, 1325, 1350, 1400, 100];
    assert_eq!(median_minor(&with_clearance), Some(1325));
}

/// An even count lands between two observations. Rounding down keeps a
/// suggestion at or below a price actually seen on a shelf.
#[test]
fn an_even_count_rounds_toward_the_cheaper_observation() {
    assert_eq!(median_minor(&[1000, 1001]), Some(1000));
}
