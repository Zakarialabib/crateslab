//! The JSON-RPC envelope and method router.
//!
//! Requests are newline-delimited JSON objects on stdin; responses are
//! newline-delimited JSON objects on stdout. The router owns no globals: the
//! [`Services`] a call needs (the resolved environment and the shared
//! transport) are injected, so every method — including the networked ones — is
//! unit-testable with a mock transport and a hand-written environment.

use std::sync::Arc;

use forge_core::{ProviderPort, ProviderRequest, WorkflowRegistry};
use forge_rag::tokenize;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::circleci::circleci_dashboard;
use crate::datasets;
use crate::providers::{self, HttpProvider};
use crate::qdrant::qdrant_collections;
use crate::remote::{self, Remote, RemoteSearchRequest};
use crate::services::Services;
use crate::transport::HttpTransport;

/// A JSON-RPC-style request on the sidecar's stdio transport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// Caller-chosen correlation id, echoed back on the response.
    pub id: u64,
    /// The method to invoke, e.g. `forge.workflows`.
    pub method: String,
    /// Method parameters.
    #[serde(default)]
    pub params: Value,
}

/// The reply to a [`Request`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    /// The request id this replies to (`0` when the request could not be read).
    pub id: u64,
    /// The result, when the call succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// The error, when the call failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

/// A JSON-RPC error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    /// One of the [`INVALID_REQUEST`], [`METHOD_NOT_FOUND`], [`NOT_CONFIGURED`]
    /// or [`INTERNAL_ERROR`] codes.
    pub code: i32,
    /// A human-readable message.
    pub message: String,
}

