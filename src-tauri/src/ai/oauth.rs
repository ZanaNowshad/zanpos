//! OAuth provider authentication for AI services.
//!
//! Supported providers:
//!   - OpenAI OAuth2 (ChatGPT) — PKCE flow, device code, or client credentials
//!   - Google OAuth2 (Gemini via AI Studio) — standard web flow
//!   - Anthropic — API key only (no public OAuth as of 2026)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub provider: OAuthProvider,
    pub client_id: String,
    /// PKCE code verifier (stored for token exchange, never sent to frontend)
    pub code_verifier: Option<String>,
    pub redirect_uri: String,
    pub auth_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OAuthProvider {
    OpenAI,
    Google,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    pub token_type: String,
    pub scope: Option<String>,
}

impl OAuthConfig {
    /// Build an OpenAI OAuth2 config using PKCE flow.
    pub fn openai(client_id: &str, redirect_uri: &str) -> Self {
        Self {
            provider: OAuthProvider::OpenAI,
            client_id: client_id.to_string(),
            code_verifier: None, // Generated at auth time
            redirect_uri: redirect_uri.to_string(),
            auth_url: "https://auth.openai.com/authorize".to_string(),
            token_url: "https://auth.openai.com/oauth/token".to_string(),
            scopes: vec![
                "openid".into(),
                "model.read".into(),
                "offline_access".into(),
            ],
        }
    }

    /// Build a Google OAuth2 config for Gemini/Vertex AI.
    pub fn google(client_id: &str, redirect_uri: &str) -> Self {
        Self {
            provider: OAuthProvider::Google,
            client_id: client_id.to_string(),
            code_verifier: None,
            redirect_uri: redirect_uri.to_string(),
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            scopes: vec![
                "openid".into(),
                "https://www.googleapis.com/auth/cloud-platform".into(),
                "https://www.googleapis.com/auth/generative-language".into(),
            ],
        }
    }

    /// Generate a state parameter + code verifier for PKCE, returning the full auth URL.
    pub fn generate_auth_url(&mut self) -> String {
        use rand::RngCore;
        use sha2::{Digest, Sha256};

        // Generate PKCE code verifier (43-128 chars)
        let mut verifier_bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut verifier_bytes);
        let verifier = base64_url_no_pad(&verifier_bytes);
        self.code_verifier = Some(verifier.clone());

        // Generate code challenge (SHA-256 of verifier, base64url)
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let challenge = base64_url_no_pad(&hasher.finalize());

        // Generate random state
        let mut state_bytes = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut state_bytes);
        let state = hex::encode(state_bytes);

        format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
            self.auth_url,
            urlencoding(&self.client_id),
            urlencoding(&self.redirect_uri),
            urlencoding(&self.scopes.join(" ")),
            state,
            challenge,
        )
    }
}

fn base64_url_no_pad(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
                c.to_string()
            } else {
                format!("%{:02X}", c as u8)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_auth_url() {
        let mut config = OAuthConfig::openai("test-client-id", "http://localhost:1420/callback");
        let url = config.generate_auth_url();
        assert!(url.starts_with("https://auth.openai.com/authorize?"));
        assert!(url.contains("client_id=test-client-id"));
        assert!(url.contains("code_challenge_method=S256"));
    }
}
