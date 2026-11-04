//! The JSON-RPC envelope and method router.
//!
//! Requests are newline-delimited JSON objects on stdin; responses are
//! newline-delimited JSON objects on stdout. The router is pure and
//! deterministic — it performs no I/O, so every method is unit-testable.

use forge_core::WorkflowRegistry;
use forge_rag::tokenize;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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
    /// One of the [`INVALID_REQUEST`], [`METHOD_NOT_FOUND`] or [`INTERNAL_ERROR`] codes.
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

/// The request could not be parsed.
pub const INVALID_REQUEST: i32 = -32600;
/// The method is not implemented.
pub const METHOD_NOT_FOUND: i32 = -32601;
/// The method failed.
pub const INTERNAL_ERROR: i32 = -32603;

/// Route a request to a forge capability. Pure and deterministic.
///
/// Methods: `forge.ping`, `forge.workflows`, `forge.tokenize`.
pub fn dispatch(request: &Request) -> Response {
    match request.method.as_str() {
        "forge.ping" => Response::ok(request.id, json!({ "pong": true })),

        "forge.workflows" => match forge_core::embedded() {
            Ok(registry) => Response::ok(request.id, registry_json(&registry)),
            Err(error) => Response::err(request.id, INTERNAL_ERROR, error.to_string()),
        },

        "forge.tokenize" => match request.params.get("text").and_then(Value::as_str) {
            Some(text) => Response::ok(request.id, json!({ "tokens": tokenize(text) })),
            None => Response::err(request.id, INVALID_REQUEST, "missing string param 'text'"),
        },

        other => Response::err(
            request.id,
            METHOD_NOT_FOUND,
            format!("unknown method '{other}'"),
        ),
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

    fn request(method: &str, params: Value) -> Request {
        Request {
            id: 7,
            method: method.into(),
            params,
        }
    }

    #[test]
    fn ping_pongs_with_the_same_id() {
        let response = dispatch(&request("forge.ping", json!({})));
        assert_eq!(response.id, 7);
        assert_eq!(response.result, Some(json!({ "pong": true })));
        assert!(response.error.is_none());
    }

    #[test]
    fn workflows_serves_the_embedded_registry() {
        let response = dispatch(&request("forge.workflows", json!({})));
        let result = response.result.expect("result");
        let count = result.get("count").and_then(Value::as_u64).expect("count");
        assert!(count >= 4);
    }

    #[test]
    fn tokenize_needs_a_text_param() {
        let ok = dispatch(&request("forge.tokenize", json!({ "text": "Coverage_Ledger" })));
        assert_eq!(ok.result, Some(json!({ "tokens": ["coverage_ledger"] })));

        let missing = dispatch(&request("forge.tokenize", json!({})));
        assert_eq!(missing.error.expect("error").code, INVALID_REQUEST);
    }

    #[test]
    fn unknown_methods_are_reported_not_panicked() {
        let response = dispatch(&request("forge.nope", json!({})));
        assert_eq!(response.error.expect("error").code, METHOD_NOT_FOUND);
    }
}
