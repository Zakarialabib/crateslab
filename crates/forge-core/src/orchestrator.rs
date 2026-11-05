//! The agentic orchestrator — the state machine and its self-healing loop.
//!
//! A [`ForgeContext`] is the accumulating record of one agentic run: the goal,
//! the evidence retrieval handed it, the proposal currently on the bench, and
//! the ordered [`ForgeStep`] journal of everything that happened. The loop is
//!
//! ```text
//! Planning → Retrieving → Drafting → Validating ─┬─► Completed
//!                                                └─► Healing ─► Drafting …
//! ```
//!
//! `Validating` is where the compiler, clippy and the Rhai scenarios speak:
//! every way a proposal can be rejected is a [`ValidationError`], which is
//! handed straight back to the generator as its next prompt instead of being
//! flattened into a string. `Healing` counts one retry; once
//! [`ForgeContext::can_retry`] is false the run records `Failed` and stops.
//!
//! ## Auditability
//!
//! Every step stores a content digest of the context that produced it and, when
//! a proposal was on the bench, a digest of that proposal. Two runs with the
//! same goal and the same evidence therefore yield the same
//! `input_context_hash`, which is what makes a journal diffable.

use forge_protocol::ProposalArtifact;
use forge_rag::EvidencePacket;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use thiserror::Error;

/// Typed errors the validator hands back to the generator.
///
/// Each variant is a *different instruction* to the next generation, so the
/// loop can quote the compiler verbatim rather than paraphrasing it.
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum ValidationError {
    /// `cargo check` rejected the proposal; `stderr` is quoted verbatim.
    #[error("Cargo check failed: {stderr}")]
    CompilationFailed {
        /// The compiler's own output.
        stderr: String,
    },
    /// Clippy rejected the proposal.
    #[error("Clippy lint violated: {message}")]
    ClippyViolation {
        /// The lint message, with its location.
        message: String,
    },
    /// The proposal does not satisfy the protocol schema.
    #[error("Manifest schema violation: {field} is missing or invalid")]
    SchemaViolation {
        /// The offending field.
        field: String,
    },
    /// A Rhai scenario asserted against the proposal and failed.
    #[error("Rhai scenario test failed: {output}")]
    ScenarioFailed {
        /// The scenario runner's own output.
        output: String,
    },
}

/// Where a run is in the loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgeStateName {
    /// Working out what to do.
    Planning,
    /// Retrieving grounded evidence.
    Retrieving,
    /// Drafting a proposal.
    Drafting,
    /// Compiling, linting and running scenarios against the draft.
    Validating,
    /// Re-drafting from the validator's feedback.
    Healing,
    /// Done, with a reviewed proposal.
    Completed,
    /// Out of retries, or abandoned.
    Failed,
}

/// The immutable record of a single step in the loop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeStep {
    /// Which state the run was in.
    pub state: ForgeStateName,
    /// Unix seconds at which the step was recorded.
    pub timestamp: u64,
    /// Digest of the context this step was taken from.
    pub input_context_hash: String,
    /// Digest of the proposal on the bench, when there was one.
    pub output_artifact_hash: Option<String>,
    /// The validator's rejection, when the step carried one.
    pub error: Option<ValidationError>,
}

/// The accumulating context passed between the loop's states.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ForgeContext {
    /// What the run is trying to achieve.
    pub goal: String,
    /// The journal, oldest first.
    pub history: Vec<ForgeStep>,
    /// The evidence every prompt is grounded on.
    pub retrieved_evidence: Vec<EvidencePacket>,
    /// The proposal currently on the bench.
    pub current_proposal: Option<ProposalArtifact>,
    /// Healing attempts made so far.
    pub retry_count: u8,
    /// The ceiling on [`ForgeContext::retry_count`].
    pub max_retries: u8,
}

impl ForgeContext {
    /// A fresh context for `goal`, allowed `max_retries` healing attempts.
    pub fn new(goal: impl Into<String>, max_retries: u8) -> Self {
        Self {
            goal: goal.into(),
            max_retries,
            ..Self::default()
        }
    }

