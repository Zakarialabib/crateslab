//! # lab-errors — the one error vocabulary of the lab
//!
//! Every crate in a workspace under test already owns a typed error taxonomy
//! (`tv-rs-core::Error`, `cctv-core::Error`, …). The lab does **not** replace
//! those; it normalizes them so the UI, the runner, the coverage ledger and
//! the scenario engine can all branch on the *same* small set of kinds.
//!
//! Three layers:
//!
//! 1. [`LabError`] — the structured error the lab itself produces.
//! 2. [`Remedy`] — the machine-readable "what should happen next" hint.
//! 3. [`ErrorMapping`] — a manifest-declared table that translates a
//!    workspace error (variant name + message) into a [`LabError`], without
//!    the lab needing a compile-time dependency on the workspace.
//!
//! The rule mirrors the workspaces' own invariant: *refusals are part of the
//! API* — a capability the device does not have is [`LabError::Capability`],
//! never a panic and never a generic 500.

use serde::{Deserialize, Serialize};

/// Coarse classification used by the UI, the coverage ledger and exit codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabErrorKind {
    /// The lab manifest itself is wrong (missing section, bad reference).
    Manifest,
    /// Discovery failed (SSDP sweep, WS-Discovery probe, registry lookup).
    Discovery,
    /// Socket/TLS/WebSocket-level failure while talking to a device.
    Transport,
    /// Refused: the target device does not expose this capability.
    Capability,
    /// Pairing / token / credentials problem.
    Auth,
    /// The operation exceeded its declared budget.
    Timeout,
    /// The rate limiter refused the call; retry later.
    RateLimited,
    /// A scenario step failed its assertion.
    Scenario,
    /// A Rhai (or other) script misbehaved.
    Script,
    /// The coverage ledger rejected an update.
    Coverage,
    /// A bug in the lab itself. Never shown to end users as-is.
    Internal,
}

impl LabErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LabErrorKind::Manifest => "manifest",
            LabErrorKind::Discovery => "discovery",
            LabErrorKind::Transport => "transport",
            LabErrorKind::Capability => "capability",
            LabErrorKind::Auth => "auth",
            LabErrorKind::Timeout => "timeout",
            LabErrorKind::RateLimited => "rate_limited",
            LabErrorKind::Scenario => "scenario",
            LabErrorKind::Script => "script",
            LabErrorKind::Coverage => "coverage",
            LabErrorKind::Internal => "internal",
        }
    }
}

/// What the caller (UI, scenario engine, CI gate) should do next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum Remedy {
    /// Plain retry is safe and may succeed.
    Retry,
    /// Retry, but not before `after_ms`.
    RetryAfter { after_ms: u64 },
    /// Re-run pairing / refresh the token, then retry.
    Reauth,
    /// The device class cannot do this; check the capability set first.
    CheckCapabilities,
    /// The manifest (or its overrides) must be corrected.
    FixManifest,
    /// Lower the rate limit / budget before retrying.
    ReduceLimits,
    /// The scenario step itself is wrong.
    FixScenario,
    /// The script failed; inspect the script, not the device.
    FixScript,
    /// Give up; surface to the operator.
    Abort,
}

/// Severity drives UI colour and CI gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Expected during testing (e.g. capability refusals).
    Info,
    /// Degraded but survivable.
    Warn,
    /// The run cannot continue meaningfully.
    Fatal,
}

/// The structured error every lab component speaks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
pub enum LabError {
    #[error("manifest {path}: {problem}")]
    Manifest { path: String, problem: String },

    #[error("discovery phase '{phase}' failed: {message}")]
    Discovery { phase: String, message: String },

    #[error("transport to {target} failed ({kind}): {message}")]
    Transport {
        target: String,
        kind: String,
        message: String,
        retryable: bool,
    },

    #[error("device {device} does not expose '{command}' (capability refusal)")]
    Capability { device: String, command: String },

    #[error("auth with {provider} failed at stage '{stage}': {message}")]
    Auth {
        provider: String,
        stage: String,
        message: String,
    },

    #[error("'{operation}' exceeded its {budget_ms} ms budget")]
    Timeout { operation: String, budget_ms: u64 },

    #[error("'{operation}' was rate limited; retry after {retry_after_ms} ms")]
    RateLimited {
        operation: String,
        retry_after_ms: u64,
    },

