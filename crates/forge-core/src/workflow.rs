//! The workflow registry — the forge's single source of truth.
//!
//! One authored `forge.workflows.toml` describes every evolution workflow
//! (its [`Domain`], its target, the tools it may use, and its ordered
//! [`Stage`]s). The Tauri shell renders this file; the pipeline engine
//! executes it; the sidecar serves it over JSON-RPC. Nothing re-implements it.

use crate::error::{ForgeError, ForgeResult};
use crate::stage::Stage;
use crate::target::Domain;
use serde::{Deserialize, Serialize};

/// One authored evolution workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workflow {
    /// Stable id, unique across the registry.
    pub id: String,
    /// Which plane this workflow belongs to.
    pub domain: Domain,
    /// Display title, rendered by the shell.
    pub title: String,
    /// The id of the target this workflow evolves.
    pub target: String,
    /// Agent tool ids available to this workflow.
    #[serde(default)]
    pub tools: Vec<String>,
    /// The ordered stages, validated against [`StageKind::order`].
    #[serde(default)]
    pub stages: Vec<Stage>,
}

/// The parsed single source of truth: a list of workflows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRegistry {
    /// Every authored workflow.
    #[serde(default)]
    pub workflows: Vec<Workflow>,
}

impl WorkflowRegistry {
    /// Parse and validate the registry from TOML.
    pub fn from_toml(text: &str) -> ForgeResult<Self> {
        let registry: WorkflowRegistry = toml::from_str(text).map_err(|error| ForgeError::Workflow {
            id: "<registry>".into(),
            problem: error.to_string(),
        })?;
        registry.validate()?;
        Ok(registry)
    }

    /// Look up a workflow by id.
    pub fn get(&self, id: &str) -> Option<&Workflow> {
        self.workflows.iter().find(|workflow| workflow.id == id)
    }

    /// Every workflow on a given plane, in registry order.
    pub fn by_domain(&self, domain: Domain) -> Vec<&Workflow> {
        self.workflows
            .iter()
            .filter(|workflow| workflow.domain == domain)
            .collect()
    }

    /// Every workflow id, in registry order.
    pub fn ids(&self) -> Vec<&str> {
        self.workflows
            .iter()
            .map(|workflow| workflow.id.as_str())
            .collect()
    }

    /// Validate ids and stage ordering.
    ///
    /// Rules: ids are unique, every workflow has at least one stage, and each
    /// workflow's stages walk [`StageKind::ALL`] in strictly increasing order.
    pub fn validate(&self) -> ForgeResult<()> {
        let mut seen = std::collections::BTreeSet::new();
        for workflow in &self.workflows {
            if workflow.stages.is_empty() {
                return Err(ForgeError::Workflow {
                    id: workflow.id.clone(),
                    problem: "has no stages".into(),
                });
            }
            if !seen.insert(workflow.id.clone()) {
                return Err(ForgeError::Workflow {
                    id: workflow.id.clone(),
                    problem: "duplicate workflow id".into(),
                });
            }
            let mut previous: Option<u8> = None;
            for stage in &workflow.stages {
                if let Some(order) = previous {
                    if stage.kind.order() <= order {
                        return Err(ForgeError::Workflow {
                            id: workflow.id.clone(),
                            problem: format!("stage '{}' is out of pipeline order", stage.id),
                        });
                    }
                }
                previous = Some(stage.kind.order());
            }
        }
        Ok(())
    }
}

/// The registry shipped with the forge, embedded at build time.
pub const EMBEDDED_REGISTRY: &str = include_str!("../forge.workflows.toml");

/// Parse the embedded registry.
pub fn embedded() -> ForgeResult<WorkflowRegistry> {
    WorkflowRegistry::from_toml(EMBEDDED_REGISTRY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::StageKind;

    #[test]
    fn embedded_registry_parses_and_covers_every_domain() {
        let registry = embedded().expect("embedded registry parses");
        assert!(!registry.workflows.is_empty());
        for workflow in &registry.workflows {
            assert_eq!(
                workflow.stages.len(),
                7,
                "workflow '{}' should walk all seven stages",
                workflow.id
            );
        }
        for domain in Domain::ALL {
            assert!(
                !registry.by_domain(domain).is_empty(),
                "domain '{}' has no workflow",
                domain.as_str()
            );
        }
    }

    #[test]
    fn embedded_registry_stages_are_the_canonical_spine() {
        let registry = embedded().expect("embedded registry parses");
        let workflow = registry.get("cratelab-intern-evolution").expect("intern workflow");
        let kinds: Vec<StageKind> = workflow.stages.iter().map(|stage| stage.kind).collect();
        assert_eq!(kinds, StageKind::ALL.to_vec());
    }

    #[test]
    fn validation_rejects_duplicate_ids_and_out_of_order_stages() {
        let duplicate = r#"
[[workflows]]
id = "a"
domain = "intern"
title = "a"
target = "lab-errors"
[[workflows.stages]]
id = "s"
kind = "study"
title = "s"

[[workflows]]
id = "a"
domain = "ops"
title = "a again"
target = "ops"
[[workflows.stages]]
id = "s"
kind = "study"
title = "s"
"#;
        let error = WorkflowRegistry::from_toml(duplicate).expect_err("duplicate id rejected");
        assert!(matches!(error, ForgeError::Workflow { .. }));

        let out_of_order = r#"
[[workflows]]
id = "a"
domain = "intern"
title = "a"
target = "lab-errors"
[[workflows.stages]]
id = "late"
kind = "artifact"
title = "artifact first"
[[workflows.stages]]
id = "early"
kind = "study"
title = "study second"
"#;
        let error = WorkflowRegistry::from_toml(out_of_order).expect_err("order enforced");
        assert!(matches!(error, ForgeError::Workflow { .. }));
    }
}
