//! The exact protocol-1.2 provider declaration.

use gray_plugin::{
    AuthMethodDecl, PROVIDER_CREDENTIALS, ProviderAuthorizationDecl, ProviderDecl,
    ProviderRequestPolicyDecl, ProviderTransportDecl,
};

use crate::oauth::RESOURCE;

pub const PLUGIN_NAME: &str = "chatgpt";
pub const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROVIDER_ID: &str = "chatgpt";
pub const AUTH_METHOD_ID: &str = "chatgpt-plan";

pub fn manifest() -> gray_plugin::Manifest {
    gray_plugin::Manifest {
        name: PLUGIN_NAME.to_string(),
        version: PLUGIN_VERSION.to_string(),
        tools: Vec::new(),
        commands: Vec::new(),
        hooks: Vec::new(),
        protocol: Some("1.2".to_string()),
        subcommands: Vec::new(),
        capabilities: vec![PROVIDER_CREDENTIALS.to_string()],
        providers: vec![provider()],
        provider_errors: Vec::new(),
    }
}

/// The host POSTs straight to the public Responses API with the OAuth access
/// token — no relay, no `chat` operation. The request policy follows OpenAI's
/// plan-usage preview rules: `store: false` (host-enforced), no
/// `previous_response_id` (history travels in `input`), and no warm replay
/// (a replay would spend the user's plan usage on nothing).
pub fn provider() -> ProviderDecl {
    ProviderDecl {
        id: PROVIDER_ID.to_string(),
        name: "ChatGPT plan".to_string(),
        transport: ProviderTransportDecl {
            kind: "openai-responses".to_string(),
            base_url: RESOURCE.parse().expect("resource URL"),
            authorization: ProviderAuthorizationDecl {
                kind: "bearer".to_string(),
                secret_name: "access_token".to_string(),
            },
            request: ProviderRequestPolicyDecl {
                prompt_cache_key: true,
                warm_replay: false,
                store: false,
                include_reasoning_encrypted: true,
                previous_response_id: false,
                tool_choice: None,
                parallel_tool_calls: None,
                text_verbosity: None,
                cache_ttl_secs: None,
            },
            headers: Vec::new(),
        },
        auth_methods: vec![AuthMethodDecl {
            id: AUTH_METHOD_ID.to_string(),
            name: "Sign in with ChatGPT".to_string(),
            kind: "oauth".to_string(),
            operations: vec![
                "login".to_string(),
                "refresh".to_string(),
                "revoke".to_string(),
                "models".to_string(),
            ],
        }],
    }
}

#[path = "manifest_tests.rs"]
#[cfg(test)]
mod tests;