impl Response {
    /// A successful reply.
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
        }
    }

    /// A failed reply.
    pub fn err(id: u64, code: i32, message: impl Into<String>) -> Self {
        Self {
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

/// The request could not be parsed, or a parameter was missing or invalid.
pub const INVALID_REQUEST: i32 = -32600;
/// The method is not implemented.
pub const METHOD_NOT_FOUND: i32 = -32601;
/// The method failed.
pub const INTERNAL_ERROR: i32 = -32603;
/// A service needs a credential that is not configured (a settings action).
pub const NOT_CONFIGURED: i32 = -32001;

/// The most rows any search method will return.
const MAX_TOP_K: usize = 20;

/// The default page size for a search method.
const DEFAULT_TOP_K: usize = 6;

/// Route a request to a forge capability.
///
/// Methods: `forge.ping`, `forge.workflows`, `forge.pipeline`, `forge.tokenize`,
/// `forge.providers`, `forge.complete`, `forge.remotes`, `forge.remote.search`,
/// `forge.datasets`, `forge.dataset.search`, `forge.circleci`, `forge.qdrant`.
pub fn dispatch(services: &dyn Services, request: &Request) -> Response {
    match request.method.as_str() {
        "forge.ping" => Response::ok(request.id, json!({ "pong": true })),

        "forge.workflows" => match forge_core::embedded() {
            Ok(registry) => Response::ok(request.id, registry_json(&registry)),
            Err(error) => forge_error(request.id, error),
        },

        "forge.pipeline" => pipeline(&request.id, &request.params),

        "forge.tokenize" => match string_param(&request.params, "text") {
            Some(text) => Response::ok(request.id, json!({ "tokens": tokenize(text) })),
            None => missing(&request.id, "text"),
        },

        "forge.providers" => Response::ok(request.id, providers_json(services.env())),

        "forge.complete" => complete(services, &request.id, &request.params),

        "forge.remotes" => Response::ok(request.id, remotes_json(services.env())),

        "forge.remote.search" => remote_search(services, &request.id, &request.params),

        "forge.datasets" => Response::ok(request.id, datasets_json(services.env())),

        "forge.dataset.search" => dataset_search(services, &request.id, &request.params),

        "forge.circleci" => {
            let transport = services.transport();
            match circleci_dashboard(services.env(), transport.as_ref()) {
                Ok(dashboard) => Response::ok(request.id, dashboard),
                Err(error) => forge_error(request.id, error),
            }
        }

        "forge.qdrant" => {
            let transport = services.transport();
            match qdrant_collections(services.env(), transport.as_ref()) {
                Ok(collections) => Response::ok(request.id, collections),
                Err(error) => forge_error(request.id, error),
            }
        }

        other => Response::err(
            request.id,
            METHOD_NOT_FOUND,
            format!("unknown method '{other}'"),
        ),
    }
}

/// A required string parameter, or `None` when it is missing or not a string.
fn string_param<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

/// The error for a missing string parameter.
fn missing(id: &u64, key: &str) -> Response {
    Response::err(*id, INVALID_REQUEST, format!("missing string param '{key}'"))
}

/// An optional `top_k`, clamped to a safe page size.
fn top_k(params: &Value) -> usize {
    params
        .get("top_k")
        .and_then(Value::as_u64)
        .map(|k| (k as usize).clamp(1, MAX_TOP_K))
        .unwrap_or(DEFAULT_TOP_K)
}

/// Render a [`forge_core::ForgeError`], distinguishing "fix your settings" from
/// a genuine failure.
fn forge_error(id: u64, error: forge_core::ForgeError) -> Response {
    let code = match error {
        forge_core::ForgeError::Credential { .. } => NOT_CONFIGURED,
        _ => INTERNAL_ERROR,
    };
    Response::err(id, code, error.to_string())
}

/// Run one authored workflow, gated on the grounded-evidence count.
///
/// Params: `{ "workflow": "<id>", "evidence": <usize, default 0> }`. A refusal
/// is returned as data in `result` (the run still succeeds), never as `error`.
fn pipeline(id: &u64, params: &Value) -> Response {
    let registry = match forge_core::embedded() {
        Ok(registry) => registry,
        Err(error) => return forge_error(*id, error),
    };
    let Some(workflow_id) = string_param(params, "workflow") else {
        return missing(id, "workflow");
    };
    let Some(workflow) = registry.get(workflow_id) else {
        return Response::err(*id, INVALID_REQUEST, format!("unknown workflow '{workflow_id}'"));
    };
    let evidence = params
        .get("evidence")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    match forge_core::Pipeline::new(workflow).run(evidence) {
        Ok(outcome) => Response::ok(*id, json!(outcome)),
        Err(error) => forge_error(*id, error),
    }
}

/// The LLM rungs, with the endpoint each one resolves to and whether it is
/// ready to call.
fn providers_json(env: &crate::env::Env) -> Value {
    let rows: Vec<Value> = providers::rungs()
        .iter()
        .map(|rung| {
            let endpoint = providers::chat_url(rung.id, env).ok();
            let credential_present = rung.credential_env.is_empty() || env.has(rung.credential_env);
            json!({
                "id": rung.id,
                "label": rung.label,
                "credential_env": rung.credential_env,
                "endpoint": endpoint,
                "configured": credential_present && endpoint.is_some(),
                "daily_cap": providers::budget_cap(rung.id),
            })
        })
        .collect();
    json!({ "count": rows.len(), "providers": rows, "budget_share": providers::BUDGET_SHARE })
}

/// Complete one grounded prompt through a rung.
fn complete(services: &dyn Services, id: &u64, params: &Value) -> Response {
    let Some(provider) = string_param(params, "provider") else {
        return missing(id, "provider");
    };
    let Some(model) = string_param(params, "model") else {
        return missing(id, "model");
    };
    let Some(prompt) = string_param(params, "prompt") else {
        return missing(id, "prompt");
    };
    let evidence: Vec<String> = params
        .get("evidence")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let adapter = HttpProvider::new(
        provider,
        model,
        services.env().clone(),
        services.transport(),
    );
    let request = ProviderRequest {
        provider: provider.to_string(),
        model: model.to_string(),
        prompt: prompt.to_string(),
        evidence,
    };
    match adapter.complete(&request) {
        Ok(response) => Response::ok(*id, json!(response)),
        Err(error) => forge_error(*id, error),
    }
}

/// The declared remotes, with the environment keys each one still needs.
fn remotes_json(env: &crate::env::Env) -> Value {
    let rows: Vec<Value> = Remote::declared()
        .iter()
        .map(|remote| json!(remote::readiness(remote, env)))
        .collect();
    json!({ "count": rows.len(), "remotes": rows })
}

/// Dispatch a search to a declared remote.
fn remote_search(services: &dyn Services, id: &u64, params: &Value) -> Response {
    let Some(remote_id) = string_param(params, "remote") else {
        return missing(id, "remote");
    };
    let Some(remote) = Remote::declared_by_id(remote_id) else {
        return Response::err(*id, INVALID_REQUEST, format!("unknown remote '{remote_id}'"));
    };
    let Some(query) = string_param(params, "query") else {
        return missing(id, "query");
    };
    let request = RemoteSearchRequest {
        remote: remote.id.clone(),
        query: query.to_string(),
        scope: params
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        top_k: top_k(params),
    };
    let transport = services.transport();
    match remote::search_remote(&remote, services.env(), transport.as_ref(), &request) {
        Ok(hits) => Response::ok(*id, json!({ "count": hits.len(), "hits": hits })),
        Err(error) => forge_error(*id, error),
    }
}

/// The pinned datasets, with the revision each one is read at.
fn datasets_json(env: &crate::env::Env) -> Value {
    let rows: Vec<Value> = datasets::pins()
        .iter()
        .map(|pin| {
            let missing = pin.missing_keys(env);
            json!({
                "id": pin.id,
                "label": pin.label,
                "repo_env": pin.repo_env,
                "revision_env": pin.revision_env,
                "token_env": pin.token_env,
                "url": pin.download_url(env).ok(),
                "revision": env.get(pin.revision_env),
                "configured": missing.is_empty(),
                "missing": missing,
            })
        })
        .collect();
    json!({ "count": rows.len(), "datasets": rows })
}

/// Retrieve grounded evidence from a pinned dataset.
fn dataset_search(services: &dyn Services, id: &u64, params: &Value) -> Response {
    let Some(dataset_id) = string_param(params, "dataset") else {
        return missing(id, "dataset");
    };
    let Some(pin) = datasets::pin(dataset_id) else {
        return Response::err(*id, INVALID_REQUEST, format!("unknown dataset '{dataset_id}'"));
    };
    let Some(query) = string_param(params, "query") else {
        return missing(id, "query");
    };
    let transport: Arc<dyn HttpTransport> = services.transport();
    match datasets::search_dataset(pin, services.env(), transport.as_ref(), query, top_k(params)) {
        Ok(packet) => Response::ok(*id, json!(packet)),
        Err(error) => forge_error(*id, error),
    }
}

fn registry_json(registry: &WorkflowRegistry) -> Value {
    json!({
        "count": registry.workflows.len(),
        "workflows": registry.workflows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::Env;
    use crate::transport::{HttpResponse, MockTransport};

    /// The injected services a test router runs against: no sockets.
    struct TestServices {
        env: Env,
        transport: Arc<dyn HttpTransport>,
    }

    impl Services for TestServices {
        fn env(&self) -> &Env {
            &self.env
        }

        fn transport(&self) -> Arc<dyn HttpTransport> {
            Arc::clone(&self.transport)
        }
    }

    fn services(env: Env, transport: MockTransport) -> TestServices {
        TestServices {
            env,
            transport: Arc::new(transport),
        }
    }

    fn offline() -> TestServices {
        services(Env::new(), MockTransport::new())
    }

    fn request(method: &str, params: Value) -> Request {
        Request {
            id: 7,
            method: method.into(),
            params,
        }
    }

    #[test]
    fn ping_pongs_with_the_same_id() {
        let response = dispatch(&offline(), &request("forge.ping", json!({})));
        assert_eq!(response.id, 7);
        assert_eq!(response.result, Some(json!({ "pong": true })));
        assert!(response.error.is_none());
    }

    #[test]
    fn workflows_serves_the_embedded_registry() {
        let response = dispatch(&offline(), &request("forge.workflows", json!({})));
        let result = response.result.expect("result");
        let count = result.get("count").and_then(Value::as_u64).expect("count");
        assert!(count >= 4);
    }

    #[test]
    fn tokenize_needs_a_text_param() {
        let ok = dispatch(
            &offline(),
            &request("forge.tokenize", json!({ "text": "Coverage_Ledger" })),
        );
        assert_eq!(ok.result, Some(json!({ "tokens": ["coverage_ledger"] })));

        let missing = dispatch(&offline(), &request("forge.tokenize", json!({})));
        assert_eq!(missing.error.expect("error").code, INVALID_REQUEST);
    }

    #[test]
    fn pipeline_refuses_without_evidence_and_clears_with_it() {
        let refused = dispatch(
            &offline(),
            &request(
                "forge.pipeline",
                json!({ "workflow": "cratelab-intern-evolution", "evidence": 0 }),
            ),
        );
        let result = refused.result.expect("result");
        assert_eq!(result["halted"], json!(true));
        assert_eq!(result["runs"][0]["verdict"]["verdict"], json!("refuse"));

        let passed = dispatch(
            &offline(),
            &request(
                "forge.pipeline",
                json!({ "workflow": "cratelab-intern-evolution", "evidence": 4 }),
            ),
        );
        let result = passed.result.expect("result");
        assert_eq!(result["halted"], json!(false));
    }

    #[test]
    fn pipeline_rejects_an_unknown_workflow() {
        let response = dispatch(&offline(), &request("forge.pipeline", json!({ "workflow": "nope" })));
        assert_eq!(response.error.expect("error").code, INVALID_REQUEST);
    }

    #[test]
    fn unknown_methods_are_reported_not_panicked() {
        let response = dispatch(&offline(), &request("forge.nope", json!({})));
        assert_eq!(response.error.expect("error").code, METHOD_NOT_FOUND);
    }

    #[test]
    fn providers_report_readiness_from_the_environment() {
        let mut env = Env::new();
        env.set("GROQ_API_KEY", "gsk_test");
        let response = dispatch(&services(env, MockTransport::new()), &request("forge.providers", json!({})));
        let result = response.result.expect("result");
        assert_eq!(result["count"], json!(10));

        let providers = result["providers"].as_array().expect("providers");
        let groq = providers.iter().find(|row| row["id"] == "groq").expect("groq");
        assert_eq!(groq["configured"], json!(true));
        assert_eq!(groq["endpoint"], json!("https://api.groq.com/openai/v1/chat/completions"));

        let nvidia = providers.iter().find(|row| row["id"] == "nvidia").expect("nvidia");
        assert_eq!(nvidia["configured"], json!(false), "no credential, no rung");
        assert_eq!(nvidia["credential_env"], json!("NVIDIA_API_KEY"));
    }

    #[test]
    fn complete_refuses_with_a_setup_code_when_a_credential_is_missing() {
        let response = dispatch(
            &offline(),
            &request(
                "forge.complete",
                json!({ "provider": "groq", "model": "llama-3.3-70b", "prompt": "explain" }),
            ),
        );
        let error = response.error.expect("error");
        assert_eq!(error.code, NOT_CONFIGURED);
        assert!(error.message.contains("GROQ_API_KEY"));
    }

    #[test]
    fn complete_posts_once_and_returns_the_grounded_reply() {
        let mut env = Env::new();
        env.set("GROQ_API_KEY", "gsk_test");
        let transport = MockTransport::new().route(
            "api.groq.com",
            HttpResponse::json(json!({
                "model": "llama-3.3-70b",
                "choices": [{ "message": { "content": "grounded answer" } }],
                "usage": { "prompt_tokens": 4, "completion_tokens": 2 }
            })),
        );
        let response = dispatch(
            &services(env, transport),
            &request(
                "forge.complete",
                json!({
                    "provider": "groq",
                    "model": "llama-3.3-70b",
                    "prompt": "explain the ledger",
                    "evidence": ["docs/ledger.md:1 @ abc"]
                }),
            ),
        );
        let result = response.result.expect("result");
        assert_eq!(result["content"], json!("grounded answer"));
        assert_eq!(result["usage"]["prompt_tokens"], json!(4));
    }

    #[test]
    fn complete_requires_a_model() {
        let response = dispatch(
            &offline(),
            &request("forge.complete", json!({ "provider": "groq", "prompt": "x" })),
        );
        assert_eq!(response.error.expect("error").code, INVALID_REQUEST);
    }

    #[test]
    fn remotes_and_datasets_report_what_is_missing() {
        let remotes = dispatch(&offline(), &request("forge.remotes", json!({})))
            .result
            .expect("result");
        assert_eq!(remotes["count"], json!(3));
        assert_eq!(remotes["remotes"][0]["id"], json!("signage-docs"));
        assert_eq!(remotes["remotes"][0]["configured"], json!(false));

        let datasets = dispatch(&offline(), &request("forge.datasets", json!({})))
            .result
            .expect("result");
        assert_eq!(datasets["count"], json!(2));
        assert_eq!(datasets["datasets"][0]["id"], json!("signage-docs"));
        assert_eq!(datasets["datasets"][0]["missing"], json!(["HF_DATASET_REPO", "HF_DATASET_REVISION", "HF_READ_TOKEN"]));
    }

    #[test]
    fn remote_search_dispatches_and_dataset_search_grounds() {
        let mut env = Env::new();
        env.set("CRATE_INTEL_MODAL_SEARCH_URL", "https://crates.modal.run");
        env.set("CRATE_INTEL_MODAL_SEARCH_TOKEN", "tok");
        env.set("HF_DATASET_REPO", "owner/signage-docs");
        env.set("HF_DATASET_REVISION", "abc123");
        env.set("HF_READ_TOKEN", "hf_read");
        let transport = MockTransport::new()
            .route(
                "crates.modal.run/search",
                HttpResponse::json(json!([
                    {"score": 1.0, "source": "cratelab://crates/lab-manifest/src/lib.rs#L1",
                     "path": "crates/lab-manifest/src/lib.rs", "start_line": 1,
                     "revision": "abc123", "text": "pub struct Manifest {}"}
                ])),
            )
            .route(
                "huggingface.co",
                HttpResponse::text(
                    "{\"source\":\"project://simple-signage/docs/tv-mode.md#L20\",\"path\":\"docs/tv-mode.md\",\"start_line\":20,\"revision\":\"abc123\",\"text\":\"tv mode theming tokens\"}\n",
                ),
            );
        let services = services(env, transport);

        let remote = dispatch(
            &services,
            &request(
                "forge.remote.search",
                json!({ "remote": "crates-search", "query": "manifest schema", "scope": "lab-manifest" }),
            ),
        )
        .result
        .expect("result");
        assert_eq!(remote["count"], json!(1));
        assert_eq!(remote["hits"][0]["start_line"], json!(1));

        let dataset = dispatch(
            &services,
            &request(
                "forge.dataset.search",
                json!({ "dataset": "signage-docs", "query": "tv mode theming" }),
            ),
        )
        .result
        .expect("result");
        assert_eq!(dataset["query"], json!("tv mode theming"));
        assert_eq!(dataset["snippets"][0]["path"], json!("docs/tv-mode.md"));
    }

    #[test]
    fn search_methods_reject_unknown_targets_and_bad_params() {
        let unknown_remote = dispatch(
            &offline(),
            &request("forge.remote.search", json!({ "remote": "nope", "query": "x" })),
        );
        assert_eq!(unknown_remote.error.expect("error").code, INVALID_REQUEST);

        let unknown_dataset = dispatch(
            &offline(),
            &request("forge.dataset.search", json!({ "dataset": "nope", "query": "x" })),
        );
        assert_eq!(unknown_dataset.error.expect("error").code, INVALID_REQUEST);

        let no_query =
            dispatch(&offline(), &request("forge.dataset.search", json!({ "dataset": "signage-docs" })));
        assert_eq!(no_query.error.expect("error").code, INVALID_REQUEST);
    }

    #[test]
    fn the_ci_dashboard_names_the_missing_token() {
        let response = dispatch(&offline(), &request("forge.circleci", json!({})));
        assert_eq!(response.error.expect("error").code, NOT_CONFIGURED);
    }

    #[test]
    fn the_vector_layer_names_the_missing_endpoint() {
        let response = dispatch(&offline(), &request("forge.qdrant", json!({})));
        assert_eq!(response.error.expect("error").code, NOT_CONFIGURED);
    }
}
