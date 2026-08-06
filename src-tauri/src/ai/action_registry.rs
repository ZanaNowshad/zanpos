use crate::ai::tool_registry::{Confirmation, ToolKind, ToolRegistry, UndoPolicy};
/// ZANPOS Action Registry — single source of truth for all AI tools.
///
/// Populated from the authoritative `tools_catalogue::all_tool_definitions()`.
/// Provider tool schemas and prompt catalogues are generated from this registry.
use crate::ai::tools_catalogue::all_tool_definitions;
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActionKind {
    Read,
    Mutation,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfirmationPolicy {
    Required,
    RiskBased,
    Automatic,
}

#[derive(Debug, Clone)]
pub struct ActionDefinition {
    pub name: String,
    pub description: String,
    pub kind: ActionKind,
    pub required_role: String,
    pub timeout_seconds: u64,
    pub confirmation: ConfirmationPolicy,
    pub has_undo: bool,
    pub input_schema: Value,
}

pub struct ActionRegistry {
    actions: HashMap<String, ActionDefinition>,
}

impl ActionRegistry {
    pub fn load() -> Result<Self, String> {
        let policy = ToolRegistry::build().map_err(|error| error.to_string())?;
        let mut reg = Self {
            actions: HashMap::new(),
        };

        for def in all_tool_definitions() {
            let descriptor = policy
                .get(&def.name)
                .ok_or_else(|| format!("AI action has no authoritative policy: {}", def.name))?;
            let is_mutation = descriptor.kind == ToolKind::Mutation;
            let action = ActionDefinition {
                name: def.name.clone(),
                description: def.description.clone(),
                kind: if is_mutation {
                    ActionKind::Mutation
                } else {
                    ActionKind::Read
                },
                required_role: if is_mutation {
                    "manager".into()
                } else {
                    "cashier".into()
                },
                timeout_seconds: 30,
                confirmation: match descriptor.confirmation {
                    Confirmation::Always => ConfirmationPolicy::Required,
                    Confirmation::AutomaticIfActionUndo => ConfirmationPolicy::RiskBased,
                    Confirmation::Never => ConfirmationPolicy::Automatic,
                },
                has_undo: descriptor.undo != UndoPolicy::None,
                input_schema: def.input_schema.clone(),
            };
            reg.actions.insert(def.name.clone(), action);
        }

        Ok(reg)
    }

    pub fn get(&self, name: &str) -> Option<&ActionDefinition> {
        self.actions.get(name)
    }

    pub fn require(&self, name: &str) -> Result<&ActionDefinition, String> {
        self.get(name)
            .ok_or_else(|| format!("AI action has no authoritative policy: {name}"))
    }

    pub fn mutations(&self) -> impl Iterator<Item = &ActionDefinition> {
        self.actions
            .values()
            .filter(|a| matches!(a.kind, ActionKind::Mutation))
    }

    pub fn reads(&self) -> impl Iterator<Item = &ActionDefinition> {
        self.actions
            .values()
            .filter(|a| matches!(a.kind, ActionKind::Read))
    }

    pub fn all(&self) -> impl Iterator<Item = &ActionDefinition> {
        self.actions.values()
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Generate provider-facing JSON Schema tool definitions for all registered actions.
    /// Preserves the full input_schema properties from the authoritative tool definitions.
    pub fn generate_provider_schemas(&self, provider: &str) -> Vec<Value> {
        self.all()
            .map(|def| {
                let properties = def
                    .input_schema
                    .get("properties")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let required = def
                    .input_schema
                    .get("required")
                    .cloned()
                    .unwrap_or_else(|| json!([]));
                match provider {
                    "openai" => json!({
                        "type": "function",
                        "function": {
                            "name": def.name,
                            "description": def.description,
                            "parameters": {
                                "type": "object",
                                "properties": properties,
                                "required": required
                            }
                        }
                    }),
                    "anthropic" => json!({
                        "name": def.name,
                        "description": def.description,
                        "input_schema": {
                            "type": "object",
                            "properties": properties,
                            "required": required
                        }
                    }),
                    _ => json!({ "name": def.name, "description": def.description }),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_loads_all_catalogue_tools() {
        let reg = ActionRegistry::load().expect("authoritative registry");
        let catalogue_count = all_tool_definitions().len();
        assert_eq!(
            reg.len(),
            catalogue_count,
            "Registry must contain every tool from the catalogue"
        );
    }

    #[test]
    fn mutations_require_confirmation() {
        let reg = ActionRegistry::load().expect("authoritative registry");
        for mutation in reg.mutations() {
            assert!(
                matches!(
                    mutation.confirmation,
                    ConfirmationPolicy::Required | ConfirmationPolicy::RiskBased
                ),
                "Mutation {} must require confirmation",
                mutation.name
            );
        }
    }

    #[test]
    fn registry_matches_authoritative_tool_kinds() {
        let reg = ActionRegistry::load().expect("authoritative registry");
        for definition in all_tool_definitions() {
            let action = reg
                .get(&definition.name)
                .expect("catalogue action is registered");
            let expected = if crate::ai::tools::is_mutation_tool(&definition.name) {
                ActionKind::Mutation
            } else {
                ActionKind::Read
            };
            assert_eq!(
                action.kind, expected,
                "wrong policy for {}",
                definition.name
            );
        }
    }

    #[test]
    fn named_non_prefix_mutations_require_manager_confirmation() {
        let reg = ActionRegistry::load().expect("authoritative registry");
        for name in [
            "open_shift",
            "force_full_resync",
            "register_device",
            "open_cash_drawer",
            "reindex_database",
            "force_close_shift",
        ] {
            let action = reg.get(name).expect("named mutation is registered");
            assert_eq!(
                action.kind,
                ActionKind::Mutation,
                "{name} must be a mutation"
            );
            assert_eq!(
                action.required_role, "manager",
                "{name} must not allow cashier authority"
            );
            assert_eq!(
                action.confirmation,
                ConfirmationPolicy::Required,
                "{name} must require confirmation"
            );
        }
    }

    #[test]
    fn unknown_actions_fail_closed() {
        let reg = ActionRegistry::load().expect("authoritative registry");
        assert!(reg.require("future_unclassified_action").is_err());
    }

    #[test]
    fn registry_covers_production_mutation_list() {
        let reg = ActionRegistry::load().expect("authoritative registry");
        let missing: Vec<_> = crate::ai::tools::MUTATION_TOOLS
            .iter()
            .filter(|name| reg.get(name).is_none())
            .collect();
        // These are internal aliases not exposed as provider-facing tools.
        // They are safe to be absent from the ActionRegistry.
        let known_internal = &[
            "product_create",
            "customer_create",
            "customer_update",
            "customer_delete",
            "supplier_create",
            "supplier_update",
            "supplier_delete",
        ];
        let unexpected: Vec<_> = missing.iter().filter(|n| !known_internal.contains(n)).collect();
        assert!(
            unexpected.is_empty(),
            "MUTATION_TOOLS entries missing from ActionRegistry (not known internals): {:?}",
            unexpected
        );
    }

    #[test]
    fn registry_mutations_are_in_production_list() {
        let reg = ActionRegistry::load().expect("authoritative registry");
        for action in reg.mutations() {
            assert!(
                crate::ai::tools::MUTATION_TOOLS.contains(&action.name.as_str()),
                "ActionRegistry mutation {} missing from MUTATION_TOOLS",
                action.name
            );
        }
    }
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    fn generates_openai_schemas_for_all_tools() {
        let reg = ActionRegistry::load().expect("authoritative registry");
        let schemas = reg.generate_provider_schemas("openai");
        assert_eq!(schemas.len(), reg.len());
        for s in &schemas {
            assert!(s["type"] == "function");
            assert!(s["function"]["name"].is_string());
        }
    }

    #[test]
    fn generates_anthropic_schemas_for_all_tools() {
        let reg = ActionRegistry::load().expect("authoritative registry");
        let schemas = reg.generate_provider_schemas("anthropic");
        assert_eq!(schemas.len(), reg.len());
        for s in &schemas {
            assert!(s["name"].is_string());
            assert!(s["input_schema"].is_object());
        }
    }

    #[test]
    fn schemas_preserve_input_properties() {
        let reg = ActionRegistry::load().expect("authoritative registry");
        let schemas = reg.generate_provider_schemas("openai");

        let search = schemas
            .iter()
            .find(|s| s["function"]["name"] == "search_products")
            .expect("search_products in openai schemas");
        let props = &search["function"]["parameters"]["properties"];
        assert!(
            props["query"].is_object(),
            "search_products must have query property"
        );
        assert_eq!(props["query"]["type"], "string");

        let required = &search["function"]["parameters"]["required"];
        assert!(required.as_array().unwrap().contains(&json!("query")));

        let anthropic = reg.generate_provider_schemas("anthropic");
        let anthro = anthropic
            .iter()
            .find(|s| s["name"] == "search_products")
            .expect("search_products in anthropic schemas");
        assert!(anthro["input_schema"]["properties"]["query"].is_object());
    }
}
