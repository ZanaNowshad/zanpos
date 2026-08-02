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
}

pub struct ActionRegistry {
    actions: HashMap<String, ActionDefinition>,
}

impl ActionRegistry {
    pub fn load() -> Self {
        let mut reg = Self {
            actions: HashMap::new(),
        };

        for def in all_tool_definitions() {
            let is_mutation = classify_mutation(&def.name);
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
                confirmation: ConfirmationPolicy::Required,
                has_undo: has_undo(&def.name),
            };
            reg.actions.insert(def.name.clone(), action);
        }

        reg
    }

    pub fn get(&self, name: &str) -> Option<&ActionDefinition> {
        self.actions.get(name)
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
    pub fn generate_provider_schemas(&self, provider: &str) -> Vec<Value> {
        self.all()
            .map(|def| match provider {
                "openai" => json!({
                    "type": "function",
                    "function": {
                        "name": def.name,
                        "description": def.description,
                        "parameters": {
                            "type": "object",
                            "properties": {},
                            "required": []
                        }
                    }
                }),
                "anthropic" => json!({
                    "name": def.name,
                    "description": def.description,
                    "input_schema": {
                        "type": "object",
                        "properties": {},
                        "required": []
                    }
                }),
                _ => json!({ "name": def.name, "description": def.description }),
            })
            .collect()
    }
}

fn classify_mutation(name: &str) -> bool {
    let mutation_verbs = [
        "create_", "update_", "delete_", "set_", "adjust_", "void_", "cancel_", "confirm_",
        "receive_", "bulk_", "add_", "remove_", "reset_", "dismiss_", "advance_", "sync_",
    ];
    mutation_verbs
        .iter()
        .any(|v| name.starts_with(v) || name.contains(&format!("_{v}")))
        || name.contains("_update")
        || name.contains("_delete")
        || name.contains("_create")
}

fn has_undo(name: &str) -> bool {
    let reversible = [
        "update_product_price",
        "update_product_name",
        "set_product_active",
        "create_product",
        "create_category",
        "bulk_update_prices",
        "bulk_stock_take",
        "adjust_stock",
        "stock_take",
    ];
    reversible.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_loads_all_catalogue_tools() {
        let reg = ActionRegistry::load();
        let catalogue_count = all_tool_definitions().len();
        assert_eq!(
            reg.len(),
            catalogue_count,
            "Registry must contain every tool from the catalogue"
        );
    }

    #[test]
    fn mutations_require_confirmation() {
        let reg = ActionRegistry::load();
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
    fn reads_outnumber_mutations() {
        let reg = ActionRegistry::load();
        let reads = reg.reads().count();
        let mutations = reg.mutations().count();
        assert!(
            reads > mutations,
            "Reads ({reads}) should outnumber mutations ({mutations})"
        );
    }
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    fn generates_openai_schemas_for_all_tools() {
        let reg = ActionRegistry::load();
        let schemas = reg.generate_provider_schemas("openai");
        assert_eq!(schemas.len(), reg.len());
        for s in &schemas {
            assert!(s["type"] == "function");
            assert!(s["function"]["name"].is_string());
        }
    }

    #[test]
    fn generates_anthropic_schemas_for_all_tools() {
        let reg = ActionRegistry::load();
        let schemas = reg.generate_provider_schemas("anthropic");
        assert_eq!(schemas.len(), reg.len());
        for s in &schemas {
            assert!(s["name"].is_string());
            assert!(s["input_schema"].is_object());
        }
    }
}
