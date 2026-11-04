//! Proposal artifacts — what a completed run produces.
//!
//! The forge never edits a target silently. The terminal stage of every
//! pipeline emits an [`Artifact`]: a reviewable proposal (a feature, a fix, or
//! a documentation/knowledge update) whose every claim carries an
//! [`EvidenceRef`] back to the exact source revision it came from.

use crate::workflow::Workflow;
use serde::{Deserialize, Serialize};

/// The kind of proposal a run produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// A new capability.
    Feature,
    /// A correctness or reliability repair.
    Fix,
    /// A documentation change.
    Docs,
    /// A knowledge-base addition (corpus/index/prompt asset).
    Knowledge,
    /// A retrieval evaluation result.
    Evaluation,
}

/// A provenance reference attached to an artifact claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRef {
    /// Source id, e.g. a crate id or a docs-tree id.
    pub source: String,
    /// Path within the source, relative and slash-separated.
    pub path: String,
    /// First line of the cited range, when the source is line-addressable.
    #[serde(default)]
    pub start_line: Option<u32>,
    /// The exact source revision the citation was taken from.
    pub revision: String,
}

/// What the forge produces: a reviewable proposal, never a silent edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    /// Which kind of proposal this is.
    pub kind: ArtifactKind,
    /// The target this proposal is for.
    pub target: String,
    /// A short title, rendered by the shell.
    pub title: String,
    /// A one-paragraph summary.
    pub summary: String,
    /// Provenance for every claim.
    #[serde(default)]
    pub evidence: Vec<EvidenceRef>,
    /// Ordered, independently reviewable next actions.
    #[serde(default)]
    pub steps: Vec<String>,
}

impl Artifact {
    /// An empty draft for a workflow that reached its artifact stage.
    ///
    /// Populated by the prompt/agent stages; awaiting review until then.
    pub fn draft(workflow: &Workflow) -> Artifact {
        Artifact {
            kind: ArtifactKind::Docs,
            target: workflow.target.clone(),
            title: format!("Evolution proposal for {}", workflow.target),
            summary: format!(
                "Draft from workflow '{}'; awaiting grounded evidence and review.",
                workflow.id
            ),
            evidence: Vec::new(),
            steps: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_carries_the_target_and_no_fabricated_evidence() {
        let workflow = Workflow {
            id: "tv-rs-evolution".into(),
            domain: crate::target::Domain::Extern,
            title: "tv-rs evolution".into(),
            target: "tv-rs".into(),
            tools: Vec::new(),
            stages: Vec::new(),
        };
        let artifact = Artifact::draft(&workflow);
        assert_eq!(artifact.target, "tv-rs");
        assert!(artifact.evidence.is_empty());
        assert!(artifact.steps.is_empty());
    }
}
