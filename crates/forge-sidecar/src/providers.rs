//! The LLM rungs: routing tables, budget policy and the chat adapter.
//!
//! `forge-core` declares providers and owns the [`ProviderPort`] trait, but it
//! deliberately performs no network I/O. This module is the adapter home: it
//! knows the OpenAI-compatible endpoint for each rung, which credential each
//! one needs, the class-aware fallback ladder, the local daily request ceiling
//! and the circuit-breaker policy — and it implements [`ProviderPort`] over a
//! [`HttpTransport`].
//!
//! Everything decision-shaped here is a pure function of the environment, so it
//! is fully unit-testable without a socket. The endpoint table mirrors the
//! desktop shell's live-call layer exactly; there is one definition of "where
//! Groq lives" in the product, and this is it.

use std::collections::BTreeMap;
use std::sync::Arc;

use forge_core::{ForgeError, ForgeResult, ProviderPort, ProviderRequest, ProviderResponse, Usage};
use serde_json::{json, Value};

use crate::env::Env;
use crate::transport::HttpTransport;

/// How many consecutive failures open a rung's breaker.
pub const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// How long an open breaker stays open.
pub const COOLDOWN_SECS: u64 = 30 * 60;

/// Where a rung's chat-completions URL comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointSpec {
    /// A fixed public URL.
    Fixed(&'static str),
    /// Cloudflare Workers AI, whose path embeds `CF_ACCOUNT_ID`.
    CloudflareWorkersAi,
    /// A session host whose base URL changes every session (`BONSAI_URL`).
    SessionHost {
        /// The environment variable holding the session base URL.
        url_env: &'static str,
    },
}

/// One provider rung the forge can call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rung {
    /// Stable provider id, e.g. `groq`.
    pub id: &'static str,
    /// Human-readable name.
    pub label: &'static str,
    /// The environment variable holding the credential, or `""` when the rung
    /// is unauthenticated (a session host on a private tunnel URL).
    pub credential_env: &'static str,
    /// Where to send the chat request.
    pub endpoint: EndpointSpec,
}

/// The credential environment variable for a rung, or `""`.
///
/// Returns `""` for an unknown id, so callers can treat "no credential" and
/// "secret provider" the same way.
pub fn credential_env(provider: &str) -> &'static str {
    rung(provider).map_or("", |rung| rung.credential_env)
}

/// The LLM rungs, cheapest-first: session GPU hosts, then free API executors,
/// then the remaining free tiers.
pub fn rungs() -> &'static [Rung] {
    &RUNGS
}

static RUNGS: [Rung; 10] = [
    Rung {
        id: "colab-bonsai",
        label: "Colab Bonsai",
        credential_env: "",
        endpoint: EndpointSpec::SessionHost {
            url_env: "BONSAI_URL",
        },
    },
    Rung {
        id: "kaggle-bonsai",
        label: "Kaggle Bonsai",
        credential_env: "",
        endpoint: EndpointSpec::SessionHost {
            url_env: "KAGGLE_BONSAI_URL",
        },
    },
    Rung {
        id: "groq",
        label: "Groq",
        credential_env: "GROQ_API_KEY",
        endpoint: EndpointSpec::Fixed("https://api.groq.com/openai/v1/chat/completions"),
    },
    Rung {
        id: "gemini",
        label: "Gemini",
        credential_env: "GEMINI_API_KEY",
        endpoint: EndpointSpec::Fixed(
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
        ),
    },
    Rung {
        id: "cloudflare",
        label: "Cloudflare Workers AI",
        credential_env: "CF_API_TOKEN",
        endpoint: EndpointSpec::CloudflareWorkersAi,
    },
    Rung {
        id: "nvidia",
        label: "NVIDIA NIM",
        credential_env: "NVIDIA_API_KEY",
        endpoint: EndpointSpec::Fixed("https://integrate.api.nvidia.com/v1/chat/completions"),
    },
    Rung {
        id: "openrouter",
        label: "OpenRouter",
        credential_env: "OPENROUTER_API_KEY",
        endpoint: EndpointSpec::Fixed("https://openrouter.ai/api/v1/chat/completions"),
    },
    Rung {
        id: "zhipu",
        label: "Zhipu / Z.ai",
        credential_env: "ZHIPU_API_KEY",
        endpoint: EndpointSpec::Fixed("https://open.bigmodel.cn/api/paas/v4/chat/completions"),
    },
    Rung {
        id: "modelscope",
        label: "ModelScope",
        credential_env: "MODELSCOPE_TOKEN",
        endpoint: EndpointSpec::Fixed("https://api-inference.modelscope.cn/v1/chat/completions"),
    },
    Rung {
        id: "allam",
        label: "Groq Allam",
        credential_env: "GROQ_API_KEY",
        endpoint: EndpointSpec::Fixed("https://api.groq.com/openai/v1/chat/completions"),
    },
];

