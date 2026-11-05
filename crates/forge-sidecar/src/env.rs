//! Environment and `.env` resolution.
//!
//! Every credential and endpoint the sidecar uses is referenced **by name**
//! only. This module turns those names into values, from the process
//! environment and (optionally) one `.env` file, so nothing secret is ever
//! compiled in, committed, or embedded in a descriptor.
//!
//! Empty values are treated as absent: a line like `GROQ_API_KEY=` in a `.env`
//! file means "not configured", not "configured with the empty string".

use std::collections::BTreeMap;
use std::path::Path;

use forge_core::{ForgeError, ForgeResult};

/// Resolved environment values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    values: BTreeMap<String, String>,
}

impl Env {
    /// An empty environment.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse `KEY=value` lines.
    ///
    /// Tolerates a leading `export`, blank lines, `#` comments, and single- or
    /// double-quoted values.
    pub fn parse(raw: &str) -> Self {
        let mut env = Self::new();
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = line.strip_prefix("export ").unwrap_or(line).trim();
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            env.set(key.trim(), value.trim().trim_matches(['\'', '"']));
        }
        env
    }

    /// The process environment.
    pub fn from_process() -> Self {
        Self::from_process_with_file(None)
    }

    /// The process environment layered over an optional `.env` file.
    ///
    /// The process wins on conflict, so a shell-provided value can override a
    /// stale file without editing it.
    pub fn from_process_with_file(path: Option<&Path>) -> Self {
        let mut env = path
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|raw| Self::parse(&raw))
            .unwrap_or_default();
        for (key, value) in std::env::vars() {
            env.set(key, value);
        }
        env
    }

    /// Record a value, ignoring empty ones.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let value = value.into();
        if !value.is_empty() {
            self.values.insert(key.into(), value);
        }
    }

    /// Look a value up. Empty values are absent.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// Whether a non-empty value is present.
    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// A value, or a [`ForgeError::Credential`] naming the service that needs it.
    pub fn require(&self, service: &str, key: &str) -> ForgeResult<&str> {
        self.get(key).ok_or_else(|| ForgeError::Credential {
            service: service.to_string(),
            key: key.to_string(),
        })
    }

    /// Every key currently present, sorted.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_tolerates_exports_comments_and_quotes() {
        let env = Env::parse(
            "# a comment\n\
             export GROQ_API_KEY=gsk_123\n\
             \n\
             HF_DATASET_REPO=\"owner/dataset\"\n\
             EMPTY=\n",
        );
        assert_eq!(env.get("GROQ_API_KEY"), Some("gsk_123"));
        assert_eq!(env.get("HF_DATASET_REPO"), Some("owner/dataset"));
        // An empty value means "not configured".
        assert_eq!(env.get("EMPTY"), None);
        assert!(!env.has("EMPTY"));
        assert_eq!(env.get("MISSING"), None);
    }

    #[test]
    fn missing_credentials_produce_a_setup_error_not_a_panic() {
        let env = Env::new();
        let error = env.require("groq", "GROQ_API_KEY").expect_err("missing");
        assert!(matches!(error, ForgeError::Credential { ref service, ref key } if service == "groq" && key == "GROQ_API_KEY"));
        assert_eq!(error.exit_code(), 2);
    }

    #[test]
    fn keys_are_sorted_for_stable_output() {
        let env = Env::parse("B=2\nA=1\n");
        assert_eq!(env.keys().collect::<Vec<_>>(), vec!["A", "B"]);
    }
}
