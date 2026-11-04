//! Provider ports — the one place the forge crosses the network boundary.
//!
//! Declaring a [`Provider`] performs no I/O: it records what is configured.
//! A provider is only ever called through a [`ProviderPort`] adapter that is
//! attached explicitly, after the UI has shown the request and the operator has
//! confirmed it. The mock adapter keeps the whole pipeline runnable offline.

use crate::error::ForgeResult;
use serde::{Deserialize, Serialize};

/// A declared provider — configuration metadata only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provider {
    /// Stable provider id, e.g. `groq`.
    pub id: String,
    /// Default model for this provider.
    pub model: String,
    /// Whether the credential for this provider is present in the environment.
    #[serde(default)]
    pub present: bool,
}

/// The request the forge hands a provider adapter at the network boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRequest {
    /// Which provider to route to.
    pub provider: String,
    /// Which model to use.
    pub model: String,
    /// The fully rendered, retrieval-grounded prompt.
    pub prompt: String,
    /// The evidence snippets included in the prompt, for the audit trail.
    #[serde(default)]
    pub evidence: Vec<String>,
}

/// Token accounting for one provider call, when the provider reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Tokens in the prompt.
    pub prompt_tokens: u64,
    /// Tokens in the completion.
    pub completion_tokens: u64,
}

/// A provider reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderResponse {
    /// Which provider answered.
    pub provider: String,
    /// Which model answered.
    pub model: String,
    /// The completion text.
    pub content: String,
    /// Token usage, when reported.
    #[serde(default)]
    pub usage: Option<Usage>,
}

/// The port every provider adapter implements.
///
/// No provider is called unless an adapter is attached *and* the operator has
/// confirmed the displayed request — the forge never fails over silently.
pub trait ProviderPort {
    /// The adapter's provider id.
    fn name(&self) -> &str;

    /// Complete one request. Implementations own their network and auth.
    fn complete(&self, request: &ProviderRequest) -> ForgeResult<ProviderResponse>;
}

/// A deterministic, network-free provider for tests and mock-first runs.
#[derive(Debug, Clone)]
pub struct MockProvider {
    name: String,
    reply: String,
}

impl MockProvider {
    /// A mock provider that always replies with `reply`.
    pub fn new(name: impl Into<String>, reply: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            reply: reply.into(),
        }
    }
}

impl ProviderPort for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn complete(&self, request: &ProviderRequest) -> ForgeResult<ProviderResponse> {
        Ok(ProviderResponse {
            provider: request.provider.clone(),
            model: request.model.clone(),
            content: self.reply.clone(),
            usage: Some(Usage {
                prompt_tokens: request.prompt.split_whitespace().count() as u64,
                completion_tokens: self.reply.split_whitespace().count() as u64,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_provider_is_deterministic_and_counts_tokens() {
        let provider = MockProvider::new("mock", "two tokens");
        assert_eq!(provider.name(), "mock");

        let request = ProviderRequest {
            provider: "mock".into(),
            model: "mock-1".into(),
            prompt: "one two three".into(),
            evidence: Vec::new(),
        };
        let response = provider.complete(&request).expect("mock completes");
        assert_eq!(response.content, "two tokens");
        let usage = response.usage.expect("usage reported");
        assert_eq!(usage.prompt_tokens, 3);
        assert_eq!(usage.completion_tokens, 2);
    }
}
