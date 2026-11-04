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

/// The trivial rule set: clear every authored gate.
///
/// Use it only for a structural dry walk ([`Pipeline::dry_run`]) — it performs
/// no evidence check, so it can never refuse. Real runs use [`GateRules`].
#[derive(Debug, Clone, Copy, Default)]
pub struct AllowAll;

impl RuleSet for AllowAll {
    fn evaluate(&self, _stage: &Stage, _prior: &[StageRun]) -> RuleVerdict {
        RuleVerdict::Pass
    }
}

/// The forge's deterministic refusal rules.
///
/// Consulted at every authored gate, gated on the number of grounded evidence
/// items the run retrieved. It is a pure function of the stage, the stages
/// already run and that count — no clock, no RNG, no I/O — so the same workflow
/// with the same evidence always yields the same refusals.
///
/// Three rules, one per gated decision:
///
/// * **Research** — refuse (`no-evidence`) when nothing was retrieved: there is
///   nothing to study, so the run declines instead of guessing.
/// * **Propose** — refuse (`no-evidence`) unless a research gate was cleared and
///   the packet is non-empty: *refuse to propose when the evidence packet is
///   empty*.
/// * **Validate** — refuse (`unproposed-artifact`) unless a propose gate was
///   cleared: an artifact is never emitted without a cleared proposal.
#[derive(Debug, Clone, Copy)]
pub struct GateRules {
    /// Number of grounded evidence items available to the run.
    evidence: usize,
}

impl GateRules {
    /// Rules gated on `evidence`, the number of grounded evidence items.
    pub fn with_evidence(evidence: usize) -> Self {
        Self { evidence }
    }

    /// True when the run retrieved no grounded evidence.
    pub fn has_no_evidence(&self) -> bool {
        self.evidence == 0
    }
}

impl RuleSet for GateRules {
    fn evaluate(&self, stage: &Stage, prior: &[StageRun]) -> RuleVerdict {
        let Some(gate) = stage.gate else {
            return RuleVerdict::Pass;
        };
        let cleared = |gate: Gate| {
            prior
                .iter()
                .any(|run| run.gate == Some(gate) && run.verdict == RuleVerdict::Pass)
        };
        match gate {
            Gate::Research => {
                if self.has_no_evidence() {
                    RuleVerdict::Refuse(Refusal::new(
                        "no-evidence",
                        format!("stage '{}' found no grounded evidence to study", stage.id),
                    ))
                } else {
                    RuleVerdict::Pass
                }
            }
            Gate::Propose => {
                if !cleared(Gate::Research) || self.has_no_evidence() {
                    RuleVerdict::Refuse(Refusal::new(
                        "no-evidence",
                        format!(
                            "refusing to propose at '{}': the evidence packet is empty",
                            stage.id
                        ),
                    ))
                } else {
                    RuleVerdict::Pass
                }
            }
            Gate::Validate => {
                if cleared(Gate::Propose) {
                    RuleVerdict::Pass
                } else {
                    RuleVerdict::Refuse(Refusal::new(
                        "unproposed-artifact",
                        format!(
                            "refusing to emit an artifact at '{}': no cleared proposal",
                            stage.id
                        ),
                    ))
                }
            }
        }
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

    /// Run gated on `evidence`, the number of grounded evidence items retrieved.
    ///
    /// Uses [`GateRules`], so a run with no evidence refuses at the first gate
    /// instead of proposing anything. Refusals are data: the run still exits `0`.
    pub fn run(&self, evidence: usize) -> ForgeResult<PipelineOutcome> {
        self.run_with_rules(&GateRules::with_evidence(evidence))
    }

    /// Walk every stage unconditionally, consulting no rules.
    ///
    /// A structural dry walk for validation and UI rendering only; it performs
    /// no evidence check, so it can never refuse.
    pub fn dry_run(&self) -> ForgeResult<PipelineOutcome> {
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
    fn dry_run_walks_every_stage_and_emits_one_artifact() {
        let workflow = first_workflow();
        let outcome = Pipeline::new(&workflow).dry_run().expect("runs");
        assert_eq!(outcome.runs.len(), workflow.stages.len());
        assert!(!outcome.refused());
        assert!(!outcome.halted);
        assert_eq!(outcome.artifacts.len(), 1);
        assert_eq!(outcome.exit_code(), 0);
    }

    #[test]
    fn zero_evidence_refuses_at_the_first_gate_as_data() {
        let workflow = first_workflow();
        let outcome = Pipeline::new(&workflow).run(0).expect("runs");
        assert!(outcome.refused());
        assert!(outcome.halted);
        // Halts at the first gated stage (study), so exactly one refusal.
        assert_eq!(outcome.refusals().len(), 1);
        assert_eq!(outcome.refusals()[0].code, "no-evidence");
        // Info-level refusal => exit 0, and no artifact was emitted.
        assert_eq!(outcome.exit_code(), 0);
        assert!(outcome.artifacts.is_empty());
    }

    #[test]
    fn grounded_evidence_clears_every_gate() {
        let workflow = first_workflow();
        let outcome = Pipeline::new(&workflow).run(3).expect("runs");
        assert!(!outcome.refused());
        assert!(!outcome.halted);
        assert_eq!(outcome.artifacts.len(), 1);
        assert_eq!(outcome.exit_code(), 0);
    }

    #[test]
    fn propose_is_refused_without_a_cleared_research_gate() {
        // A first gated stage that proposes, with no research gate before it,
        // must refuse to propose even when evidence is present.
        let stage = Stage {
            id: "tools".into(),
            kind: StageKind::Tools,
            title: "Arm the agent".into(),
            gate: Some(Gate::Propose),
        };
        let verdict = GateRules::with_evidence(5).evaluate(&stage, &[]);
        assert!(matches!(verdict, RuleVerdict::Refuse(_)));
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
