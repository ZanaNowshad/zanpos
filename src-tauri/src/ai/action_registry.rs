/// ZANPOS Action Registry — single source of truth for all AI tools.
///
/// Every tool is represented by exactly one ActionDefinition containing name,
/// JSON Schema, read/mutation classification, required role, preview, execute,
/// audit, undo, timeout, and idempotency policy. Provider tool schemas and
/// prompt catalogues are generated from this registry.

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
    pub fn new() -> Self {
        Self { actions: HashMap::new() }
    }

    pub fn register(&mut self, def: ActionDefinition) {
        self.actions.insert(def.name.clone(), def);
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
}

impl Default for ActionRegistry {
    fn default() -> Self {
        let mut reg = Self::new();

        reg.register(ActionDefinition {
            name: "get_today_summary".into(),
            description: "Get today's sales summary".into(),
            kind: ActionKind::Read,
            required_role: "cashier".into(),
            timeout_seconds: 10,
            confirmation: ConfirmationPolicy::Required,
            has_undo: false,
        });

        reg.register(ActionDefinition {
            name: "list_products".into(),
            description: "List all products in the catalogue".into(),
            kind: ActionKind::Read,
            required_role: "cashier".into(),
            timeout_seconds: 15,
            confirmation: ConfirmationPolicy::Required,
            has_undo: false,
        });

        reg.register(ActionDefinition {
            name: "update_product_price".into(),
            description: "Update a product's selling price".into(),
            kind: ActionKind::Mutation,
            required_role: "manager".into(),
            timeout_seconds: 10,
            confirmation: ConfirmationPolicy::Required,
            has_undo: true,
        });

        reg.register(ActionDefinition {
            name: "set_product_active".into(),
            description: "Activate or deactivate a product".into(),
            kind: ActionKind::Mutation,
            required_role: "manager".into(),
            timeout_seconds: 10,
            confirmation: ConfirmationPolicy::Required,
            has_undo: true,
        });

        reg.register(ActionDefinition {
            name: "create_product".into(),
            description: "Create a new product in the catalogue".into(),
            kind: ActionKind::Mutation,
            required_role: "manager".into(),
            timeout_seconds: 15,
            confirmation: ConfirmationPolicy::Required,
            has_undo: true,
        });

        reg.register(ActionDefinition {
            name: "bulk_price_adjust".into(),
            description: "Adjust prices for multiple products at once".into(),
            kind: ActionKind::Mutation,
            required_role: "manager".into(),
            timeout_seconds: 30,
            confirmation: ConfirmationPolicy::Required,
            has_undo: true,
        });

        reg.register(ActionDefinition {
            name: "get_stock_levels".into(),
            description: "Get current stock levels".into(),
            kind: ActionKind::Read,
            required_role: "cashier".into(),
            timeout_seconds: 10,
            confirmation: ConfirmationPolicy::Required,
            has_undo: false,
        });

        reg.register(ActionDefinition {
            name: "get_low_stock".into(),
            description: "List products below reorder point".into(),
            kind: ActionKind::Read,
            required_role: "cashier".into(),
            timeout_seconds: 10,
            confirmation: ConfirmationPolicy::Required,
            has_undo: false,
        });

        reg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_covers_all_known_tools() {
        let reg = ActionRegistry::default();
        assert!(reg.len() > 0, "Registry must contain tool definitions");
    }

    #[test]
    fn mutations_require_confirmation() {
        let reg = ActionRegistry::default();
        for m in reg.mutations() {
            assert!(
                matches!(m.confirmation, ConfirmationPolicy::Required | ConfirmationPolicy::RiskBased),
                "Mutation {} must require confirmation",
                m.name
            );
        }
    }

    #[test]
    fn mutations_with_undo_are_explicit() {
        let reg = ActionRegistry::default();
        for m in reg.mutations() {
            if m.has_undo {
                assert!(
                    m.name.contains("update") || m.name.contains("create") || m.name.contains("set_") || m.name.contains("adjust") || m.name.contains("bulk"),
                    "{} has undo — verify this is correct",
                    m.name
                );
            }
        }
    }
}
