use serde::{Deserialize, Serialize};

/// The absolute contract the Generator Engine MUST output.
/// No markdown, no conversational text. Pure, parseable data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalArtifact {
    pub crate_name: String,
    pub description: String,
    pub files: Vec<FileProposal>,
    pub cargo_toml_patch: CargoTomlPatch,
    pub reasoning: String, // Brief explanation of the architectural choices made
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileProposal {
    /// Relative path from the crate root (e.g., "src/orchestrator.rs")
    pub path: String,
    pub action: FileAction,
    /// The complete, valid Rust source code for this file
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FileAction {
    Create,
    Update,
    Delete,
}

/// Explicit dependency modifications, preventing the LLM from hallucinating Cargo features
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CargoTomlPatch {
    pub add_dependencies: Vec<Dependency>,
    pub add_dev_dependencies: Vec<Dependency>,
    pub add_features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version: Option<String>,
    pub path: Option<String>, // For local workspace crates
    pub features: Vec<String>,
}