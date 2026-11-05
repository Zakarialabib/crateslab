//! The one place the sidecar touches the network.
//!
//! Every service adapter — LLM rungs, Modal, Kaggle, Hugging Face, CircleCI,
//! Qdrant — is written against [`HttpTransport`] rather than against `ureq`
//! directly. That keeps each adapter unit-testable with [`MockTransport`] (no
//! sockets, no credentials) while the binary still owns exactly one real
//! client, [`UreqTransport`].
//!
//! Nothing here logs or echoes request headers, so credentials never reach a
//! response body, an error message or a test assertion.

use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

/// How much of an error body is surfaced. Enough to diagnose, too little to
/// leak a payload.
const MAX_ERROR_BODY: usize = 400;

/// A minimal HTTP reply: the status and an already-read body.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpResponse {
    /// The HTTP status code.
    pub status: u16,
    /// The parsed JSON body, or `Value::String` when the body was not JSON.
    pub body: Value,
}

impl HttpResponse {
    /// A `200 OK` reply carrying a JSON body.
    pub fn json(body: Value) -> Self {
        Self { status: 200, body }
    }

    /// A `200 OK` reply carrying a raw text body (e.g. a JSONL dataset).
    pub fn text(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            body: Value::String(body.into()),
        }
    }

    /// The body as a string, when it is textual.
    pub fn as_str(&self) -> Option<&str> {
        self.body.as_str()
    }

    /// A JSON pointer into the body, e.g. `/choices/0/message/content`.
    pub fn pointer(&self, path: &str) -> Option<&Value> {
        self.body.pointer(path)
    }
}

/// Why a transport call failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    /// The server answered with a non-2xx status.
    #[error("HTTP {status}: {body}")]
    Status {
        /// The status code returned.
        status: u16,
        /// A truncated excerpt of the response body.
        body: String,
    },
    /// The request never produced a usable reply (DNS, TLS, timeout, reset).
    #[error("network or TLS failure: {0}")]
    Network(String),
}

/// A blocking HTTP client the sidecar can be handed.
///
/// Implementations must be `Send + Sync`: one transport is shared by every
/// adapter in a process.
pub trait HttpTransport: Send + Sync {
    /// Issue a GET.
    fn get(&self, url: &str, headers: &[(&str, String)]) -> Result<HttpResponse, TransportError>;

    /// Issue a JSON POST.
    fn post_json(
        &self,
        url: &str,
        headers: &[(&str, String)],
        body: &Value,
    ) -> Result<HttpResponse, TransportError>;
}

/// The real client: `ureq` with a bounded per-request timeout.
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    /// A client whose every request times out after `timeout`.
    pub fn new(timeout: Duration) -> Self {
        Self {
            agent: ureq::AgentBuilder::new().timeout(timeout).build(),
        }
    }
}

impl HttpTransport for UreqTransport {
    fn get(&self, url: &str, headers: &[(&str, String)]) -> Result<HttpResponse, TransportError> {
        let mut request = self.agent.get(url);
        for (name, value) in headers {
            request = request.set(name, value);
        }
        read(request.call())
    }

    fn post_json(
        &self,
        url: &str,
        headers: &[(&str, String)],
        body: &Value,
    ) -> Result<HttpResponse, TransportError> {
        let mut request = self.agent.post(url);
        for (name, value) in headers {
            request = request.set(name, value);
        }
        read(request.send_json(body))
    }
}

/// Read a `ureq` outcome into an [`HttpResponse`], truncating error bodies.
fn read(outcome: Result<ureq::Response, ureq::Error>) -> Result<HttpResponse, TransportError> {
    match outcome {
        Ok(response) => {
            let status = response.status();
            let raw = response.into_string().unwrap_or_default();
            let body = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
            Ok(HttpResponse { status, body })
        }
        Err(ureq::Error::Status(status, response)) => Err(TransportError::Status {
            status,
            body: truncate(&response.into_string().unwrap_or_default()),
        }),
        Err(ureq::Error::Transport(error)) => Err(TransportError::Network(truncate(&error.to_string()))),
    }
}

