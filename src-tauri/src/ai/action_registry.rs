/// ZANPOS Action Registry — single source of truth for all AI tools.
///
/// Every tool is represented by exactly one ActionDefinition containing name,
/// JSON Schema, read/mutation classification, required role, preview, execute,
/// audit, undo, timeout, and idempotency policy. Provider tool schemas and
/// prompt catalogues are generated from this registry.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum ActionKind {
    Read,
    Mutation,
}

#[derive(Debug, Clone)]
pub struct ActionDefinition {
    pub name: String,
    pub description: String,
    pub kind: ActionKind,
    pub required_role: String,
    pub timeout_seconds: u64,
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
        self.actions.values().filter(|a| matches!(a.kind, ActionKind::Mutation))
    }

    pub fn reads(&self) -> impl Iterator<Item = &ActionDefinition> {
        self.actions.values().filter(|a| matches!(a.kind, ActionKind::Read))
    }
}