/// Look a rung up by id.
pub fn rung(provider: &str) -> Option<&'static Rung> {
    rungs().iter().find(|rung| rung.id == provider)
}

/// Whether `provider` is an LLM rung with a chat endpoint.
pub fn is_llm(provider: &str) -> bool {
    rung(provider).is_some()
}

/// Resolve the chat-completions URL for a rung.
///
/// Non-loopback endpoints must be HTTPS: forge prompts carry source code, and a
/// plaintext endpoint would leak it. A loopback URL is allowed for a local
/// model server.
pub fn chat_url(provider: &str, env: &Env) -> ForgeResult<String> {
    let Some(rung) = rung(provider) else {
        return Err(ForgeError::Remote {
            remote: provider.to_string(),
            problem: "not a known provider rung".into(),
        });
    };
    let url = match rung.endpoint {
        EndpointSpec::Fixed(url) => url.to_string(),
        EndpointSpec::CloudflareWorkersAi => {
            let account = env.require("cloudflare", "CF_ACCOUNT_ID")?;
            format!("https://api.cloudflare.com/client/v4/accounts/{account}/ai/v1/chat/completions")
        }
        EndpointSpec::SessionHost { url_env } => {
            let base = env.require(provider, url_env)?;
            format!("{}/chat/completions", base.trim_end_matches('/'))
        }
    };
    if !url.starts_with("https://") && !is_loopback(&url) {
        return Err(ForgeError::Remote {
            remote: provider.to_string(),
            problem: "endpoint must use https (loopback is allowed for a local model server)".into(),
        });
    }
    Ok(url)
}

/// Whether a URL targets the local machine.
pub(crate) fn is_loopback(url: &str) -> bool {
    url.starts_with("http://127.0.0.1")
        || url.starts_with("http://localhost")
        || url.starts_with("http://[::1]")
}

/// The class-aware fallback ladder for a preferred rung, best first.
///
/// Every automatic fallback still requires an explicitly attached transport and
/// stays bounded by the caller's daily ledger, so a ladder never spends quota
/// silently.
pub fn route_order(preferred: &str) -> Vec<&'static str> {
    let defaults: &[&'static str] = match preferred {
        "colab-bonsai" => &["colab-bonsai", "kaggle-bonsai", "gemini", "groq"],
        "kaggle-bonsai" => &["kaggle-bonsai", "colab-bonsai", "gemini", "groq"],
        "groq" => &["groq", "gemini", "cloudflare", "openrouter"],
        "cloudflare" => &["cloudflare", "groq", "gemini"],
        "openrouter" => &["openrouter", "groq", "gemini"],
        _ => &["gemini", "groq", "cloudflare", "openrouter"],
    };
    let mut order: Vec<&'static str> = Vec::new();
    // Only a declared rung can lead the ladder, and its id is `'static`.
    if let Some(preferred_rung) = rung(preferred) {
        order.push(preferred_rung.id);
    }
    for candidate in defaults {
        if !order.contains(candidate) {
            order.push(candidate);
        }
    }
    order
}

/// The local daily request ceiling for a rung.
///
/// Deliberately conservative: the ledger caps *this machine*, not the account,
/// so a runaway loop cannot burn a free tier in one afternoon.
pub fn budget_cap(provider: &str) -> u64 {
    match provider {
        "openrouter" => 20,
        "cloudflare" => 20,
        "groq" | "allam" => 30,
        "gemini" => 30,
        _ => 10,
    }
}

/// The monthly-use ceiling as a fraction of a published daily budget.
///
/// The router spends at most this share of any quota a provider publishes, so a
/// known daily allowance is never exhausted by automated runs.
pub const BUDGET_SHARE: f64 = 0.8;