    #[error("scenario step '{step}': {cause}")]
    Scenario { step: String, cause: String },

    #[error("script '{script}': {message}")]
    Script { script: String, message: String },

    #[error("coverage ledger: {problem}")]
    Coverage { problem: String },

    #[error("internal lab bug: {bug} (please report)")]
    Internal { bug: String },
}

impl LabError {
    pub fn kind(&self) -> LabErrorKind {
        match self {
            LabError::Manifest { .. } => LabErrorKind::Manifest,
            LabError::Discovery { .. } => LabErrorKind::Discovery,
            LabError::Transport { .. } => LabErrorKind::Transport,
            LabError::Capability { .. } => LabErrorKind::Capability,
            LabError::Auth { .. } => LabErrorKind::Auth,
            LabError::Timeout { .. } => LabErrorKind::Timeout,
            LabError::RateLimited { .. } => LabErrorKind::RateLimited,
            LabError::Scenario { .. } => LabErrorKind::Scenario,
            LabError::Script { .. } => LabErrorKind::Script,
            LabError::Coverage { .. } => LabErrorKind::Coverage,
            LabError::Internal { .. } => LabErrorKind::Internal,
        }
    }

    /// Is an automatic retry (after any [`Remedy`] delay) reasonable?
    pub fn retryable(&self) -> bool {
        match self {
            LabError::Transport { retryable, .. } => *retryable,
            LabError::Timeout { .. } | LabError::RateLimited { .. } => true,
            LabError::Discovery { .. } => true,
            // Capability refusals are deterministic; retrying cannot help.
            LabError::Capability { .. } => false,
            LabError::Auth { .. } => false,
            LabError::Manifest { .. } => false,
            LabError::Scenario { .. } => false,
            LabError::Script { .. } => false,
            LabError::Coverage { .. } => false,
            LabError::Internal { .. } => false,
        }
    }

    pub fn remedy(&self) -> Remedy {
        match self {
            LabError::Manifest { .. } | LabError::Coverage { .. } => Remedy::FixManifest,
            LabError::Discovery { .. } => Remedy::Retry,
            LabError::Transport { retryable, .. } => {
                if *retryable {
                    Remedy::Retry
                } else {
                    Remedy::Abort
                }
            }
            LabError::Capability { .. } => Remedy::CheckCapabilities,
            LabError::Auth { .. } => Remedy::Reauth,
            LabError::Timeout { .. } => Remedy::RetryAfter { after_ms: 1_000 },
            LabError::RateLimited { retry_after_ms, .. } => {
                Remedy::RetryAfter { after_ms: *retry_after_ms }
            }
            LabError::Scenario { .. } => Remedy::FixScenario,
            LabError::Script { .. } => Remedy::FixScript,
            LabError::Internal { .. } => Remedy::Abort,
        }
    }

    pub fn severity(&self) -> Severity {
        match self {
            LabError::Capability { .. } => Severity::Info,
            LabError::RateLimited { .. } | LabError::Timeout { .. } => Severity::Warn,
            LabError::Internal { .. } => Severity::Fatal,
            _ => Severity::Warn,
        }
    }

    /// Exit code for the headless CLI (`lab run …`) so CI gates behave like
    /// `tv-rs-cli ensure` does: non-zero when the run is not clean.
    pub fn exit_code(&self) -> i32 {
        match self.severity() {
            Severity::Info => 0,
            Severity::Warn => 2,
            Severity::Fatal => 3,
        }
    }
}

/// One manifest-declared translation rule.
///
/// `matches` is compared against the *source error's variant or kind string*
/// (case-insensitive, prefix match allowed with a trailing `*`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MappingRule {
    pub matches: String,
    pub to: LabErrorKind,
    #[serde(default)]
    pub retryable: Option<bool>,
    #[serde(default)]
    pub note: Option<String>,
}

/// A manifest-declared table: "this workspace's errors look like this".
///
/// Lives in the workspace section of a lab manifest:
///
/// ```toml
/// [[errors.mapping]]
/// matches = "Capability*"
/// to = "capability"
/// note = "per-vendor capability sets refuse before any socket is opened"
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorMapping {
    /// e.g. `"tv-rs-core::Error"` — documentation only.
    pub source: String,
    #[serde(default)]
    pub rules: Vec<MappingRule>,
}

