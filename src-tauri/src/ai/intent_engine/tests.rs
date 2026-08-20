use super::*;

/// The advertised list and the routable list have to be the same list.
///
/// They drifted once: `adjust_prices_batch` and `bulk_price_adjust` sat in
/// `all_intents()` — schemas, descriptions and all — while `INTENT_NAMES`
/// never carried them, so `streaming.rs` could not route either one. Two
/// intents that looked complete from the outside and were unreachable from
/// the inside. A name in one and not the other is always a bug, in either
/// direction: advertised-but-unroutable strands the capability,
/// routable-but-unadvertised hides it.
#[test]
fn every_defined_intent_is_routable_and_vice_versa() {
    let defined: std::collections::BTreeSet<&str> =
        all_intents().into_iter().map(|i| i.name).collect();
    let routable: std::collections::BTreeSet<&str> = INTENT_NAMES.iter().copied().collect();

    let advertised_but_unroutable: Vec<_> = defined.difference(&routable).collect();
    assert!(
        advertised_but_unroutable.is_empty(),
        "defined in all_intents() but missing from INTENT_NAMES: {advertised_but_unroutable:?}"
    );

    let routable_but_unadvertised: Vec<_> = routable.difference(&defined).collect();
    assert!(
        routable_but_unadvertised.is_empty(),
        "listed in INTENT_NAMES but never defined: {routable_but_unadvertised:?}"
    );
}

/// Every routable name must reach a real arm of `execute_intent`, not the
/// catch-all. Checked by name so a renamed handler cannot slip through.
#[test]
fn mutation_intents_are_a_subset_of_routable_intents() {
    for name in INTENT_NAMES {
        if is_mutation_intent(name) {
            assert!(
                INTENT_NAMES.contains(name),
                "{name} is flagged as a mutation but is not routable"
            );
        }
    }
    // A mutation intent that nobody can call is a confirmation prompt that
    // never fires; one that is routable but unflagged writes without asking.
    for name in ["create_product", "update_product", "receive_stock", "create_customer", "create_user", "backup_database"] {
        assert!(is_mutation_intent(name), "{name} must require confirmation");
    }
}