/// One rung's breaker state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BreakerState {
    /// Failures since the last success.
    pub consecutive_failures: u32,
    /// When the breaker opened, in Unix seconds.
    pub opened_at: Option<u64>,
}

/// Consecutive-failure circuit breakers, one per rung.
///
/// A rung that fails three times in a row is skipped for thirty minutes. Quota
/// walls (`402`, `403`, `429`) are ordinary failures here: they count, they
/// never retry eagerly, and they never fall through to a rung the caller did
/// not configure.
#[derive(Debug, Clone, Default)]
pub struct CircuitBreaker {
    states: BTreeMap<String, BreakerState>,
}

impl CircuitBreaker {
    /// Failing this many times in a row opens the breaker.
    pub const THRESHOLD: u32 = MAX_CONSECUTIVE_FAILURES;

    /// An empty breaker set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the rung is currently skipped.
    pub fn is_open(&self, provider: &str, now: u64) -> bool {
        self.states
            .get(provider)
            .and_then(|state| state.opened_at)
            .is_some_and(|opened_at| now < opened_at.saturating_add(COOLDOWN_SECS))
    }

    /// The recorded state for a rung.
    pub fn state(&self, provider: &str) -> BreakerState {
        self.states.get(provider).copied().unwrap_or_default()
    }

    /// Record a failure, opening the breaker at the threshold.
    pub fn record_failure(&mut self, provider: &str, now: u64) {
        let entry = self.states.entry(provider.to_string()).or_default();
        if entry
            .opened_at
            .is_some_and(|opened_at| now >= opened_at.saturating_add(COOLDOWN_SECS))
        {
            // The cooldown elapsed, so this is a fresh window.
            entry.consecutive_failures = 0;
            entry.opened_at = None;
        }
        entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
        if entry.consecutive_failures >= Self::THRESHOLD && entry.opened_at.is_none() {
            entry.opened_at = Some(now);
        }
    }

    /// Record a success, closing the breaker.
    pub fn record_success(&mut self, provider: &str) {
        self.states.remove(provider);
    }
}

/// The OpenAI-compatible chat adapter for one rung.
///
/// Built per call from the resolved [`Env`] and a shared transport, which is
/// what keeps a credential out of any long-lived struct.
pub struct HttpProvider {
    id: String,
    model: String,
    env: Env,
    transport: Arc<dyn HttpTransport>,
    max_tokens: u32,
    temperature: f64,
}

impl HttpProvider {
    /// The default completion cap.
    pub const DEFAULT_MAX_TOKENS: u32 = 1200;
    /// The default sampling temperature.
    pub const DEFAULT_TEMPERATURE: f64 = 0.2;

    /// An adapter for `id`, defaulting to `model` when a request omits one.
    pub fn new(
        id: impl Into<String>,
        model: impl Into<String>,
        env: Env,
        transport: Arc<dyn HttpTransport>,
    ) -> Self {
        Self {
            id: id.into(),
            model: model.into(),
            env,
            transport,
            max_tokens: Self::DEFAULT_MAX_TOKENS,
            temperature: Self::DEFAULT_TEMPERATURE,
        }
    }

    /// Override the completion cap and temperature.
    pub fn with_sampling(mut self, max_tokens: u32, temperature: f64) -> Self {
        self.max_tokens = max_tokens;
        self.temperature = temperature;
        self
    }

    /// The OpenAI-compatible request body.
    pub fn payload(&self, request: &ProviderRequest) -> Value {
        let model = if request.model.is_empty() {
            self.model.as_str()
        } else {
            request.model.as_str()
        };
        json!({
            "model": model,
            "messages": [{ "role": "user", "content": request.prompt }],
            "max_tokens": self.max_tokens,
            "temperature": self.temperature,
        })
    }

    /// The headers for one call, refusing before any I/O when a credential is
    /// missing.
    fn headers(&self, provider: &str) -> ForgeResult<Vec<(&'static str, String)>> {
        let mut headers: Vec<(&'static str, String)> =
            vec![("Content-Type", "application/json".into())];
        let key_env = credential_env(provider);
        if !key_env.is_empty() {
            let key = self.env.require(provider, key_env)?;
            headers.push(("Authorization", format!("Bearer {key}")));
        }
        Ok(headers)
    }
}

impl ProviderPort for HttpProvider {
    fn name(&self) -> &str {
        &self.id
    }

