//! Targets and domains — *what* the forge evolves and *which plane* it lives on.
//!
//! The forge treats every evolution surface the same way. A [`Domain`] says
//! which plane a workflow belongs to; a [`Target`] says which artifact tree it
//! walks. Both are authored in the workflow registry and rendered by the shell,
//! so there is no per-surface frontend code.

use serde::{Deserialize, Serialize};

/// Which plane a workflow belongs to.
///
/// * `intern` — a cratelab crate itself (the bench evolves the bench).
/// * `extern` — a public workspace such as `tv-rs` or `cctv-rs`.
/// * `ops` — operations surfaces (CI status, provider readiness, env hygiene).
/// * `rag` — the retrieval plane that feeds every other domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    /// A cratelab crate.
    Intern,
    /// A public Rust workspace under evolution.
    Extern,
    /// An operations surface.
    Ops,
    /// The retrieval / intelligence plane.
    Rag,
}

impl Domain {
    /// The stable wire/registry name.
    pub fn as_str(self) -> &'static str {
        match self {
            Domain::Intern => "intern",
            Domain::Extern => "extern",
            Domain::Ops => "ops",
            Domain::Rag => "rag",
        }
    }

    /// Every domain, in registry order.
    pub const ALL: [Domain; 4] = [Domain::Intern, Domain::Extern, Domain::Ops, Domain::Rag];
}

/// The thing the forge evolves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    /// Stable id, e.g. `tv-rs` or `lab-manifest`.
    pub id: String,
    /// Workspace root. Prefer a repo-relative or environment-resolved path;
    /// never bake an absolute user path into a committed registry.
    pub root: String,
    /// Glob of the crate directories to walk, e.g. `crates/*`.
    #[serde(default = "default_crate_glob")]
    pub crate_glob: String,
    /// Which plane this target belongs to.
    pub domain: Domain,
}

fn default_crate_glob() -> String {
    "crates/*".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_wire_names_round_trip() {
        for domain in Domain::ALL {
            let json = serde_json::to_string(&domain).expect("serialize");
            assert_eq!(json, format!("\"{}\"", domain.as_str()));
            let back: Domain = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, domain);
        }
    }

    #[test]
    fn target_defaults_the_crate_glob() {
        let target: Target = toml::from_str("id = 'tv-rs'\nroot = 'TV_RS_ROOT'\ndomain = 'extern'")
            .expect("parse");
        assert_eq!(target.crate_glob, "crates/*");
        assert_eq!(target.domain, Domain::Extern);
    }
}
