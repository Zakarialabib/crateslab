//! The deterministic stage pipeline engine.
//!
//! [`Pipeline`] walks a workflow's authored stages in order, consulting a
//! [`RuleSet`] at every gated stage. It uses no RNG and performs no network
//! I/O: the same workflow against the same rule set yields the same
//! [`PipelineOutcome`] every time, which is what makes the forge auditable.
//!
//! A refusal halts the run but is **not** a failure — the [`Refusal`] is
//! recorded in the outcome and [`PipelineOutcome::exit_code`] stays `0`.

use crate::artifact::Artifact;
use crate::error::ForgeResult;
use crate::stage::{Gate, Refusal, RuleVerdict, Stage, StageKind};
use crate::workflow::Workflow;
use lab_errors::Severity;
use serde::{Deserialize, Serialize};

/// The record of one stage run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageRun {
    /// The stage id.
    pub stage: String,
    /// Which of the seven stages it was.
    pub kind: StageKind,
    /// The gate consulted, if any.
    #[serde(default)]
    pub gate: Option<Gate>,
    /// The verdict.
    pub verdict: RuleVerdict,
    /// Free-form, deterministic notes attached by the rule set.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// The full result of driving a workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineOutcome {
    /// The workflow id that ran.
    pub workflow: String,
    /// One entry per stage reached.
    pub runs: Vec<StageRun>,
    /// Artifacts emitted by the terminal stage.
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    /// Whether a refusal halted the run early.
    pub halted: bool,
}

impl PipelineOutcome {
    /// True when any stage produced a refusal.
    pub fn refused(&self) -> bool {
        self.runs
            .iter()
            .any(|run| matches!(run.verdict, RuleVerdict::Refuse(_)))
    }

    /// Every refusal recorded, in order.
    pub fn refusals(&self) -> Vec<&Refusal> {
        self.runs
            .iter()
            .filter_map(|run| match &run.verdict {
                RuleVerdict::Refuse(refusal) => Some(refusal),
                RuleVerdict::Pass => None,
            })
            .collect()
    }

    /// Exit code, in the lab's vocabulary.
    ///
    /// Refusals are data ([`Severity::Info`]), so a refused run exits `0`.
    pub fn exit_code(&self) -> i32 {
        self.refusals()
            .into_iter()
            .map(|refusal| match refusal.severity {
                Severity::Info => 0,
                Severity::Warn => 2,
                Severity::Fatal => 3,
            })
            .max()
            .unwrap_or(0)
    }
}

/// A rule the pipeline consults at a gate.
///
/// Implementations **must be deterministic**: they may read `prior` runs but
/// must not touch the clock, the filesystem or the network.
pub trait RuleSet {
    /// Decide the verdict for `stage`, given the stages already run.
    fn evaluate(&self, stage: &Stage, prior: &[StageRun]) -> RuleVerdict;
}

/// The default rule set: clear every authored gate.
///
/// Real rules arrive with the scripted-prompt layer (e.g. "refuse to propose
/// when the evidence packet is empty").
#[derive(Debug, Clone, Copy, Default)]
pub struct AllowAll;

impl RuleSet for AllowAll {
    fn evaluate(&self, _stage: &Stage, _prior: &[StageRun]) -> RuleVerdict {
        RuleVerdict::Pass
    }
}

/// Drives a workflow's stages.
#[derive(Debug, Clone, Copy)]
pub struct Pipeline<'a> {
    workflow: &'a Workflow,
}

impl<'a> Pipeline<'a> {
    /// A pipeline over one authored workflow.
    pub fn new(workflow: &'a Workflow) -> Self {
        Self { workflow }
    }

    /// Run with the default (allow-all) rule set.
    pub fn run(&self) -> ForgeResult<PipelineOutcome> {
        self.run_with_rules(&AllowAll)
    }

    /// Run, consulting `rules` at every gated stage.
    pub fn run_with_rules(&self, rules: &dyn RuleSet) -> ForgeResult<PipelineOutcome> {
        let mut runs = Vec::with_capacity(self.workflow.stages.len());
        let mut artifacts = Vec::new();
        let mut halted = false;

        for stage in &self.workflow.stages {
            let verdict = match stage.gate {
                Some(_) => rules.evaluate(stage, &runs),
                None => RuleVerdict::Pass,
            };
            let refused = matches!(verdict, RuleVerdict::Refuse(_));
            runs.push(StageRun {
                stage: stage.id.clone(),
                kind: stage.kind,
                gate: stage.gate,
                verdict,
                notes: Vec::new(),
            });

            if refused {
                halted = true;
                break;
            }
            if stage.kind == StageKind::Artifact {
                artifacts.push(Artifact::draft(self.workflow));
            }
        }

        Ok(PipelineOutcome {
            workflow: self.workflow.id.clone(),
            runs,
            artifacts,
            halted,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::embedded;

    fn first_workflow() -> Workflow {
        embedded()
            .expect("registry")
            .workflows
            .into_iter()
            .next()
            .expect("at least one workflow")
    }

    #[test]
    fn allow_all_walks_every_stage_and_emits_one_artifact() {
        let workflow = first_workflow();
        let outcome = Pipeline::new(&workflow).run().expect("runs");
        assert_eq!(outcome.runs.len(), workflow.stages.len());
        assert!(!outcome.refused());
        assert!(!outcome.halted);
        assert_eq!(outcome.artifacts.len(), 1);
        assert_eq!(outcome.exit_code(), 0);
    }

    #[test]
    fn a_refusal_is_data_and_still_exits_zero() {
        /// Refuses the very first gated stage.
        struct RefuseResearch;
        impl RuleSet for RefuseResearch {
            fn evaluate(&self, stage: &Stage, _prior: &[StageRun]) -> RuleVerdict {
                RuleVerdict::Refuse(Refusal::new(
                    "no-evidence",
                    format!("stage '{}' had no grounded evidence", stage.id),
                ))
            }
        }

        let workflow = first_workflow();
        let outcome = Pipeline::new(&workflow)
            .run_with_rules(&RefuseResearch)
            .expect("runs");
        assert!(outcome.refused());
        assert!(outcome.halted);
        assert_eq!(outcome.refusals().len(), 1);
        // Info-level refusal => exit 0, and no artifact was emitted.
        assert_eq!(outcome.exit_code(), 0);
        assert!(outcome.artifacts.is_empty());
    }
}