/// Keep only the first [`MAX_ERROR_BODY`] characters, on a character boundary.
fn truncate(text: &str) -> String {
    text.chars().take(MAX_ERROR_BODY).collect()
}

/// One recorded call, for assertions.
#[derive(Debug, Clone, PartialEq)]
pub struct MockCall {
    /// `GET` or `POST`.
    pub method: String,
    /// The full URL the adapter requested.
    pub url: String,
    /// The headers sent, as `(name, value)` pairs.
    pub headers: Vec<(String, String)>,
    /// The JSON body sent (null for GET).
    pub body: Value,
}

/// A canned transport for tests: routes by URL substring, records every call.
///
/// It mirrors the real client's most important property: a non-2xx status is
/// returned as [`TransportError::Status`], not as a body, so error handling is
/// exercised rather than skipped.
#[derive(Default)]
pub struct MockTransport {
    routes: Vec<(String, HttpResponse)>,
    calls: Mutex<Vec<MockCall>>,
}

impl MockTransport {
    /// An empty mock with no routes.
    pub fn new() -> Self {
        Self::default()
    }

    /// Answer any URL containing `url_contains` with `response`.
    pub fn route(mut self, url_contains: impl Into<String>, response: HttpResponse) -> Self {
        self.routes.push((url_contains.into(), response));
        self
    }

    /// Every call made, in order.
    pub fn calls(&self) -> Vec<MockCall> {
        self.calls
            .lock()
            .map(|calls| calls.clone())
            .unwrap_or_default()
    }

    fn respond(
        &self,
        method: &str,
        url: &str,
        headers: &[(&str, String)],
        body: Value,
    ) -> Result<HttpResponse, TransportError> {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(MockCall {
                method: method.to_string(),
                url: url.to_string(),
                headers: headers
                    .iter()
                    .map(|(name, value)| ((*name).to_string(), value.clone()))
                    .collect(),
                body,
            });
        }
        self.routes
            .iter()
            .find(|(needle, _)| url.contains(needle.as_str()))
            .map(|(_, response)| response.clone())
            .ok_or_else(|| TransportError::Network(format!("no mock route for {method} {url}")))
            .and_then(|response| {
                if response.status >= 400 {
                    let body = response
                        .body
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| response.body.to_string());
                    Err(TransportError::Status {
                        status: response.status,
                        body: truncate(&body),
                    })
                } else {
                    Ok(response)
                }
            })
    }
}

impl HttpTransport for MockTransport {
    fn get(&self, url: &str, headers: &[(&str, String)]) -> Result<HttpResponse, TransportError> {
        self.respond("GET", url, headers, Value::Null)
    }

    fn post_json(
        &self,
        url: &str,
        headers: &[(&str, String)],
        body: &Value,
    ) -> Result<HttpResponse, TransportError> {
        self.respond("POST", url, headers, body.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn mock_routes_by_url_substring_and_records_calls() {
        let transport = MockTransport::new().route("/search", HttpResponse::json(json!([1, 2])));
        let response = transport
            .post_json("https://example.modal.run/search", &[], &json!({"query": "x"}))
            .expect("routed");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, json!([1, 2]));

        let calls = transport.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method, "POST");
        assert_eq!(calls[0].body["query"], json!("x"));
    }

    #[test]
    fn an_unrouted_call_is_a_network_error_not_a_panic() {
        let transport = MockTransport::new();
        let error = transport.get("https://example.test/x", &[]).expect_err("unrouted");
        assert!(matches!(error, TransportError::Network(_)));
    }

    #[test]
    fn non_json_bodies_are_kept_as_text() {
        let response = HttpResponse::text("{\"id\":1}\n{\"id\":2}\n");
        assert_eq!(response.as_str(), Some("{\"id\":1}\n{\"id\":2}\n"));
        assert!(HttpResponse::json(json!({"a": 1})).as_str().is_none());
    }

    #[test]
    fn error_bodies_are_truncated() {
        assert_eq!(truncate(&"x".repeat(1000)).chars().count(), MAX_ERROR_BODY);
    }
}
