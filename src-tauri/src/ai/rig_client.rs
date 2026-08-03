/// Rig provider pilot — read-only OpenAI-compatible LLM client.
///
/// Feature-gated behind `rig-pilot`. Read-only: no tool execution, no DB access.
/// The legacy provider adapter remains selectable for rollback.
#[cfg(feature = "rig-pilot")]
mod inner {
    use rig::{agent::AgentBuilder, completion::Prompt, providers::openai};

    #[allow(dead_code)]
    pub struct RigClient {
        api_key: String,
        model_name: String,
    }

    #[allow(dead_code)]
    impl RigClient {
        pub fn new(api_key: String, model_name: String) -> Result<Self, String> {
            // Validate that the API key can initialize an OpenAI client.
            let _ = openai::Client::new(&api_key);
            Ok(Self {
                api_key,
                model_name,
            })
        }

        /// Read-only chat completion via Rig agent.
        /// No mutations, no tool execution, no database access.
        pub async fn chat(&self, prompt: &str) -> Result<String, String> {
            let client = openai::Client::new(&self.api_key);
            let model = client.completion_model(&self.model_name);
            let agent = AgentBuilder::new(model).build();
            agent
                .prompt(prompt)
                .await
                .map_err(|e| format!("Rig completion failed: {e}"))
        }
    }
}

#[allow(unused_imports)]
#[cfg(feature = "rig-pilot")]
pub use inner::RigClient;

#[cfg(not(feature = "rig-pilot"))]
pub struct RigClient;

#[cfg(not(feature = "rig-pilot"))]
impl RigClient {
    pub fn new(_api_key: String, _model_name: String) -> Result<Self, String> {
        Err("Rig pilot not compiled in — enable the rig-pilot feature".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "rig-pilot")]
    fn rig_client_accepts_valid_key() {
        let result = RigClient::new("sk-test123".into(), "gpt-4o".into());
        assert!(result.is_ok());
    }

    #[test]
    #[cfg(not(feature = "rig-pilot"))]
    fn rig_client_fails_gracefully_without_feature() {
        let result = RigClient::new("sk-test".into(), "gpt-4o".into());
        assert_eq!(
            result.err().unwrap(),
            "Rig pilot not compiled in — enable the rig-pilot feature"
        );
    }
}
