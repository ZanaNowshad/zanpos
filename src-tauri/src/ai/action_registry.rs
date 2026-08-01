use crate::ai::action_registry::ActionRegistry;
use serde_json::{json, Value};

impl ActionRegistry {
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
