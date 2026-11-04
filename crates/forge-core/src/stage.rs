//! The seven-stage pipeline spine and its gates.
//!
//! Stages are *authored* in the workflow registry; the engine only knows the
//! ordered [`StageKind`] vocabulary and the three [`Gate`]s it can consult.
//! A gate produces a [`RuleVerdict`], and a refusal is carried as data
//! (a [`Refusal`]) rather than raised as an exception.

use lab_errors::Severity;
use serde::{Deserialize, Serialize};

/// The ordered stages every evolution run walks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageKind {
    /// Study the research sources (a NotebookLM-style corpus).
    Study,
    /// Build the knowledge base from the studied sources.
    Knowledge,
    /// Retrieve a provenance-rich evidence packet.
    Retrieve,
    /// Render scripted, retrieval-grounded prompts.
    Prompt,
    /// Hand the agent the tools it needs for this target.
    Tools,
    /// Run the gated step pipeline (research → propose → validate).
    Pipeline,
    /// Emit a feature / fix / doc proposal.
    Artifact,
}

impl StageKind {
    /// Every stage, in pipeline order.
    pub const ALL: [StageKind; 7] = [
        StageKind::Study,
        StageKind::Knowledge,
        StageKind::Retrieve,
        StageKind::Prompt,
        StageKind::Tools,
        StageKind::Pipeline,
        StageKind::Artifact,
    ];

    /// The stable wire/registry name.
    pub fn as_str(self) -> &'static str {
        match self {
            StageKind::Study => "study",
            StageKind::Knowledge => "knowledge",
            StageKind::Retrieve => "retrieve",
            StageKind::Prompt => "prompt",
            StageKind::Tools => "tools",
            StageKind::Pipeline => "pipeline",
            StageKind::Artifact => "artifact",
        }
    }

    /// Position in the pipeline, used to validate authored ordering.
    pub fn order(self) -> u8 {
        match self {
            StageKind::Study => 0,
            StageKind::Knowledge => 1,
            StageKind::Retrieve => 2,
            StageKind::Prompt => 3,
            StageKind::Tools => 4,
            StageKind::Pipeline => 5,
            StageKind::Artifact => 6,
        }
    }

    /// The gate this stage falls under by default.
    pub fn default_gate(self) -> Gate {
        match self {
            StageKind::Study
            | StageKind::Knowledge
            | StageKind::Retrieve
            | StageKind::Prompt => Gate::Research,
            StageKind::Tools | StageKind::Pipeline => Gate::Propose,
            StageKind::Artifact => Gate::Validate,
        }
    }
}

/// A decision gate between research → propose → validate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gate {
    /// Is the gathered evidence sufficient to proceed?
    Research,
    /// Is the proposed change safe and in scope?
    Propose,
    /// Does the emitted artifact satisfy the output contract?
    Validate,
}

impl Gate {
    /// The stable wire/registry name.
    pub fn as_str(self) -> &'static str {
        match self {
            Gate::Research => "research",
            Gate::Propose => "propose",
            Gate::Validate => "validate",
        }
    }
}

/// A gate refusal — carried as **data**, not raised as an error.
///
/// Refusals default to [`Severity::Info`], which keeps the caller's exit code
/// at `0`: declining to act is part of the API, not a failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refusal {
    /// A stable, machine-checkable code (e.g. `no-evidence`).
    pub code: String,
    /// A human-readable explanation.
    pub message: String,
    /// Severity of the refusal. Info by default.
    #[serde(default = "refusal_severity")]
    pub severity: Severity,
}

impl Refusal {
    /// A refusal at the default [`Severity::Info`].
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            severity: Severity::Info,
        }
    }
}

fn refusal_severity() -> Severity {
    Severity::Info
}

/// The verdict of one stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "verdict")]
pub enum RuleVerdict {
    /// The stage cleared its gate.
    Pass,
    /// The stage declined to act; the refusal is carried as data.
    Refuse(Refusal),
}

/// One authored stage in a workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage {
    /// Stable stage id, unique within its workflow.
    pub id: String,
    /// Which of the seven stages this is.
    pub kind: StageKind,
    /// Display title, rendered by the shell.
    pub title: String,
    /// The gate to consult before running. `None` means the stage always runs.
    #[serde(default)]
    pub gate: Option<Gate>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_seven_kinds_are_ordered_and_stable() {
        assert_eq!(StageKind::ALL.len(), 7);
        for (index, kind) in StageKind::ALL.iter().enumerate() {
            assert_eq!(usize::from(kind.order()), index);
            assert_eq!(kind.as_str(), kind.as_str().to_ascii_lowercase());
        }
        assert!(StageKind::Study.order() < StageKind::Artifact.order());
    }

    #[test]
    fn default_gates_follow_research_propose_validate() {
        assert_eq!(StageKind::Study.default_gate(), Gate::Research);
        assert_eq!(StageKind::Retrieve.default_gate(), Gate::Research);
        assert_eq!(StageKind::Tools.default_gate(), Gate::Propose);
        assert_eq!(StageKind::Artifact.default_gate(), Gate::Validate);
    }

    #[test]
    fn refusal_defaults_to_info_severity() {
        let refusal = Refusal::new("no-evidence", "no indexed chunk matched");
        assert_eq!(refusal.severity, Severity::Info);

        // And it survives a round-trip through the wire format.
        let json = serde_json::to_string(&RuleVerdict::Refuse(refusal.clone())).expect("serialize");
        let back: RuleVerdict = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, RuleVerdict::Refuse(refusal));
    }
}
