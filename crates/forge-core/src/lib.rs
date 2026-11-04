//! # forge-core — the forge SDK
//!
//! The forge is cratelab's agentic evolution subsystem: it turns a *target*
//! (a public Rust workspace such as `tv-rs`, a cratelab crate, an ops surface,
//! or the retrieval plane itself) into a **gated pipeline** that studies
//! sources, builds a knowledge base, retrieves grounded evidence, renders
//! scripted prompts, hands tools to an agent, and emits a reviewable
//! *evolution artifact* — a feature, a fix, or a doc/knowledge proposal.
//!
//! It replaces a scattered control plane (the former `fleet/` tree plus a
//! drawer of one-off Python scripts) with **one source of truth**, so the
//! Tauri/Svelte shell, the CLI and the `forge-sidecar` binary all render and
//! execute the *same* workflow registry instead of re-implementing it.
//!
//! ## The seven stages
//!
//! Every workflow walks the same ordered spine ([`StageKind`]):
//!
//! 1. [`StageKind::Study`] — study research sources (a NotebookLM-style corpus).
//! 2. [`StageKind::Knowledge`] — build the knowledge base.
//! 3. [`StageKind::Retrieve`] — retrieve a grounded evidence packet.
//! 4. [`StageKind::Prompt`] — render scripted prompts from the evidence.
//! 5. [`StageKind::Tools`] — hand tools to the agent.
//! 6. [`StageKind::Pipeline`] — run the gated step pipeline.
//! 7. [`StageKind::Artifact`] — emit a feature/fix/doc proposal.
//!
//! ## Invariants
//!
//! * **Manifest-driven.** Workflows live in one authored `forge.workflows.toml`;
//!   the UI renders them and the engine executes them. Generated proposes,
//!   authored disposes.
//! * **Refusals are data.** A declined stage is a [`Refusal`] carrying
//!   `Severity::Info` and exit code `0` — never a panic and never a generic 500.
//! * **Deterministic.** The pipeline uses no RNG and performs no network I/O;
//!   providers sit behind a [`ProviderPort`] that is attached explicitly.
//! * **One error vocabulary.** Every forge failure classifies into the lab's
//!   `LabError` taxonomy (see [`error`]).

pub mod artifact;
pub mod error;
pub mod pipeline;
pub mod provider;
pub mod stage;
pub mod target;
pub mod workflow;

pub use artifact::{Artifact, ArtifactKind, EvidenceRef};
pub use error::{ForgeError, ForgeResult};
pub use pipeline::{AllowAll, GateRules, Pipeline, PipelineOutcome, RuleSet, StageRun};
pub use provider::{MockProvider, Provider, ProviderPort, ProviderRequest, ProviderResponse};
pub use stage::{Gate, Refusal, RuleVerdict, Stage, StageKind};
pub use target::{Domain, Target};
pub use workflow::{embedded, Workflow, WorkflowRegistry, EMBEDDED_REGISTRY};
