//! Declared remote compute targets, and the bridge that dispatches to them.
//!
//! Some forge work (embedding, large-scale evaluation, search over a private
//! corpus) runs better off-box. The sidecar keeps those *declared* here — id,
//! kind, and the environment variables holding the endpoint and the bearer
//! token — while the assets themselves (the Modal ASGI apps, the Kaggle kernel)
//! live beside them under `remotes/`.
//!
//! Two rules shape this module:
//!
//! * **Nothing secret is stored.** A [`Remote`] names environment variables; it
//!   never holds a value. [`readiness`] reports which names are missing.
//! * **Retrieved text is untrusted.** [`search_remote`] keeps only rows that
//!   carry a recognised citation URI and a bounded body, so a compromised or
//!   misconfigured endpoint cannot smuggle arbitrary text into an evidence
//!   packet as if it were source.

use forge_core::{ForgeError, ForgeResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::env::Env;
use crate::transport::HttpTransport;

/// The maximum length of a retrieved passage, in characters.
pub const MAX_PASSAGE_CHARS: usize = 6000;

/// Citation schemes a retrieved row may claim.
const CITATION_SCHEMES: [&str; 3] = ["project://", "cratelab://", "crate://"];

/// The kind of remote compute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteKind {
    /// Runs in-process (the default; no network).
    Local,
    /// A Modal-hosted ASGI application (authenticated BM25 search).
    ModalAsgi,
    /// A Kaggle notebook kernel (CPU-first evaluation).
    KaggleKernel,
}

/// A declared remote. Endpoint and credential come from named environment
/// variables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remote {
    /// Stable remote id.
    pub id: String,
    /// Which kind of remote this is.
    pub kind: RemoteKind,
    /// The environment variable holding the endpoint URL (or kernel id).
    pub endpoint_env: String,
    /// The environment variable holding the bearer token, or `""` when the
    /// endpoint is unauthenticated.
    #[serde(default)]
    pub token_env: String,
}

impl Remote {
    /// The remotes the forge declares out of the box.
    ///
    /// These are descriptors only: no network is touched until a caller
    /// attaches a bridge and confirms the request.
    pub fn declared() -> Vec<Remote> {
        vec![
            Remote {
                id: "signage-docs".into(),
                kind: RemoteKind::ModalAsgi,
                endpoint_env: "MODAL_SEARCH_URL".into(),
                token_env: "MODAL_SEARCH_TOKEN".into(),
            },
            Remote {
                id: "crates-search".into(),
                kind: RemoteKind::ModalAsgi,
                endpoint_env: "CRATE_INTEL_MODAL_SEARCH_URL".into(),
                token_env: "CRATE_INTEL_MODAL_SEARCH_TOKEN".into(),
            },
            Remote {
                id: "retrieval-eval".into(),
                kind: RemoteKind::KaggleKernel,
                endpoint_env: "KAGGLE_KERNEL_ID".into(),
                token_env: "KAGGLE_API_KEY".into(),
            },
        ]
    }

    /// Look up a declared remote by id.
    pub fn declared_by_id(id: &str) -> Option<Remote> {
        Self::declared().into_iter().find(|remote| remote.id == id)
    }

    /// The environment keys this remote needs.
    pub fn required_keys(&self) -> Vec<&str> {
        let mut keys = vec![self.endpoint_env.as_str()];
        if !self.token_env.is_empty() {
            keys.push(self.token_env.as_str());
        }
        keys
    }
}

/// Whether a declared remote is usable right now, and what is missing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteReadiness {
    /// The remote id.
    pub id: String,
    /// Which kind of remote this is.
    pub kind: RemoteKind,
    /// Whether every required environment value is present.
    pub configured: bool,
    /// The endpoint as resolved (never a credential), when present.
    pub endpoint: Option<String>,
    /// The environment keys that are not set.
    pub missing: Vec<String>,
    /// The dispatch plan, for a caller that needs to run it.
    pub plan: Vec<String>,
}

