use super::*;

fn names(routes: &[Route]) -> Vec<&str> {
    routes.iter().map(|route| route.name).collect()
}

fn registered() -> Vec<(String, Vec<String>)> {
    vec![
        (
            "akelny".into(),
            ["food", "beverages", "household", "baby", "general"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        ),
        (
            "bahrain_pharmacy".into(),
            ["cosmetics", "personal_care", "baby", "health", "household"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        ),
    ]
}

/// The worked example from the design: a body cream routes to personal care,
/// and must not spend a request on an electronics retailer.
#[test]
fn a_body_cream_routes_to_personal_care_and_never_to_electronics() {
    let routes = routes_for("Dove Intensive Cream 150ml", None);
    assert_eq!(routes[0].name, "personal_care");
    assert!(!names(&routes).contains(&"electronics"));

    let sources = registered();
    let chosen = sources_for(&routes, &sources);
    assert_eq!(chosen.first(), Some(&"bahrain_pharmacy"));
    // Akelny still gets asked — it carries household and general goods.
    assert!(chosen.contains(&"akelny"));
}

#[test]
fn a_phone_routes_to_electronics_alone() {
    let routes = routes_for("Samsung Galaxy S26 Ultra 256GB", None);
    assert_eq!(routes[0].name, "electronics");
    assert!(!names(&routes).contains(&"food"));

    // No v1 source covers electronics, so the router says so rather than
    // pretending a grocery aggregator might know.
    let sources = registered();
    let chosen = sources_for(&routes, &sources);
    assert_eq!(
        chosen,
        vec!["akelny"],
        "only the general fallback should match"
    );
}

/// A product filed under nothing still has to route, or the feature only works
/// once somebody has tidied 28,000 rows.
#[test]
fn a_product_with_no_category_still_reaches_a_source() {
    for name in ["Assorted Item", "", "Local Speciality"] {
        let routes = routes_for(name, None);
        assert_eq!(routes.last().unwrap().name, "general", "{name:?}");
        let sources = registered();
        assert!(!sources_for(&routes, &sources).is_empty(), "{name:?}");
    }
}

/// The shop put it there deliberately; a word in a name is weaker evidence.
/// "Cream" is in both a face cream and a carton of single cream.
#[test]
fn the_shops_own_category_outweighs_a_word_in_the_name() {
    let dairy = routes_for("Almarai Fresh Cream 200ml", Some("Dairy & Eggs"));
    assert_eq!(dairy[0].name, "food");

    let beauty = routes_for("Nivea Cream 200ml", Some("Beauty & Personal Care"));
    assert_eq!(beauty[0].name, "personal_care");
}

#[test]
fn arabic_names_route_as_readily_as_english() {
    assert_eq!(routes_for("حليب المراعي كامل الدسم", None)[0].name, "food");
    assert_eq!(
        routes_for("شامبو هيد اند شولدرز", None)[0].name,
        "personal_care"
    );
}

#[test]
fn routes_come_back_strongest_first() {
    let routes = routes_for("Johnson Baby Shampoo 200ml", Some("Baby & Kids"));
    let scores: Vec<u8> = routes.iter().map(|route| route.score).collect();
    assert!(
        scores.windows(2).all(|pair| pair[0] >= pair[1]),
        "{scores:?}"
    );
    assert!(names(&routes).contains(&"baby"));
}

/// One incidental word should not drag a product into a whole extra source.
#[test]
fn a_single_weak_signal_does_not_open_an_unrelated_route() {
    let routes = routes_for("Samsung Galaxy S26 Ultra 256GB", None);
    let electronics = routes.iter().find(|r| r.name == "electronics").unwrap();
    assert!(electronics.score >= 60, "{}", electronics.score);
    // "general" is the floor and is expected; nothing else weak survives.
    assert_eq!(routes.len(), 2, "{:?}", names(&routes));
}