    /// Append a step to the journal, hashing the context that produced it.
    ///
    /// Reaching [`ForgeStateName::Healing`] counts one retry against
    /// [`ForgeContext::max_retries`].
    pub fn record_step(&mut self, state: ForgeStateName, error: Option<ValidationError>) {
        if state == ForgeStateName::Healing {
            self.retry_count = self.retry_count.saturating_add(1);
        }

        let input_context_hash = self.context_digest();
        let output_artifact_hash = self.current_proposal.as_ref().map(proposal_digest);

        self.history.push(ForgeStep {
            state,
            timestamp: now_secs(),
            input_context_hash,
            output_artifact_hash,
            error,
        });
    }

    /// Whether another healing attempt is allowed.
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    /// A stable digest of everything that feeds the next generation.
    fn context_digest(&self) -> String {
        let proposal = self
            .current_proposal
            .as_ref()
            .map(|proposal| proposal.crate_name.as_str())
            .unwrap_or("");
        hash_str(&format!(
            "{}\nsteps={}\nretries={}\nproposal={}",
            self.goal,
            self.history.len(),
            self.retry_count,
            proposal
        ))
    }
}

/// Unix seconds, saturating at `0` for a clock before the epoch.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

/// A SHA-256 digest of a proposal's full JSON form.
fn proposal_digest(proposal: &ProposalArtifact) -> String {
    hash_str(&serde_json::to_string(proposal).unwrap_or_default())
}

/// Lower-case hex SHA-256 of `input`.
fn hash_str(input: &str) -> String {
    hex(&Sha256::digest(input.as_bytes()))
}

/// Lower-case hex of a byte slice.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Writing into a `String` is infallible.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_protocol::{CargoTomlPatch, FileAction, FileProposal};

    fn proposal() -> ProposalArtifact {
        ProposalArtifact {
            crate_name: "forge-protocol".into(),
            description: "a test proposal".into(),
            files: vec![FileProposal {
                path: "src/lib.rs".into(),
                action: FileAction::Create,
                content: "pub fn a() {}".into(),
            }],
            cargo_toml_patch: CargoTomlPatch::default(),
            reasoning: "because".into(),
        }
    }

    #[test]
    fn recording_a_step_hashes_the_context_it_came_from() {
        let mut context = ForgeContext::new("add a chunker", 2);
        context.record_step(ForgeStateName::Planning, None);

        assert_eq!(context.history.len(), 1);
        let step = &context.history[0];
        assert_eq!(step.state, ForgeStateName::Planning);
        assert_eq!(step.input_context_hash.len(), 64);
        assert!(step.output_artifact_hash.is_none());

        // Same context, same digest: the journal is reproducible.
        let mut twin = ForgeContext::new("add a chunker", 2);
        twin.record_step(ForgeStateName::Planning, None);
        assert_eq!(twin.history[0].input_context_hash, step.input_context_hash);
    }

    #[test]
    fn a_proposal_on_the_bench_is_hashed_into_the_step() {
        let mut context = ForgeContext::new("add a chunker", 2);
        context.current_proposal = Some(proposal());
        context.record_step(ForgeStateName::Validating, None);

        let digest = context.history[0]
            .output_artifact_hash
            .clone()
            .expect("the proposal digest");
        assert_eq!(digest.len(), 64);
        assert_eq!(digest, proposal_digest(&proposal()));
    }

    #[test]
    fn healing_counts_a_retry_and_can_retry_runs_out() {
        let mut context = ForgeContext::new("add a chunker", 2);
        assert!(context.can_retry());

        context.record_step(
            ForgeStateName::Healing,
            Some(ValidationError::ClippyViolation {
                message: "needless return".into(),
            }),
        );
        assert_eq!(context.retry_count, 1);
        assert!(context.can_retry());

        context.record_step(ForgeStateName::Healing, None);
        assert_eq!(context.retry_count, 2);
        assert!(!context.can_retry());
    }

    #[test]
    fn a_failed_validation_step_keeps_the_validator_output() {
        let mut context = ForgeContext::new("add a chunker", 1);
        let error = ValidationError::CompilationFailed {
            stderr: "error[E0425]: cannot find value `x`".into(),
        };
        context.record_step(ForgeStateName::Validating, Some(error.clone()));
        assert_eq!(context.history[0].error, Some(error));
    }
}