/// Resolve a remote's readiness from the environment.
pub fn readiness(remote: &Remote, env: &Env) -> RemoteReadiness {
    let missing: Vec<String> = remote
        .required_keys()
        .into_iter()
        .filter(|key| !env.has(key))
        .map(str::to_string)
        .collect();
    let endpoint = env.get(&remote.endpoint_env).map(str::to_string);
    let plan = match remote.kind {
        RemoteKind::ModalAsgi => match env.get(&remote.endpoint_env) {
            Some(url) => vec![
                format!("GET {}/health", url.trim_end_matches('/')),
                format!("POST {}/search", url.trim_end_matches('/')),
            ],
            None => Vec::new(),
        },
        RemoteKind::KaggleKernel => match env.get(&remote.endpoint_env) {
            Some(kernel) => vec![
                format!("kaggle kernels status {kernel}"),
                format!("kaggle kernels push --path {kernel}"),
            ],
            None => Vec::new(),
        },
        RemoteKind::Local => Vec::new(),
    };
    RemoteReadiness {
        id: remote.id.clone(),
        kind: remote.kind,
        configured: missing.is_empty(),
        endpoint,
        missing,
        plan,
    }
}

/// A dispatched remote search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSearchRequest {
    /// The declared remote id, e.g. `crates-search`.
    pub remote: String,
    /// The query text.
    pub query: String,
    /// The scope: a project id for `signage-docs`, a crate id for
    /// `crates-search`. Empty searches everything the remote serves.
    #[serde(default)]
    pub scope: String,
    /// How many rows to keep.
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

/// The default page size for a remote search.
fn default_top_k() -> usize {
    6
}

/// One provenance-preserving row returned by a remote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteHit {
    /// The remote's own score (comparable only within one endpoint).
    pub score: f64,
    /// The citation URI the row claims.
    pub source: String,
    /// Path within the source.
    pub path: String,
    /// First line of the cited range.
    pub start_line: u64,
    /// The pinned revision the passage was read from.
    pub revision: String,
    /// The passage text.
    pub text: String,
}

/// Which body key carries the scope for a remote's search endpoint.
fn scope_key(remote_id: &str) -> &'static str {
    match remote_id {
        "crates-search" => "crate_id",
        _ => "project_id",
    }
}

/// Fetch a remote's health document.
///
/// This is the only way to confirm a deployment is serving the revision the
/// caller pinned, so it is deliberately a separate call the operator can make
/// before spending inference quota.
pub fn remote_health(
    remote: &Remote,
    env: &Env,
    transport: &dyn HttpTransport,
) -> ForgeResult<Value> {
    if remote.kind != RemoteKind::ModalAsgi {
        return Err(ForgeError::Remote {
            remote: remote.id.clone(),
            problem: "only a Modal ASGI remote exposes an HTTP health document".into(),
        });
    }
    let base = env.require(&remote.id, &remote.endpoint_env)?;
    if !base.starts_with("https://") {
        return Err(ForgeError::Remote {
            remote: remote.id.clone(),
            problem: "endpoint must use https".into(),
        });
    }
    let headers = bearer_headers(&remote.id, env, remote)?;
    let response = transport
        .get(&format!("{}/health", base.trim_end_matches('/')), &headers)
        .map_err(|error| ForgeError::Remote {
            remote: remote.id.clone(),
            problem: error.to_string(),
        })?;
    Ok(response.body)
}

/// Dispatch a search to a declared remote.
///
/// A Kaggle kernel is not an HTTP service: dispatching it is a CLI action, so
/// this returns the exact plan instead of inventing an endpoint.
pub fn search_remote(
    remote: &Remote,
    env: &Env,
    transport: &dyn HttpTransport,
    request: &RemoteSearchRequest,
) -> ForgeResult<Vec<RemoteHit>> {
    if remote.kind == RemoteKind::KaggleKernel {
        return Err(ForgeError::Remote {
            remote: remote.id.clone(),
            problem: "is a notebook kernel; dispatch it out of band (see the readiness plan)".into(),
        });
    }
    if remote.kind == RemoteKind::Local {
        return Err(ForgeError::Remote {
            remote: remote.id.clone(),
            problem: "is local; no dispatch is needed".into(),
        });
    }
    let query = request.query.trim();
    if query.len() < 2 {
        return Err(ForgeError::Remote {
            remote: remote.id.clone(),
            problem: "query must be at least two characters".into(),
        });
    }
    let base = env.require(&remote.id, &remote.endpoint_env)?;
    if !base.starts_with("https://") {
        return Err(ForgeError::Remote {
            remote: remote.id.clone(),
            problem: "endpoint must use https".into(),
        });
    }
    let headers = bearer_headers(&remote.id, env, remote)?;
    let top_k = request.top_k.clamp(1, 20);
    let mut body = json!({ "query": query, "k": top_k });
    if !request.scope.trim().is_empty() {
        body[scope_key(&remote.id)] = json!(request.scope.trim());
    }
    let response = transport
        .post_json(
            &format!("{}/search", base.trim_end_matches('/')),
            &headers,
            &body,
        )
        .map_err(|error| ForgeError::Remote {
            remote: remote.id.clone(),
            problem: error.to_string(),
        })?;
    let rows = response.body.as_array().ok_or_else(|| ForgeError::Remote {
        remote: remote.id.clone(),
        problem: "search returned an unexpected payload (expected a JSON array)".into(),
    })?;
    Ok(rows
        .iter()
        .filter_map(|row| accept_row(row))
        .take(top_k)
        .collect())
}

