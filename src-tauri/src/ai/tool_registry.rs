use crate::ai::{tool_validators, tools};
use crate::errors::{AppError, AppResult};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Read,
    Mutation,
}

impl ToolKind {
    pub fn is_mutation(self) -> bool {
        self == Self::Mutation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    Never,
    AutomaticIfActionUndo,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustLevel {
    Internal,
    External,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataScope {
    GlobalRead,
    BranchRead,
    BranchMutation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoPolicy {
    None,
    Action,
    Run,
}

fn undo_policy(name: &str) -> UndoPolicy {
    match name {
        "bulk_price_adjust" | "bulk_stock_set" => UndoPolicy::Run,
        "create_product"
        | "product_create"
        | "update_product_price"
        | "set_product_active"
        | "update_product_name"
        | "adjust_stock"
        | "stock_take"
        | "update_reorder_point"
        | "create_customer"
        | "update_customer"
        | "advance_delivery_status"
        | "bulk_stock_take"
        | "create_category"
        | "update_category"
        | "create_user"
        | "update_user"
        | "create_tax_rule"
        | "update_tax_rule"
        | "update_product_full"
        | "update_store_settings"
        | "confirm_delivery_payment"
        | "cancel_delivery"
        | "create_device"
        | "set_device_active"
        | "receive_stock"
        | "add_loyalty_points"
        | "bulk_update_prices" => UndoPolicy::Action,
        _ => UndoPolicy::None,
    }
}

fn routine_reversible_mutation(name: &str) -> bool {
    matches!(
        name,
        "update_product_price"
            | "update_product_name"
            | "set_product_active"
            | "create_category"
            | "update_category"
            | "create_customer"
            | "update_customer"
            | "create_supplier"
            | "update_supplier"
            | "add_product_barcode"
            | "remove_product_barcode"
            | "create_customer_note"
    )
}

#[derive(Debug, Clone)]
pub struct ToolDescriptor {
    pub name: String,
    pub kind: ToolKind,
    pub confirmation: Confirmation,
    pub feature_key: Option<&'static str>,
    #[allow(dead_code)]
    pub trust: TrustLevel,
    #[allow(dead_code)]
    pub scope: DataScope,
    #[allow(dead_code)]
    pub undo: UndoPolicy,
    schema: Value,
}

impl ToolDescriptor {
    pub fn validate(&self, input: &Value) -> AppResult<()> {
        tool_validators::validate_schema(input, &self.schema)
    }
}

pub struct ToolRegistry {
    ordered: Vec<ToolDescriptor>,
    by_name: HashMap<String, usize>,
}

static TOOL_REGISTRY: OnceLock<ToolRegistry> = OnceLock::new();

fn canonical_tool_name(name: &str) -> &str {
    match name {
        "product_create" => "create_product",
        _ => name,
    }
}

impl ToolRegistry {
    pub fn build() -> AppResult<Self> {
        let definitions = crate::ai::tools_catalogue::all_tool_definitions();
        Self::from_definitions(definitions)
    }

    pub fn global() -> AppResult<&'static Self> {
        if let Some(registry) = TOOL_REGISTRY.get() {
            return Ok(registry);
        }
        let validated = Self::build()?;
        Ok(TOOL_REGISTRY.get_or_init(|| validated))
    }

    fn from_definitions(definitions: Vec<crate::ai::client::ToolDef>) -> AppResult<Self> {
        let mut ordered = Vec::with_capacity(definitions.len());
        let mut by_name = HashMap::with_capacity(definitions.len());
        for definition in definitions {
            let bad = definition
                .name
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && !matches!(b, b'_' | b'-'));
            if definition.name.is_empty() || bad {
                return Err(AppError::Validation(format!(
                    "AI tool name contains unsupported characters: {}",
                    definition.name
                )));
            }
            if by_name.contains_key(&definition.name) {
                return Err(AppError::Validation(format!(
                    "Duplicate AI tool definition: {}",
                    definition.name
                )));
            }
            let kind = if tools::is_mutation_tool(&definition.name) {
                ToolKind::Mutation
            } else {
                ToolKind::Read
            };
            let descriptor = ToolDescriptor {
                feature_key: tools::feature_key_for_tool(&definition.name),
                confirmation: if kind.is_mutation()
                    && routine_reversible_mutation(&definition.name)
                    && undo_policy(&definition.name) == UndoPolicy::Action
                {
                    Confirmation::AutomaticIfActionUndo
                } else if kind.is_mutation() {
                    Confirmation::Always
                } else {
                    Confirmation::Never
                },
                name: definition.name.clone(),
                kind,
                trust: if crate::ai::tool_policy::is_external_content_tool(&definition.name) {
                    TrustLevel::External
                } else {
                    TrustLevel::Internal
                },
                scope: if kind.is_mutation() {
                    DataScope::BranchMutation
                } else {
                    DataScope::BranchRead
                },
                undo: if kind.is_mutation() {
                    undo_policy(&definition.name)
                } else {
                    UndoPolicy::None
                },
                schema: definition.input_schema,
            };
            by_name.insert(definition.name, ordered.len());
            ordered.push(descriptor);
        }
        for mutation in tools::MUTATION_TOOLS {
            if !by_name.contains_key(canonical_tool_name(mutation)) {
                return Err(AppError::Validation(format!(
                    "Mutation tool has no canonical definition: {mutation}"
                )));
            }
        }
        Ok(Self { ordered, by_name })
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.ordered.len()
    }

    pub fn get(&self, name: &str) -> Option<&ToolDescriptor> {
        self.by_name
            .get(canonical_tool_name(name))
            .map(|index| &self.ordered[*index])
    }

    #[cfg(test)]
    pub fn iter(&self) -> impl Iterator<Item = &ToolDescriptor> {
        self.ordered.iter()
    }
}

#[cfg(test)]
mod tests {
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
}
