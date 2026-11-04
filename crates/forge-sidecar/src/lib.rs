//! # forge-sidecar — the stdio bridge to the forge
//!
//! One process, one transport, no scatter. The sidecar exposes the forge over
//! newline-delimited JSON-RPC on stdio, so *any* host — the Tauri shell, a CLI,
//! an editor, an agent runtime — talks to the same workflows, the same
//! retrieval plane and the same pipeline, instead of each re-implementing them.
//!
//! * [`rpc`] — the request/response envelope and the method router.
//! * [`remote`] — the declared remote compute targets (Modal, Kaggle) the
//!   sidecar can bridge to. Endpoints and credentials come from the
//!   environment; nothing secret is ever embedded here.
//!
//! The binary ([`main`](../main/index.html)) is a thin loop: read a request,
//! [`rpc::dispatch`] it, write the response. All logic lives in the library so
//! it is testable without a subprocess.

pub mod remote;
pub mod rpc;

pub use remote::{Remote, RemoteKind};
pub use rpc::{dispatch, Request, Response, RpcError};