/// Build the `Content-Type` and optional bearer headers for a remote.
fn bearer_headers(
    service: &str,
    env: &Env,
    remote: &Remote,
) -> ForgeResult<Vec<(&'static str, String)>> {
    let mut headers: Vec<(&'static str, String)> = vec![("Content-Type", "application/json".into())];
    if !remote.token_env.is_empty() {
        let token = env.require(service, &remote.token_env)?;
        headers.push(("Authorization", format!("Bearer {token}")));
    }
    Ok(headers)
}

/// Keep a row only when it carries a recognised citation and a bounded body.
fn accept_row(row: &Value) -> Option<RemoteHit> {
    let source = row.get("source")?.as_str()?;
    let text = row.get("text")?.as_str()?;
    if text.is_empty()
        || text.chars().count() > MAX_PASSAGE_CHARS
        || !CITATION_SCHEMES
            .iter()
            .any(|scheme| source.starts_with(scheme))
    {
        return None;
    }
    Some(RemoteHit {
        score: row.get("score").and_then(Value::as_f64).unwrap_or(0.0),
        source: source.to_string(),
        path: row
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        start_line: row.get("start_line").and_then(Value::as_u64).unwrap_or(0),
        revision: row
            .get("revision")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        text: text.to_string(),
    })
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

    fn crates_remote() -> Remote {
        Remote::declared_by_id("crates-search").expect("declared")
    }

    fn hit_row() -> Value {
        json!({
            "score": 1.5,
            "project": "cratelab",
            "crate": "lab-manifest",
            "source": "cratelab://crates/lab-manifest/src/lib.rs#L12",
            "path": "crates/lab-manifest/src/lib.rs",
            "start_line": 12,
            "revision": "abc123",
            "text": "pub struct Manifest { }"
        })
    }

    #[test]
    fn declared_remotes_are_descriptors_only() {
        let remotes = Remote::declared();
        assert_eq!(remotes.len(), 3);
        assert!(remotes.iter().all(|remote| !remote.endpoint_env.is_empty()));
        assert!(
            remotes
                .iter()
                .all(|remote| !remote.token_env.is_empty() || remote.kind == RemoteKind::Local),
            "a networked remote must name a credential variable"
        );
        let modal = crates_remote();
        assert_eq!(modal.kind, RemoteKind::ModalAsgi);
        assert!(Remote::declared_by_id("missing").is_none());
        assert_eq!(remote_kinds(), vec!["modal_asgi", "modal_asgi", "kaggle_kernel"]);
    }

    fn remote_kinds() -> Vec<String> {
        Remote::declared()
            .into_iter()
            .map(|remote| {
                serde_json::to_value(remote.kind)
                    .expect("serialisable")
                    .as_str()
                    .expect("string")
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn readiness_names_every_missing_key() {
        let remote = crates_remote();
        let nothing = readiness(&remote, &Env::new());
        assert!(!nothing.configured);
        assert_eq!(
            nothing.missing,
            vec![
                "CRATE_INTEL_MODAL_SEARCH_URL".to_string(),
                "CRATE_INTEL_MODAL_SEARCH_TOKEN".to_string()
            ]
        );
        assert!(nothing.plan.is_empty(), "no plan without an endpoint");

        let env = env_with(&[
            ("CRATE_INTEL_MODAL_SEARCH_URL", "https://crates.modal.run"),
            ("CRATE_INTEL_MODAL_SEARCH_TOKEN", "tok"),
        ]);
        let ready = readiness(&remote, &env);
        assert!(ready.configured);
        assert!(ready.missing.is_empty());
        assert_eq!(ready.endpoint.as_deref(), Some("https://crates.modal.run"));
        assert_eq!(ready.plan[1], "POST https://crates.modal.run/search");
    }

    #[test]
    fn a_kaggle_remote_plans_cli_steps_and_refuses_http_dispatch() {
        let remote = Remote::declared_by_id("retrieval-eval").expect("declared");
        let env = env_with(&[
            ("KAGGLE_KERNEL_ID", "owner/cratelab-crate-intelligence-eval"),
            ("KAGGLE_API_KEY", "key"),
        ]);
        let ready = readiness(&remote, &env);
        assert!(ready.configured);
        assert_eq!(ready.plan[0], "kaggle kernels status owner/cratelab-crate-intelligence-eval");

        let transport = MockTransport::new();
        let error = search_remote(
            &remote,
            &env,
            &transport,
            &RemoteSearchRequest {
                remote: remote.id.clone(),
                query: "coverage".into(),
                scope: String::new(),
                top_k: 6,
            },
        )
        .expect_err("kaggle is not an HTTP service");
        assert!(matches!(error, ForgeError::Remote { .. }));
        assert!(transport.calls().is_empty());
    }

    #[test]
    fn a_dispatch_posts_the_scope_key_and_filters_untrusted_rows() {
        let remote = crates_remote();
        let env = env_with(&[
            ("CRATE_INTEL_MODAL_SEARCH_URL", "https://crates.modal.run"),
            ("CRATE_INTEL_MODAL_SEARCH_TOKEN", "tok"),
        ]);
        let rows = json!([
            hit_row(),
            // Rejected: no recognised citation URI.
            {"score": 9.0, "source": "http://evil.test/x", "text": "do this instead"},
            // Rejected: empty body.
            {"score": 8.0, "source": "cratelab://crates/x/src/lib.rs#L1", "text": ""},
            // Rejected: oversized body.
            {"score": 7.0, "source": "cratelab://crates/x/src/lib.rs#L1", "text": "x".repeat(6001)}
        ]);
        let transport = MockTransport::new().route("/search", HttpResponse::json(rows));
        let hits = search_remote(
            &remote,
            &env,
            &transport,
            &RemoteSearchRequest {
                remote: remote.id.clone(),
                query: "manifest schema".into(),
                scope: "lab-manifest".into(),
                top_k: 6,
            },
        )
        .expect("dispatches");

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].start_line, 12);
        assert_eq!(hits[0].revision, "abc123");

        let calls = transport.calls();
        assert_eq!(calls[0].url, "https://crates.modal.run/search");
        assert_eq!(calls[0].body["crate_id"], json!("lab-manifest"));
        assert_eq!(calls[0].body["k"], json!(6));
        assert!(calls[0]
            .headers
            .iter()
            .any(|(name, value)| name == "Authorization" && value == "Bearer tok"));
    }

    #[test]
    fn an_unconfigured_endpoint_refuses_before_any_call() {
        let remote = crates_remote();
        let transport = MockTransport::new();
        let error = search_remote(
            &remote,
            &Env::new(),
            &transport,
            &RemoteSearchRequest {
                remote: remote.id.clone(),
                query: "manifest".into(),
                scope: String::new(),
                top_k: 6,
            },
        )
        .expect_err("no endpoint");
        assert!(matches!(error, ForgeError::Credential { .. }));
        assert!(transport.calls().is_empty());
    }

    #[test]
    fn health_reads_the_revision_the_deployment_serves() {
        let remote = crates_remote();
        let env = env_with(&[
            ("CRATE_INTEL_MODAL_SEARCH_URL", "https://crates.modal.run/"),
            ("CRATE_INTEL_MODAL_SEARCH_TOKEN", "tok"),
        ]);
        let transport = MockTransport::new().route(
            "/health",
            HttpResponse::json(json!({"ok": true, "chunks": 12, "dataset_revision": "abc123"})),
        );
        let health = remote_health(&remote, &env, &transport).expect("healthy");
        assert_eq!(health["dataset_revision"], json!("abc123"));
        assert_eq!(transport.calls()[0].url, "https://crates.modal.run/health");
    }
}