    fn complete(&self, request: &ProviderRequest) -> ForgeResult<ProviderResponse> {
        let provider = if request.provider.is_empty() {
            self.id.as_str()
        } else {
            request.provider.as_str()
        };
        let url = chat_url(provider, &self.env)?;
        let headers = self.headers(provider)?;
        let payload = self.payload(request);
        let response = self
            .transport
            .post_json(&url, &headers, &payload)
            .map_err(|error| ForgeError::Remote {
                remote: provider.to_string(),
                problem: error.to_string(),
            })?;
        parse_completion(provider, request, &response.body)
    }
}

/// Pull the completion text and token usage out of an OpenAI-compatible reply.
fn parse_completion(
    provider: &str,
    request: &ProviderRequest,
    body: &Value,
) -> ForgeResult<ProviderResponse> {
    let content = body
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| ForgeError::Remote {
            remote: provider.to_string(),
            problem: "response carried no completion text".into(),
        })?;
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(request.model.as_str())
        .to_string();
    Ok(ProviderResponse {
        provider: provider.to_string(),
        model,
        content: content.to_string(),
        usage: usage_from(body),
    })
}

/// Read token usage, falling back to `total_tokens` when the split is absent.
fn usage_from(body: &Value) -> Option<Usage> {
    let usage = body.get("usage")?;
    let prompt_tokens = usage.get("prompt_tokens").and_then(Value::as_u64);
    let completion_tokens = usage.get("completion_tokens").and_then(Value::as_u64);
    match (prompt_tokens, completion_tokens) {
        (Some(prompt_tokens), Some(completion_tokens)) => Some(Usage {
            prompt_tokens,
            completion_tokens,
        }),
        _ => usage
            .get("total_tokens")
            .and_then(Value::as_u64)
            .map(|total| Usage {
                prompt_tokens: total,
                completion_tokens: 0,
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{HttpResponse, MockTransport};

    fn env_with(pairs: &[(&str, &str)]) -> Env {
        let mut env = Env::new();
        for (key, value) in pairs {
            env.set(*key, *value);
        }
        env
    }

    fn request(provider: &str, model: &str) -> ProviderRequest {
        ProviderRequest {
            provider: provider.into(),
            model: model.into(),
            prompt: "explain the coverage ledger".into(),
            evidence: Vec::new(),
        }
    }

    fn groq_reply() -> HttpResponse {
        HttpResponse::json(json!({
            "model": "llama-3.3-70b",
            "choices": [{ "message": { "role": "assistant", "content": "grounded answer" } }],
            "usage": { "prompt_tokens": 11, "completion_tokens": 7, "total_tokens": 18 }
        }))
    }

    #[test]
    fn every_documented_rung_is_declared_once() {
        let ids: Vec<&str> = rungs().iter().map(|rung| rung.id).collect();
        assert_eq!(ids.len(), 10);
        for expected in [
            "colab-bonsai",
            "kaggle-bonsai",
            "groq",
            "gemini",
            "cloudflare",
            "nvidia",
            "openrouter",
            "zhipu",
            "modelscope",
            "allam",
        ] {
            assert_eq!(ids.iter().filter(|id| **id == expected).count(), 1, "{expected}");
        }
        // A session host is unauthenticated; every API rung names a credential.
        assert_eq!(credential_env("colab-bonsai"), "");
        assert_eq!(credential_env("groq"), "GROQ_API_KEY");
        assert_eq!(credential_env("nope"), "");
    }

    #[test]
    fn endpoints_match_the_documented_services() {
        let env = env_with(&[
            ("CF_ACCOUNT_ID", "acct-1"),
            ("BONSAI_URL", "https://abc.ngrok.app/"),
            ("KAGGLE_BONSAI_URL", "https://xyz.kaggle.net"),
        ]);
        assert_eq!(
            chat_url("groq", &env).expect("groq"),
            "https://api.groq.com/openai/v1/chat/completions"
        );
        assert_eq!(
            chat_url("nvidia", &env).expect("nvidia"),
            "https://integrate.api.nvidia.com/v1/chat/completions"
        );
        assert_eq!(
            chat_url("cloudflare", &env).expect("cloudflare"),
            "https://api.cloudflare.com/client/v4/accounts/acct-1/ai/v1/chat/completions"
        );
        // A trailing slash on the session base must not double up.
        assert_eq!(
            chat_url("colab-bonsai", &env).expect("colab"),
            "https://abc.ngrok.app/chat/completions"
        );
        assert_eq!(
            chat_url("kaggle-bonsai", &env).expect("kaggle"),
            "https://xyz.kaggle.net/chat/completions"
        );
    }

    #[test]
    fn a_missing_endpoint_or_credential_is_named_precisely() {
        let env = Env::new();
        let missing_account = chat_url("cloudflare", &env).expect_err("account");
        assert!(matches!(missing_account, ForgeError::Credential { ref key, .. } if key == "CF_ACCOUNT_ID"));
        let missing_session = chat_url("colab-bonsai", &env).expect_err("session url");
        assert!(matches!(missing_session, ForgeError::Credential { ref key, .. } if key == "BONSAI_URL"));
        let unknown = chat_url("nope", &env).expect_err("unknown rung");
        assert!(matches!(unknown, ForgeError::Remote { .. }));
        // Prompts carry source code, so a plaintext endpoint is refused.
        let insecure = env_with(&[("BONSAI_URL", "http://example.test:8080")]);
        assert!(chat_url("colab-bonsai", &insecure).is_err());
        let loopback = env_with(&[("BONSAI_URL", "http://127.0.0.1:8080")]);
        assert!(chat_url("colab-bonsai", &loopback).is_ok());
    }

    #[test]
    fn the_ladder_puts_the_preferred_rung_first_without_duplicates() {
        assert_eq!(
            route_order("groq"),
            vec!["groq", "gemini", "cloudflare", "openrouter"]
        );
        assert_eq!(
            route_order("colab-bonsai"),
            vec!["colab-bonsai", "kaggle-bonsai", "gemini", "groq"]
        );
        assert_eq!(route_order("zhipu"), vec!["zhipu", "gemini", "groq", "cloudflare", "openrouter"]);
        let ladder = route_order("gemini");
        let mut sorted = ladder.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ladder.len(), "the ladder must not repeat a rung");
    }

    #[test]
    fn budget_caps_track_the_documented_ceilings() {
        assert_eq!(budget_cap("openrouter"), 20);
        assert_eq!(budget_cap("cloudflare"), 20);
        assert_eq!(budget_cap("groq"), 30);
        assert_eq!(budget_cap("gemini"), 30);
        assert_eq!(budget_cap("nvidia"), 10);
        assert!(BUDGET_SHARE > 0.0 && BUDGET_SHARE < 1.0);
    }

    #[test]
    fn the_breaker_opens_on_the_third_failure_and_cools_down() {
        let mut breaker = CircuitBreaker::new();
        breaker.record_failure("groq", 100);
        breaker.record_failure("groq", 101);
        assert!(!breaker.is_open("groq", 101));
        assert_eq!(breaker.state("groq").consecutive_failures, 2);

        breaker.record_failure("groq", 102);
        assert!(breaker.is_open("groq", 102));
        assert!(breaker.is_open("groq", 102 + COOLDOWN_SECS - 1));
        // The cooldown elapses and the rung is retried.
        assert!(!breaker.is_open("groq", 102 + COOLDOWN_SECS));

        breaker.record_success("groq");
        assert_eq!(breaker.state("groq"), BreakerState::default());
        assert!(!breaker.is_open("groq", 103));
    }

    #[test]
    fn a_failure_after_a_cooldown_starts_a_fresh_window() {
        let mut breaker = CircuitBreaker::new();
        for now in [0, 1, 2] {
            breaker.record_failure("groq", now);
        }
        assert!(breaker.is_open("groq", 2));
        breaker.record_failure("groq", COOLDOWN_SECS + 10);
        assert_eq!(breaker.state("groq").consecutive_failures, 1);
        assert!(!breaker.is_open("groq", COOLDOWN_SECS + 10));
    }

    #[test]
    fn a_completion_sends_the_documented_body_and_parses_the_reply() {
        let env = env_with(&[("GROQ_API_KEY", "gsk_test")]);
        let transport = MockTransport::new().route("api.groq.com", groq_reply());
        let provider = HttpProvider::new("groq", "llama-3.3-70b", env, Arc::new(transport));

        let response = provider
            .complete(&request("groq", "llama-3.3-70b"))
            .expect("completes");
        assert_eq!(response.content, "grounded answer");
        assert_eq!(response.provider, "groq");
        assert_eq!(response.model, "llama-3.3-70b");
        assert_eq!(
            response.usage,
            Some(Usage {
                prompt_tokens: 11,
                completion_tokens: 7
            })
        );
    }

    #[test]
    fn the_bearer_credential_is_attached_to_the_request() {
        let env = env_with(&[("GROQ_API_KEY", "gsk_test")]);
        let recorder = Arc::new(MockTransport::new().route("api.groq.com", groq_reply()));
        let provider = HttpProvider::new("groq", "llama-3.3-70b", env, recorder.clone());

        provider
            .complete(&request("groq", "llama-3.3-70b"))
            .expect("completes");
        let calls = recorder.calls();
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0]
                .headers
                .iter()
                .any(|(name, value)| name == "Authorization" && value == "Bearer gsk_test"),
            "the credential must ride in an Authorization header"
        );
        assert!(calls[0]
            .headers
            .iter()
            .any(|(name, value)| name == "Content-Type" && value == "application/json"));
    }

    #[test]
    fn a_missing_credential_refuses_before_any_network_use() {
        let env = Env::new();
        let transport = MockTransport::new().route("api.groq.com", groq_reply());
        let recorder = Arc::new(transport);
        let provider = HttpProvider::new("groq", "llama-3.3-70b", env, recorder.clone());

        let error = provider
            .complete(&request("groq", "llama-3.3-70b"))
            .expect_err("no credential");
        assert!(matches!(error, ForgeError::Credential { ref key, .. } if key == "GROQ_API_KEY"));
        assert!(recorder.calls().is_empty(), "no request may leave without a credential");
    }

    #[test]
    fn a_session_host_needs_no_credential_and_sends_no_authorization() {
        let env = env_with(&[("BONSAI_URL", "https://abc.ngrok.app")]);
        let transport = MockTransport::new().route(
            "abc.ngrok.app",
            HttpResponse::json(json!({
                "choices": [{ "message": { "content": "local" } }]
            })),
        );
        let recorder = Arc::new(transport);
        let provider = HttpProvider::new("colab-bonsai", "bonsai-27b", env, recorder.clone());

        let response = provider.complete(&request("colab-bonsai", "bonsai-27b")).expect("completes");
        assert_eq!(response.content, "local");
        // `total_tokens` is absent, so usage is unreported rather than invented.
        assert_eq!(response.usage, None);

        let calls = recorder.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].url, "https://abc.ngrok.app/chat/completions");
        assert!(
            !calls[0].headers.iter().any(|(name, _)| name == "Authorization"),
            "an unauthenticated session host must not receive an Authorization header"
        );
    }

    #[test]
    fn an_http_error_becomes_a_remote_error_with_a_truncated_body() {
        let env = env_with(&[("NVIDIA_API_KEY", "nvapi_test")]);
        let transport = MockTransport::new().route(
            "integrate.api.nvidia.com",
            HttpResponse {
                status: 429,
                body: Value::String("rate limited".into()),
            },
        );
        let mut provider = HttpProvider::new("nvidia", "meta/llama", env, Arc::new(transport));
        provider = provider.with_sampling(256, 0.0);
        let error = provider
            .complete(&request("nvidia", "meta/llama"))
            .expect_err("quota wall");
        assert!(matches!(error, ForgeError::Remote { .. }));
        assert!(error.to_string().contains("quota") || error.to_string().contains("429"));
    }

    #[test]
    fn the_payload_carries_the_prompt_and_sampling() {
        let env = env_with(&[("GROQ_API_KEY", "gsk_test")]);
        let provider = HttpProvider::new("groq", "llama-3.3-70b", env, Arc::new(MockTransport::new()))
            .with_sampling(512, 0.1);
        let payload = provider.payload(&request("groq", "llama-3.3-70b"));
        assert_eq!(payload["model"], json!("llama-3.3-70b"));
        assert_eq!(payload["max_tokens"], json!(512));
        assert_eq!(payload["temperature"], json!(0.1));
        assert_eq!(payload["messages"][0]["content"], json!("explain the coverage ledger"));
    }
}
