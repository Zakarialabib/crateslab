//! The optional read-only Qdrant serving layer.
//!
//! Qdrant is deliberately *not* the store of record. The pinned Hugging Face
//! dataset ([`crate::datasets`]) is what the forge cites; Qdrant is the vector
//! layer an operator may stand up in front of it for faster/neural search. So
//! every call here is a `GET` and nothing is ever written, re-indexed or
//! deleted from this sidecar.
//!
//! Credentials are named, never stored: `QDRANT_URL` is the deployment base and
//! `QDRANT_API_KEY` (optional for a loopback server) is sent as `api-key`.

use forge_core::{ForgeError, ForgeResult};
use serde_json::{json, Value};

use crate::env::Env;
use crate::providers::is_loopback;
use crate::transport::HttpTransport;

/// The collection names a Qdrant deployment serves.
///
/// Returns an endpoint summary rather than a raw payload: the caller wants to
/// know *which* collections exist, not the whole Qdrant envelope.
pub fn qdrant_collections(env: &Env, transport: &dyn HttpTransport) -> ForgeResult<Value> {
    let base = env.require("qdrant", "QDRANT_URL")?;
    if !base.starts_with("https://") && !is_loopback(base) {
        return Err(ForgeError::Remote {
            remote: "qdrant".to_string(),
            problem: "endpoint must use https (loopback is allowed for a local server)".into(),
        });
    }
    let base = base.trim_end_matches('/');

    let mut headers: Vec<(&str, String)> = vec![("Accept", "application/json".to_string())];
    if let Some(key) = env.get("QDRANT_API_KEY") {
        headers.push(("api-key", key.to_string()));
    }

    let response = transport
        .get(&format!("{base}/collections"), &headers)
        .map_err(|error| ForgeError::Remote {
            remote: "qdrant".to_string(),
            problem: error.to_string(),
        })?;

    let collections: Vec<&str> = response
        .pointer("/result/collections")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.get("name").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();

    Ok(json!({
        "endpoint": base,
        "count": collections.len(),
        "collections": collections,
    }))
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

    #[test]
    fn a_missing_endpoint_refuses_before_any_network_use() {
        let transport = MockTransport::new();
        let error = qdrant_collections(&Env::new(), &transport).expect_err("no endpoint");
        assert!(matches!(error, ForgeError::Credential { ref key, .. } if key == "QDRANT_URL"));
        assert!(transport.calls().is_empty());
    }

    #[test]
    fn a_plaintext_remote_endpoint_is_refused() {
        let env = env_with(&[("QDRANT_URL", "http://qdrant.example.test")]);
        let transport = MockTransport::new();
        let error = qdrant_collections(&env, &transport).expect_err("insecure");
        assert!(matches!(error, ForgeError::Remote { .. }));
        assert!(transport.calls().is_empty());
    }

    #[test]
    fn collections_are_listed_with_the_api_key_header() {
        let env = env_with(&[
            ("QDRANT_URL", "https://qdrant.example.test/"),
            ("QDRANT_API_KEY", "qk_test"),
        ]);
        let transport = MockTransport::new().route(
            "/collections",
            HttpResponse::json(json!({"result": {"collections": [
                {"name": "crate-intelligence"},
                {"name": "signage-docs"}
            ]}})),
        );

        let summary = qdrant_collections(&env, &transport).expect("reads");
        assert_eq!(summary["count"], 2);
        assert_eq!(summary["collections"][0], "crate-intelligence");
        // A trailing slash on the base must not double up.
        assert_eq!(summary["endpoint"], "https://qdrant.example.test");

        let calls = transport.calls();
        assert_eq!(calls[0].url, "https://qdrant.example.test/collections");
        assert!(calls[0]
            .headers
            .iter()
            .any(|(name, value)| name == "api-key" && value == "qk_test"));
    }

    #[test]
    fn a_loopback_server_needs_no_api_key() {
        let env = env_with(&[("QDRANT_URL", "http://127.0.0.1:6333")]);
        let transport = MockTransport::new().route(
            "/collections",
            HttpResponse::json(json!({"result": {"collections": []}})),
        );
        let summary = qdrant_collections(&env, &transport).expect("reads");
        assert_eq!(summary["count"], 0);
        assert!(!transport.calls()[0]
            .headers
            .iter()
            .any(|(name, _)| name == "api-key"));
    }

    #[test]
    fn a_non_qdrant_payload_is_an_empty_list_not_a_panic() {
        let env = env_with(&[("QDRANT_URL", "https://qdrant.example.test")]);
        let transport = MockTransport::new().route("/collections", HttpResponse::text("nope"));
        let summary = qdrant_collections(&env, &transport).expect("reads");
        assert_eq!(summary["count"], 0);
    }
}
