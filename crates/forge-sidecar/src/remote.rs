//! Declared remote compute targets.
//!
//! Some forge work (embedding, large-scale evaluation) runs better off-box.
//! The sidecar keeps those *declared* here — id, kind and the environment
//! variable that holds the endpoint — while the actual assets (the Modal ASGI
//! app, the Kaggle kernel) live beside them under `remotes/`.
//!
//! Nothing secret is stored in this module or the repo: a [`Remote`] only names
//! the environment variable a credential or endpoint comes from.

use serde::{Deserialize, Serialize};

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

/// A declared remote. Endpoint and credentials come from `endpoint_env`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remote {
    /// Stable remote id.
    pub id: String,
    /// Which kind of remote this is.
    pub kind: RemoteKind,
    /// The environment variable holding the endpoint URL.
    pub endpoint_env: String,
}

impl Remote {
    /// The remotes the forge declares out of the box.
    ///
    /// These are descriptors only: no network is touched until a caller
    /// attaches a bridge and confirms the request.
    pub fn declared() -> Vec<Remote> {
        vec![
            Remote {
                id: "crates-search".into(),
                kind: RemoteKind::ModalAsgi,
                endpoint_env: "CRATE_INTEL_MODAL_SEARCH_URL".into(),
            },
            Remote {
                id: "retrieval-eval".into(),
                kind: RemoteKind::KaggleKernel,
                endpoint_env: "KAGGLE_KERNEL_ID".into(),
            },
        ]
    }

    /// Look up a declared remote by id.
    pub fn declared_by_id(id: &str) -> Option<Remote> {
        Self::declared().into_iter().find(|remote| remote.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_remotes_are_descriptors_only() {
        let remotes = Remote::declared();
        assert_eq!(remotes.len(), 2);
        assert!(remotes.iter().all(|remote| !remote.endpoint_env.is_empty()));
        let modal = Remote::declared_by_id("crates-search").expect("modal remote");
        assert_eq!(modal.kind, RemoteKind::ModalAsgi);
        assert!(Remote::declared_by_id("missing").is_none());
    }
}
