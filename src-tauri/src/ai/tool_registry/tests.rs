//! Tests for `tool_registry`.
//!
//! Split out for size only — the parent was 564 lines against a 500-line ship-gate rule.
use super::*;

#[test]
fn registry_covers_every_provider_definition_once() {
    let registry = ToolRegistry::build().unwrap();
    let definitions = crate::ai::tools_catalogue::all_tool_definitions();
    assert_eq!(registry.len(), definitions.len());
    for definition in definitions {
        assert!(
            registry.get(&definition.name).is_some(),
            "{}",
            definition.name
        );
    }
}

#[test]
fn global_registry_is_built_once_but_fresh_validation_remains_available() {
    let first = ToolRegistry::global().unwrap();
    let second = ToolRegistry::global().unwrap();

    assert!(std::ptr::eq(first, second));
    assert_eq!(ToolRegistry::build().unwrap().len(), first.len());
}

#[test]
fn registry_rejects_injected_duplicate_definitions() {
    let mut definitions = crate::ai::tools_catalogue::all_tool_definitions();
    definitions.push(definitions[0].clone());
    assert!(ToolRegistry::from_definitions(definitions).is_err());
}

#[test]
fn internal_product_create_alias_resolves_without_provider_advertisement() {
    let registry = ToolRegistry::build().unwrap();
    assert_eq!(
        registry.get("product_create").unwrap().name,
        "create_product"
    );
    assert_eq!(
        crate::ai::tools_catalogue::all_tool_definitions()
            .iter()
            .filter(|definition| {
                definition.name == "create_product" || definition.name == "product_create"
            })
            .count(),
        1
    );
}

#[test]
fn bulk_stock_set_is_a_confirmed_run_mutation() {
    let registry = ToolRegistry::build().unwrap();
    let descriptor = registry.get("bulk_stock_set").unwrap();

    assert_eq!(descriptor.kind, ToolKind::Mutation);
    assert_eq!(descriptor.confirmation, Confirmation::Always);
    assert_eq!(descriptor.undo, UndoPolicy::Run);
}

#[test]
fn every_provider_tool_name_uses_supported_characters() {
    for (index, definition) in crate::ai::tools_catalogue::all_tool_definitions()
        .into_iter()
        .enumerate()
    {
        assert!(
            !definition.name.is_empty()
                && definition
                    .name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')),
            "tools[{index}].function.name is invalid: {}",
            definition.name
        );
    }
}

#[test]
fn routine_reversible_mutations_use_risk_based_confirmation() {
    let registry = ToolRegistry::build().unwrap();
    let name = "update_product_price";
    let descriptor = registry.get(name).unwrap();
    assert_eq!(
        descriptor.confirmation,
        Confirmation::AutomaticIfActionUndo,
        "{name}"
    );
    assert_eq!(descriptor.undo, UndoPolicy::Action, "{name}");
    assert_eq!(descriptor.scope, DataScope::BranchMutation);
}

#[test]
fn sensitive_and_unknown_undo_mutations_still_require_confirmation() {
    let registry = ToolRegistry::build().unwrap();
    for name in [
        "delete_product",
        "create_refund",
        "adjust_stock",
        "bulk_update_prices",
        "create_user",
        "backup_database",
        "send_whatsapp_message",
    ] {
        if let Some(descriptor) = registry.get(name) {
            assert_eq!(descriptor.confirmation, Confirmation::Always, "{name}");
        }
    }
}

#[test]
fn registry_exposes_tool_centre_permission_and_risk_metadata() {
    let registry = ToolRegistry::build().unwrap();

    let barcode = registry.get("lookup_barcode").unwrap();
    assert_eq!(barcode.required_role, RequiredRole::Cashier);
    assert_eq!(barcode.risk, RiskLevel::Medium);
    assert!(!barcode.description.trim().is_empty());

    let price = registry.get("update_product_price").unwrap();
    assert_eq!(price.required_role, RequiredRole::Manager);
    assert_eq!(price.risk, RiskLevel::Medium);
    assert_eq!(price.confirmation, Confirmation::AutomaticIfActionUndo);

    let outbound = registry.get("send_whatsapp_to_customer").unwrap();
    assert_eq!(outbound.required_role, RequiredRole::Manager);
    assert_eq!(outbound.risk, RiskLevel::Critical);
    assert_eq!(outbound.confirmation, Confirmation::Always);
}

#[test]
fn no_undo_mutation_is_never_automatic() {
    let registry = ToolRegistry::build().unwrap();
    for descriptor in registry.iter().filter(|d| d.kind.is_mutation()) {
        if descriptor.undo != UndoPolicy::Action {
            assert_ne!(
                descriptor.confirmation,
                Confirmation::AutomaticIfActionUndo,
                "{}",
                descriptor.name
            );
        }
    }
}

#[test]
fn irreversible_mutations_do_not_claim_undo() {
    let registry = ToolRegistry::build().unwrap();
    for name in [
        "open_cash_drawer",
        "send_whatsapp_message",
        "backup_database",
    ] {
        if let Some(descriptor) = registry.get(name) {
            assert_eq!(descriptor.undo, UndoPolicy::None, "{name}");
        }
    }
    assert_eq!(
        registry.get("bulk_price_adjust").unwrap().undo,
        UndoPolicy::Run
    );
}

#[test]
fn network_descriptors_are_explicitly_untrusted() {
    let registry = ToolRegistry::build().unwrap();
    for name in [
        "web_search",
        "fetch_url",
        "smart_barcode_lookup",
        "lookup_barcode",
        "get_exchange_rates",
        "get_prayer_times",
        "get_bahrain_holidays",
    ] {
        assert_eq!(
            registry.get(name).unwrap().trust,
            TrustLevel::External,
            "{name}"
        );
    }
}

#[test]
fn schema_validation_rejects_unknown_and_oversized_input() {
    let registry = ToolRegistry::build().unwrap();
    let search = registry.get("search_products").unwrap();
    assert!(search
        .validate(&serde_json::json!({"query":"tea", "extra":true}))
        .is_err());
    assert!(search
        .validate(&serde_json::json!({"query":"x".repeat(4097)}))
        .is_err());
    assert!(search.validate(&serde_json::json!({"query":"tea"})).is_ok());
}

#[test]
fn list_categories_accepts_an_optional_query_filter() {
    let registry = ToolRegistry::build().unwrap();
    let categories = registry.get("list_categories").unwrap();

    assert!(categories.validate(&serde_json::json!({})).is_ok());
    assert!(categories
        .validate(&serde_json::json!({"query":"drinks"}))
        .is_ok());
    assert!(categories
        .validate(&serde_json::json!({"query":"drinks", "extra":true}))
        .is_err());
}
