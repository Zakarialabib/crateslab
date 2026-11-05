//! The forge error vocabulary — one taxonomy, shared with the lab.
//!
//! The forge does not invent a second error language. Every failure it can
//! produce maps back onto the lab's [`LabErrorKind`], [`Remedy`] and
//! [`Severity`], so the Tauri UI, the coverage ledger and the `lab gate` CLI
//! keep branching on the *same* small set of kinds.
//!
//! Invariant (inherited from the lab): **refusals are data.** A stage that
//! declines to act yields a [`crate::stage::Refusal`] with [`Severity::Info`]
//! and exit code `0` — never a panic, never a generic 500.

use lab_errors::{LabError, LabErrorKind, Remedy, Severity};
use serde::{Deserialize, Serialize};

/// A forge failure, always classifiable into the lab's vocabulary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
pub enum ForgeError {
    /// A lab error surfaced through the forge boundary.
    #[error(transparent)]
    Lab(#[from] LabError),

    /// The workflow or task registry, or one of its entries, is malformed.
    #[error("workflow '{id}': {problem}")]
    Workflow {
        /// Workflow id, or `<registry>` for whole-file problems.
        id: String,
        /// What is wrong.
        problem: String,
    },

    /// A pipeline stage failed or refused.
    #[error("stage '{stage}': {problem}")]
    Pipeline {
        /// Stage id.
        stage: String,
        /// What is wrong.
        problem: String,
    },

    /// Retrieval over the knowledge base failed.
    #[error("retrieval for '{query}' failed: {problem}")]
    Retrieval {
        /// The query that failed.
        query: String,
        /// What is wrong.
        problem: String,
    },

    /// A remote (Modal/Kaggle) bridge call failed.
    #[error("remote '{remote}': {problem}")]
    Remote {
        /// Remote id.
        remote: String,
        /// What is wrong.
        problem: String,
    },

    /// A service needs a credential that is not configured.
    ///
    /// This is deliberately its own class: the remedy is not a code change but
    /// a settings action, so the UI can offer a setup link instead of a
    /// generic failure.
    #[error("{service} is not configured: set {key}")]
    Credential {
        /// The service or provider id, e.g. `groq`.
        service: String,
        /// The environment variable that must hold the credential.
        key: String,
    },
}

impl ForgeError {
    /// The coarse kind, in the lab's vocabulary.
    pub fn kind(&self) -> LabErrorKind {
        match self {
            ForgeError::Lab(error) => error.kind(),
            ForgeError::Workflow { .. } => LabErrorKind::Manifest,
            ForgeError::Pipeline { .. } => LabErrorKind::Scenario,
            ForgeError::Retrieval { .. } => LabErrorKind::Discovery,
            ForgeError::Remote { .. } => LabErrorKind::Transport,
            ForgeError::Credential { .. } => LabErrorKind::Manifest,
        }
    }

    /// What the caller (UI, pipeline engine, CI gate) should do next.
    pub fn remedy(&self) -> Remedy {
        match self {
            ForgeError::Lab(error) => error.remedy(),
            ForgeError::Workflow { .. } => Remedy::FixManifest,
            ForgeError::Pipeline { .. } => Remedy::FixScenario,
            ForgeError::Retrieval { .. } => Remedy::Retry,
            ForgeError::Remote { .. } => Remedy::Abort,
            ForgeError::Credential { .. } => Remedy::FixManifest,
        }
    }

    /// Severity, driving UI colour and CI gating.
    pub fn severity(&self) -> Severity {
        match self {
            ForgeError::Lab(error) => error.severity(),
            ForgeError::Retrieval { .. } => Severity::Warn,
            ForgeError::Remote { .. } => Severity::Fatal,
            ForgeError::Credential { .. } => Severity::Warn,
            ForgeError::Workflow { .. } | ForgeError::Pipeline { .. } => Severity::Warn,
        }
    }

    /// Exit code for the headless CLI, consistent with `LabError::exit_code`.
    pub fn exit_code(&self) -> i32 {
        match self.severity() {
            Severity::Info => 0,
            Severity::Warn => 2,
            Severity::Fatal => 3,
        }
    }
}

/// Convenience alias for results inside the forge.
pub type ForgeResult<T> = Result<T, ForgeError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forge_kinds_map_into_the_lab_vocabulary() {
        let workflow = ForgeError::Workflow {
            id: "tv-rs-evolution".into(),
            problem: "no stages".into(),
        };
        assert_eq!(workflow.kind(), LabErrorKind::Manifest);
        assert_eq!(workflow.remedy(), Remedy::FixManifest);
        assert_eq!(workflow.severity(), Severity::Warn);
        assert_eq!(workflow.exit_code(), 2);

        let remote = ForgeError::Remote {
            remote: "modal-search".into(),
            problem: "unreachable".into(),
        };
        assert_eq!(remote.kind(), LabErrorKind::Transport);
        assert_eq!(remote.severity(), Severity::Fatal);
        assert_eq!(remote.exit_code(), 3);

        let credential = ForgeError::Credential {
            service: "groq".into(),
            key: "GROQ_API_KEY".into(),
        };
        assert_eq!(credential.kind(), LabErrorKind::Manifest);
        assert_eq!(credential.remedy(), Remedy::FixManifest);
        assert_eq!(credential.severity(), Severity::Warn);
        assert_eq!(credential.exit_code(), 2);
        assert!(credential.to_string().contains("GROQ_API_KEY"));
    }

    #[test]
    fn lab_errors_are_transparent() {
        let lab = LabError::Capability {
            device: "tv-192.168.1.40".into(),
            command: "tv.panel.orientation".into(),
        };
        let forge: ForgeError = lab.into();
        // A capability refusal stays an info-level refusal through the bridge.
        assert_eq!(forge.severity(), Severity::Info);
        assert_eq!(forge.exit_code(), 0);
    }
}
