//! # lab-coverage — the manifest surface vs what the lab actually touched
//!
//! A test run only means something when it is *placed* on the surface it
//! exercised. The ledger does exactly that: every invocation the runner
//! performs becomes one record under a
//! `(workspace, command, arg-bucket)` cell. The heatmap is then just the
//! difference between the manifest's declared surface and the filled cells.
//!
//! Coverage here is **behavioural**, not line coverage: a command counts as
//! covered when the lab has invoked it against a bucket of arguments and
//! observed an outcome — pass, refusal, failure, timeout or rate limit.
//! A refusal is *valuable* coverage: it proves the workspace's "refusals
//! are part of the API" invariant actually holds at that cell.

use lab_errors::{LabError, LabErrorKind, LabResult};
use lab_manifest::LabManifest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// One cell of the heatmap: a command under one argument bucket.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CellKey {
    pub workspace: String,
    pub command: String,
    /// Argument domain bucket, e.g. `"vendor=roku"` or `"default"`.
    /// Built by the runner from the invocation's enum-valued args.
    pub bucket: String,
}

/// What happened the last time(s) this cell was exercised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pass,
    /// Capability refusal — expected and useful.
    Refused,
    /// Any other error (transport, auth, script, …).
    Failed,
    Timeout,
    RateLimited,
}

