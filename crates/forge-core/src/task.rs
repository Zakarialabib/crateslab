//! The task catalog — the forge's authored *what*, the sibling of the workflow
//! registry.
//!
//! A [`Workflow`](crate::workflow::Workflow) is *how* a run proceeds (the
//! seven-stage spine: study → knowledge → retrieve → prompt → tools →
//! pipeline → artifact). A [`Task`] is *what* the run is about — a research
//! summary, a documentation audit, a bug investigation. Both are authored in
//! the engine (`forge.workflows.toml`, `forge.tasks.toml`); the Tauri shell
//! renders them and the command surface validates against them, so no frontend
//! hard-codes either model.

use crate::error::{ForgeError, ForgeResult};
use serde::{Deserialize, Serialize};

/// The surface a task is offered on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskScope {
    /// The per-crate pilot: local dry runs and confirmed live previews.
    Pilot,
    /// The internal crate-intelligence prompt path.
    #[serde(rename = "crate-intelligence")]
    CrateIntelligence,
}

impl TaskScope {
    /// The stable wire/registry name.
    pub fn as_str(self) -> &'static str {
        match self {
            TaskScope::Pilot => "pilot",
            TaskScope::CrateIntelligence => "crate-intelligence",
        }
    }

    /// Every scope, in registry order.
    pub const ALL: [TaskScope; 2] = [TaskScope::Pilot, TaskScope::CrateIntelligence];
}

/// One authored task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// Stable id, unique across the catalog.
    pub id: String,
    /// Display label, rendered by the shell.
    pub label: String,
    /// Grouping label (Understand / Improve / Plan / Diagnose / …).
    pub group: String,
    /// The offline half of the task.
    #[serde(default)]
    pub local: String,
    /// The provider half of the task.
    #[serde(default)]
    pub live: String,
    /// The short human-facing rail the shell renders for the selected task.
    #[serde(default)]
    pub steps: Vec<String>,
    /// Optional reference URLs shown alongside the task.
    #[serde(default)]
    pub refs: Vec<String>,
    /// The surfaces this task is offered on.
    #[serde(default)]
    pub scopes: Vec<TaskScope>,
}

/// The parsed single source of truth for tasks: a list of authored tasks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskCatalog {
    /// Every authored task.
    #[serde(default)]
    pub tasks: Vec<Task>,
}

impl TaskCatalog {
    /// Parse and validate the catalog from TOML.
    pub fn from_toml(text: &str) -> ForgeResult<Self> {
        let catalog: TaskCatalog = toml::from_str(text).map_err(|error| ForgeError::Workflow {
            id: "<tasks>".into(),
            problem: error.to_string(),
        })?;
        catalog.validate()?;
        Ok(catalog)
    }

    /// Look up a task by id.
    pub fn get(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    /// Every task, in catalog order.
    pub fn ids(&self) -> Vec<&str> {
        self.tasks.iter().map(|task| task.id.as_str()).collect()
    }

    /// Every task offered on a given scope, in catalog order.
    pub fn in_scope(&self, scope: TaskScope) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|task| task.scopes.contains(&scope))
            .collect()
    }

    /// Every id offered on a given scope, in catalog order.
    pub fn ids_in_scope(&self, scope: TaskScope) -> Vec<&str> {
        self.in_scope(scope).into_iter().map(|task| task.id.as_str()).collect()
    }

    /// Validate ids and required fields.
    ///
    /// Rules: ids are unique, and every task has a label, a group, steps and at
    /// least one scope.
    pub fn validate(&self) -> ForgeResult<()> {
        let mut seen = std::collections::BTreeSet::new();
        for task in &self.tasks {
            if !seen.insert(task.id.clone()) {
                return Err(ForgeError::Workflow {
                    id: task.id.clone(),
                    problem: "duplicate task id".into(),
                });
            }
            if task.label.is_empty() || task.group.is_empty() {
                return Err(ForgeError::Workflow {
                    id: task.id.clone(),
                    problem: "has no label or group".into(),
                });
            }
            if task.steps.is_empty() {
                return Err(ForgeError::Workflow {
                    id: task.id.clone(),
                    problem: "has no steps".into(),
                });
            }
            if task.scopes.is_empty() {
                return Err(ForgeError::Workflow {
                    id: task.id.clone(),
                    problem: "has no scope".into(),
                });
            }
        }
        Ok(())
    }
}

/// The task catalog shipped with the forge, embedded at build time.
pub const EMBEDDED_TASKS: &str = include_str!("../forge.tasks.toml");

/// Parse the embedded task catalog.
pub fn embedded_tasks() -> ForgeResult<TaskCatalog> {
    TaskCatalog::from_toml(EMBEDDED_TASKS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_parses_and_covers_every_scope() {
        let catalog = embedded_tasks().expect("embedded task catalog parses");
        assert!(!catalog.tasks.is_empty());
        for scope in TaskScope::ALL {
            assert!(
                !catalog.in_scope(scope).is_empty(),
                "scope '{}' has no task",
                scope.as_str()
            );
        }
        // The pilot keeps its eight tasks; crate-intelligence adds its own two.
        assert_eq!(catalog.ids_in_scope(TaskScope::Pilot).len(), 8);
        assert_eq!(catalog.ids_in_scope(TaskScope::CrateIntelligence).len(), 4);
    }

    #[test]
    fn pilot_and_crate_intelligence_sets_are_the_expected_ids() {
        let catalog = embedded_tasks().expect("catalog");
        assert_eq!(
            catalog.ids_in_scope(TaskScope::Pilot),
            vec![
                "research-summary",
                "feature-study",
                "issue-triage",
                "rustdoc-audit",
                "bug-investigation",
                "debug-plan",
                "knowledge-synthesis",
                "cross-language-research",
            ]
        );
        assert_eq!(
            catalog.ids_in_scope(TaskScope::CrateIntelligence),
            vec![
                "research-summary",
                "rustdoc-audit",
                "conformance-review",
                "architecture-map",
            ]
        );
    }

    #[test]
    fn validation_rejects_duplicates_and_missing_fields() {
        let duplicate = r#"
[[tasks]]
id = "a"
label = "a"
group = "g"
steps = ["one"]
scopes = ["pilot"]

[[tasks]]
id = "a"
label = "a again"
group = "g"
steps = ["one"]
scopes = ["pilot"]
"#;
        assert!(TaskCatalog::from_toml(duplicate).is_err());

        let no_scope = r#"
[[tasks]]
id = "a"
label = "a"
group = "g"
steps = ["one"]
"#;
        assert!(TaskCatalog::from_toml(no_scope).is_err());
    }
}
