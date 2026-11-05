//! # forge-sidecar — the stdio bridge to the forge
//!
//! One process, one transport, no scatter. The sidecar exposes the forge over
//! newline-delimited JSON-RPC on stdio, so *any* host — the Tauri shell, a CLI,
//! an editor, an agent runtime — talks to the same workflows, the same
//! retrieval plane, the same pipeline and the same provider rungs instead of
//! each re-implementing them.
//!
//! ## Layout
//!
//! * [`rpc`] — the request/response envelope and the method router.
//! * [`services`] — the resolved environment and the transport every adapter
//!   borrows, injected so the router stays unit-testable without a socket.
//! * [`providers`] — the LLM rungs: endpoint table, fallback ladder, budget
//!   ceilings, circuit breakers and the [`forge_core::ProviderPort`] adapter.
//! * [`remote`] — the declared remote compute targets (Modal, Kaggle) and the
//!   bridge that dispatches to them.
//! * [`datasets`] — pinned private Hugging Face datasets, fetched into the
//!   retrieval plane as the store of record.
//! * [`circleci`] — the read-only CI gate dashboard.
//! * [`qdrant`] — the optional vector-serving layer, read-only.
//!
//! ## Credentials
//!
//! No secret is compiled in, committed, or embedded in a descriptor: every
//! adapter names the environment variable it needs (see [`env`]) and refuses
//! *before* any network use when it is missing.
//!
//! The binary ([`main`](../main/index.html)) is a thin loop: read a request,
//! dispatch it, write the response. All logic lives in the library so it is
//! testable without a subprocess.

pub mod circleci;
pub mod datasets;
pub mod env;
pub mod providers;
pub mod qdrant;
pub mod remote;
pub mod rpc;
pub mod services;
pub mod transport;

pub use circleci::circleci_dashboard;
pub use datasets::{DatasetPin, load_chunks, parse_jsonl, search_dataset};
pub use env::Env;
pub use providers::{
    CircuitBreaker, EndpointSpec, HttpProvider, Rung, budget_cap, chat_url, credential_env,
    is_llm, route_order, rung, rungs,
};
pub use qdrant::qdrant_collections;
pub use remote::{Remote, RemoteHit, RemoteKind, RemoteReadiness, RemoteSearchRequest, search_remote};
pub use rpc::{Request, Response, RpcError, dispatch};
pub use services::{LiveServices, Services};
pub use transport::{HttpResponse, HttpTransport, MockTransport, TransportError, UreqTransport};