impl ErrorMapping {
    /// Translate a source error (variant/kind + message) into a [`LabError`].
    /// Falls back to [`LabErrorKind::Transport`] with `retryable = true` when
    /// no rule matches, so unknown errors degrade loudly instead of silently.
    pub fn map(&self, variant: &str, message: impl Into<String>) -> LabError {
        let variant_lc = variant.to_ascii_lowercase();
        let rule = self.rules.iter().find(|r| {
            let m = r.matches.to_ascii_lowercase();
            match m.strip_suffix('*') {
                Some(prefix) => variant_lc.starts_with(prefix),
                None => variant_lc == m,
            }
        });
        let message = message.into();
        match rule {
            Some(r) => {
                let retryable = r.retryable.unwrap_or(matches!(
                    r.to,
                    LabErrorKind::Transport
                        | LabErrorKind::Timeout
                        | LabErrorKind::RateLimited
                        | LabErrorKind::Discovery
                ));
                build_mapped(r.to, retryable, message)
            }
            None => LabError::Transport {
                target: self.source.clone(),
                kind: variant.to_string(),
                message,
                retryable: true,
            },
        }
    }
}

fn build_mapped(kind: LabErrorKind, retryable: bool, message: String) -> LabError {
    match kind {
        LabErrorKind::Capability => LabError::Capability {
            device: "source".into(),
            command: message,
        },
        LabErrorKind::Timeout => LabError::Timeout { operation: message, budget_ms: 0 },
        LabErrorKind::RateLimited => LabError::RateLimited {
            operation: message,
            retry_after_ms: 0,
        },
        LabErrorKind::Auth => LabError::Auth { provider: "source".into(), stage: "unspecified".into(), message },
        LabErrorKind::Discovery => LabError::Discovery { phase: "mapped".into(), message },
        LabErrorKind::Manifest => LabError::Manifest { path: "source".into(), problem: message },
        LabErrorKind::Scenario => LabError::Scenario { step: "mapped".into(), cause: message },
        LabErrorKind::Script => LabError::Script { script: "source".into(), message },
        LabErrorKind::Coverage => LabError::Coverage { problem: message },
        LabErrorKind::Internal => LabError::Internal { bug: message },
        LabErrorKind::Transport => LabError::Transport {
            target: "source".into(),
            kind: "mapped".into(),
            message,
            retryable,
        },
    }
}

/// Convenience alias for results inside the lab.
pub type LabResult<T> = Result<T, LabError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_refusal_is_info_and_not_retryable() {
        let e = LabError::Capability {
            device: "tv-192.168.1.40".into(),
            command: "tv.panel.orientation".into(),
        };
        assert_eq!(e.kind(), LabErrorKind::Capability);
        assert!(!e.retryable());
        assert_eq!(e.remedy(), Remedy::CheckCapabilities);
        assert_eq!(e.severity(), Severity::Info);
        assert_eq!(e.exit_code(), 0);
    }

    #[test]
    fn rate_limit_carries_retry_after() {
        let e = LabError::RateLimited {
            operation: "tv.media.seek".into(),
            retry_after_ms: 2_500,
        };
        assert!(e.retryable());
        assert_eq!(e.remedy(), Remedy::RetryAfter { after_ms: 2_500 });
    }

    #[test]
    fn mapping_rules_prefix_and_exact() {
        let map = ErrorMapping {
            source: "tv-rs-core::Error".into(),
            rules: vec![
                MappingRule {
                    matches: "Capability*".into(),
                    to: LabErrorKind::Capability,
                    retryable: Some(false),
                    note: Some("capability sets refuse early".into()),
                },
                MappingRule {
                    matches: "Pairing*".into(),
                    to: LabErrorKind::Auth,
                    retryable: None,
                    note: None,
                },
            ],
        };
        let mapped = map.map("CapabilityRefused", "panel orientation");
        assert_eq!(mapped.kind(), LabErrorKind::Capability);
        assert!(!mapped.retryable());

        let mapped = map.map("PairingRejected", "token window expired");
        assert_eq!(mapped.kind(), LabErrorKind::Auth);
        assert_eq!(mapped.remedy(), Remedy::Reauth);

        // Unknown variants degrade to a retryable transport error.
        let unknown = map.map("SomethingNew", "mystery");
        assert_eq!(unknown.kind(), LabErrorKind::Transport);
        assert!(unknown.retryable());
    }
}