impl Outcome {
    pub fn from_error(e: &LabError) -> Self {
        match e.kind() {
            LabErrorKind::Capability => Outcome::Refused,
            LabErrorKind::Timeout => Outcome::Timeout,
            LabErrorKind::RateLimited => Outcome::RateLimited,
            _ => Outcome::Failed,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Pass => "pass",
            Outcome::Refused => "refused",
            Outcome::Failed => "failed",
            Outcome::Timeout => "timeout",
            Outcome::RateLimited => "rate_limited",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellRecord {
    pub outcome: Outcome,
    /// Unix millis (lab wall clock; mocks may run a virtual clock).
    pub at_ms: u64,
    pub duration_ms: u64,
    #[serde(default)]
    pub scenario: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

/// Outcome counts per cell — small enough to duplicate per record bucket.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellStats {
    pub runs: u32,
    pub pass: u32,
    pub refused: u32,
    pub failed: u32,
    pub timeout: u32,
    pub rate_limited: u32,
}

/// One row of the heatmap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeatRow {
    pub workspace: String,
    pub command: String,
    pub label: String,
    pub danger: String,
    pub buckets_covered: usize,
    pub runs: u32,
    pub pass: u32,
    pub refused: u32,
    /// Hard failures only — timeouts and rate limits are counted apart so a
    /// gate is not fooled by its own throttling.
    pub failed: u32,
    pub timeout: u32,
    pub rate_limited: u32,
    /// `pass` | `partial` | `failed` | `uncovered`
    pub status: &'static str,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Ledger {
    cells: BTreeMap<CellKey, Vec<CellRecord>>,
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append one observation. Errors from the *lab itself* (manifest,
    /// coverage, internal) are rejected — they say nothing about the
    /// workspace under test and would poison the heatmap.
    pub fn record(&mut self, key: CellKey, rec: CellRecord) -> LabResult<()> {
        if rec.outcome == Outcome::Failed {
            if let Some(d) = &rec.detail {
                if d.starts_with("manifest:") || d.starts_with("coverage:") {
                    return Err(LabError::Coverage {
                        problem: format!(
                            "lab-internal error recorded as cell outcome: {d}"
                        ),
                    });
                }
            }
        }
        self.cells.entry(key).or_default().push(rec);
        Ok(())
    }

    pub fn cell(&self, key: &CellKey) -> Option<&Vec<CellRecord>> {
        self.cells.get(key)
    }

    fn stats(&self, key: &CellKey) -> CellStats {
        let mut s = CellStats::default();
        if let Some(recs) = self.cells.get(key) {
            for r in recs {
                s.runs += 1;
                match r.outcome {
                    Outcome::Pass => s.pass += 1,
                    Outcome::Refused => s.refused += 1,
                    Outcome::Failed => s.failed += 1,
                    Outcome::Timeout => s.timeout += 1,
                    Outcome::RateLimited => s.rate_limited += 1,
                }
            }
        }
        s
    }

    /// Heatmap rows for one manifest's declared surface.
    pub fn heatmap(&self, manifest: &LabManifest) -> Vec<HeatRow> {
        let ws = &manifest.workspace.name;
        manifest
            .commands
            .iter()
            .map(|cmd| {
                let buckets: Vec<CellKey> = self
                    .cells
                    .keys()
                    .filter(|k| k.workspace == *ws && k.command == cmd.id)
                    .cloned()
                    .collect();
                let mut row = HeatRow {
                    workspace: ws.clone(),
                    command: cmd.id.clone(),
                    label: cmd.label.clone(),
                    danger: format!("{:?}", cmd.danger).to_ascii_lowercase(),
                    buckets_covered: buckets.len(),
                    runs: 0,
                    pass: 0,
                    refused: 0,
                    failed: 0,
                    timeout: 0,
                    rate_limited: 0,
                    status: "uncovered",
                };
                for b in &buckets {
                    let s = self.stats(b);
                    row.runs += s.runs;
                    row.pass += s.pass;
                    row.refused += s.refused;
                    row.failed += s.failed;
                    row.timeout += s.timeout;
                    row.rate_limited += s.rate_limited;
                }
                row.status = if row.pass > 0 {
                    "pass"
                } else if row.runs == 0 {
                    "uncovered"
                } else if row.refused > 0
                    && row.failed == 0
                    && row.timeout == 0
                    && row.rate_limited == 0
                {
                    "partial"
                } else {
                    "failed"
                };
                row
            })
            .collect()
    }

    /// Aggregate percentage for a workspace: commands with at least one
    /// passing (or refused-only) cell over all declared commands.
    pub fn coverage_percent(&self, manifest: &LabManifest) -> f64 {
        let rows = self.heatmap(manifest);
        if rows.is_empty() {
            return 0.0;
        }
        let covered = rows
            .iter()
            .filter(|r| r.status == "pass" || r.status == "partial")
            .count();
        (covered as f64 / rows.len() as f64) * 100.0
    }

    /// Markdown table for CI artifacts and PR comments.
    pub fn to_markdown(&self, manifest: &LabManifest) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "## Lab coverage — {} (v{})\n",
            manifest.workspace.name, manifest.workspace.version
        );
        let rows = self.heatmap(manifest);
        let _ = writeln!(
            out,
            "| Command | Label | Danger | Buckets | Runs | Pass | Refused | Failed | Timeout | RateLim | Status |"
        );
        let _ = writeln!(
            out,
            "|---|---|---|---:|---:|---:|---:|---:|---:|---:|---|"
        );
        for r in &rows {
            let _ = writeln!(
                out,
                "| `{}` | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                r.command, r.label, r.danger, r.buckets_covered, r.runs, r.pass, r.refused,
                r.failed, r.timeout, r.rate_limited, r.status
            );
        }
        let _ = writeln!(out, "\n**Coverage: {:.1}%**", self.coverage_percent(manifest));
        out
    }

    pub fn to_json(&self, manifest: &LabManifest) -> serde_json::Value {
        serde_json::json!({
            "workspace": manifest.workspace.name,
            "version": manifest.workspace.version,
            "coverage_percent": (self.coverage_percent(manifest) * 100.0).round() / 100.0,
            "rows": self.heatmap(manifest),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lab_manifest::{CommandSpec, WorkspaceRef};

    fn manifest(cmd_ids: &[&str]) -> LabManifest {
        LabManifest {
            schema_version: 1,
            workspace: WorkspaceRef {
                name: "tv-rs".into(),
                prefix: "tv".into(),
                repo: "r".into(),
                facade: "tv-rs".into(),
                core: "tv-rs-core".into(),
                version: "0.6.0".into(),
            },
            errors: Default::default(),
            device_classes: vec![],
            commands: cmd_ids
                .iter()
                .map(|id| CommandSpec {
                    id: id.to_string(),
                    label: id.to_string(),
                    kind: Default::default(),
                    danger: Default::default(),
                    source_enum: None,
                    variant: None,
                    args: vec![],
                    timeout_ms: 8000,
                    rate_limit: None,
                    emits: vec![],
                    conformance: None,
                    invariants: vec![],
                    mock: None,
                    help: None,
                })
                .collect(),
            events: vec![],
            servers: vec![],
            limits: vec![],
            quirks: None,
        }
    }

    fn key(ws: &str, cmd: &str, bucket: &str) -> CellKey {
        CellKey {
            workspace: ws.into(),
            command: cmd.into(),
            bucket: bucket.into(),
        }
    }

    fn rec(outcome: Outcome) -> CellRecord {
        CellRecord { outcome, at_ms: 0, duration_ms: 10, scenario: None, detail: None }
    }

    #[test]
    fn heatmap_reflects_outcomes() {
        let m = manifest(&["tv.wake", "tv.power.off", "tv.key.press"]);
        let mut l = Ledger::new();
        l.record(key("tv-rs", "tv.wake", "default"), rec(Outcome::Pass)).unwrap();
        l.record(key("tv-rs", "tv.power.off", "vendor=roku"), rec(Outcome::Refused)).unwrap();
        let rows = l.heatmap(&m);
        assert_eq!(rows[0].status, "pass");
        assert_eq!(rows[1].status, "partial"); // refused-only = partial
        assert_eq!(rows[2].status, "uncovered");
        let pct = l.coverage_percent(&m);
        assert!((pct - 200.0 / 3.0).abs() < 0.01, "{pct}");
    }

    #[test]
    fn timeout_and_rate_limit_are_not_counted_as_failed() {
        let m = manifest(&["tv.media.seek"]);
        let mut l = Ledger::new();
        l.record(key("tv-rs", "tv.media.seek", "default"), rec(Outcome::Timeout)).unwrap();
        l.record(key("tv-rs", "tv.media.seek", "default"), rec(Outcome::RateLimited)).unwrap();
        let row = &l.heatmap(&m)[0];
        assert_eq!(row.failed, 0, "timeouts/rate limits must not inflate `failed`");
        assert_eq!(row.timeout, 1);
        assert_eq!(row.rate_limited, 1);
        assert_eq!(row.pass, 0);
        // still not a passing cell — a gate must not read this as covered
        assert_eq!(row.status, "failed");
    }

    #[test]
    fn lab_internal_errors_are_rejected_from_cells() {
        let _m = manifest(&["tv.wake"]);
        let mut l = Ledger::new();
        let bad = CellRecord {
            outcome: Outcome::Failed,
            at_ms: 0,
            duration_ms: 0,
            scenario: None,
            detail: Some("manifest: broken".into()),
        };
        assert!(l.record(key("tv-rs", "tv.wake", "default"), bad).is_err());
    }

    #[test]
    fn markdown_contains_table_and_percent() {
        let m = manifest(&["tv.wake"]);
        let mut l = Ledger::new();
        l.record(key("tv-rs", "tv.wake", "default"), rec(Outcome::Pass)).unwrap();
        let md = l.to_markdown(&m);
        assert!(md.contains("| `tv.wake` |"));
        assert!(md.contains("Coverage: 100.0%"), "{md}");
        let js = l.to_json(&m);
        assert_eq!(js["coverage_percent"], 100.0);
    }
}
