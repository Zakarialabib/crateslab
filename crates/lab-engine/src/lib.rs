//! # lab-engine — the machinery between manifests and outcomes
//!
//! Three jobs:
//!
//! 1. **Discovery** — find the workspaces the lab can test, either from a
//!    local checkout ([`discovery::local`], via `Cargo.toml` workspace
//!    members) or from the crates.io sparse index
//!    ([`discovery::registry`], behind the `registry` feature).
//! 2. **Matching** — reconcile *what is installed / published* with *what
//!    the manifests declare*, and report version drift so the UI can say
//!    "this manifest describes 0.6.0 but the checkout is 0.6.1".
//! 3. **Driving** — [`Lab`] owns the sink (mock fleet today, hardware
//!    adapter behind the shell's toggle tomorrow), enforces destructive
//!    confirmation, books every outcome into the coverage ledger.

use lab_coverage::Ledger;
use lab_errors::{LabError, LabResult};
use lab_manifest::LabManifest;
use lab_mocks::{CommandSink, Invocation, SinkOutcome};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod discovery;
#[cfg(feature = "registry")]
pub mod registry;

/// One workspace member crate found on disk or in the registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrateInfo {
    pub name: String,
    pub version: String,
    /// Local path (empty for registry-only information).
    #[serde(default)]
    pub path: String,
}

/// A workspace discovered on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalWorkspace {
    pub name: String,
    pub root: String,
    pub members: Vec<CrateInfo>,
}

impl LocalWorkspace {
    pub fn member(&self, name: &str) -> Option<&CrateInfo> {
        self.members.iter().find(|m| m.name == name)
    }
}

/// How far a manifest has drifted from reality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Drift {
    /// Manifest version == the version being tested.
    InSync,
    /// The checkout/registry is newer than the manifest describes.
    ManifestBehind,
    /// The checkout is older; the lab may exercise stale behaviour.
    ManifestAhead,
    /// No version information available for comparison.
    Unknown,
}

/// Result of reconciling one workspace across sources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceMatch {
    pub workspace: String,
    pub prefix: String,
    pub manifest_version: String,
    #[serde(default)]
    pub local_version: Option<String>,
    #[serde(default)]
    pub registry_version: Option<String>,
    pub drift: Drift,
    /// Crates the discovery layer actually found for this workspace.
    pub crates_found: usize,
}

fn compare_semver(a: &str, b: &str) -> std::cmp::Ordering {
    let num = |s: &str| -> Vec<u64> {
        s.split('.')
            .map(|p| p.split(['-', '+']).next().unwrap_or("0").parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (a, b) = (num(a), num(b));
    for i in 0..3 {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

/// Match local workspaces + optional registry data against manifests.
pub fn match_workspaces(
    locals: &[LocalWorkspace],
    manifests: &[LabManifest],
    registry: Option<&BTreeMap<String, String>>,
) -> Vec<WorkspaceMatch> {
    manifests
        .iter()
        .map(|m| {
            let local = locals.iter().find(|w| w.name == m.workspace.name);
            let local_version = local.and_then(|w| w.member(&m.workspace.facade)).map(|c| c.version.clone());
            let registry_version = registry.and_then(|r| r.get(&m.workspace.facade).cloned());
            let reference = local_version.as_deref().or(registry_version.as_deref());
            let drift = match reference {
                None => Drift::Unknown,
                Some(v) => match compare_semver(&m.workspace.version, v) {
                    std::cmp::Ordering::Equal => Drift::InSync,
                    std::cmp::Ordering::Less => Drift::ManifestBehind,
                    std::cmp::Ordering::Greater => Drift::ManifestAhead,
                },
            };
            WorkspaceMatch {
                workspace: m.workspace.name.clone(),
                prefix: m.workspace.prefix.clone(),
                manifest_version: m.workspace.version.clone(),
                local_version,
                registry_version,
                drift,
                crates_found: local.map(|w| w.members.len()).unwrap_or(0),
            }
        })
        .collect()
}

/// The lab proper: manifests + a sink + the ledger.
pub struct Lab {
    manifests: BTreeMap<String, LabManifest>,
    sink: Box<dyn CommandSink>,
    ledger: Ledger,
    confirm_destructive: bool,
}

impl Lab {
    pub fn new(sink: Box<dyn CommandSink>, confirm_destructive: bool) -> Self {
        Self {
            manifests: BTreeMap::new(),
            sink,
            ledger: Ledger::new(),
            confirm_destructive,
        }
    }

    /// Toggle the destructive-command gate (the shell flips it for the
    /// duration of one confirmed invocation, then resets it).
    pub fn set_confirm_destructive(&mut self, allow: bool) {
        self.confirm_destructive = allow;
    }

    pub fn load_manifest(&mut self, manifest: LabManifest) -> LabResult<()> {
        manifest.validate()?;
        self.manifests
            .insert(manifest.workspace.name.clone(), manifest);
        Ok(())
    }

    pub fn manifest(&self, workspace: &str) -> LabResult<&LabManifest> {
        self.manifests
            .get(workspace)
            .ok_or_else(|| LabError::Manifest {
                path: workspace.into(),
                problem: "no manifest loaded for this workspace".into(),
            })
    }

    pub fn manifest_ids(&self) -> Vec<String> {
        self.manifests.keys().cloned().collect()
    }

    /// Direct invocation from the UI command console. Enforces confirmation
    /// for destructive commands and books the outcome as coverage.
    pub fn invoke(
        &mut self,
        workspace: &str,
        command_id: &str,
        args: BTreeMap<String, serde_json::Value>,
        scenario: Option<&str>,
    ) -> LabResult<SinkOutcome> {
        let manifest = self.manifest(workspace)?;
        let spec = manifest.command(command_id).ok_or_else(|| LabError::Manifest {
            path: format!("command '{command_id}'"),
            problem: "not declared in the manifest".into(),
        })?;
        if spec.requires_confirm() && !self.confirm_destructive {
            return Err(LabError::Scenario {
                step: command_id.into(),
                cause: "destructive command requires explicit confirmation".into(),
            });
        }
        let device = lab_mocks::pick_device_for(self.sink.as_ref(), command_id).ok_or_else(|| {
            LabError::Discovery { phase: "fleet".into(), message: "no devices".into() }
        })?;
        // Book coverage regardless of outcome — refusals are data too.
        // (Bucket computed before `args` moves into the invocation.)
        let bucket = {
            let mut parts: Vec<String> = args
                .iter()
                .filter(|(_, v)| v.is_string() || v.is_boolean() || v.is_i64())
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            if parts.is_empty() {
                "default".into()
            } else {
                parts.sort();
                parts.join(",")
            }
        };
        let inv = Invocation {
            device,
            command_id: command_id.into(),
            args,
            scenario: scenario.map(|s| s.into()),
        };
        let res = self.sink.execute(&inv);
        let rec = lab_coverage::CellRecord {
            outcome: match &res {
                Ok(_) => lab_coverage::Outcome::Pass,
                Err(e) => lab_coverage::Outcome::from_error(e),
            },
            at_ms: 0,
            duration_ms: res.as_ref().map(|o| o.duration_ms).unwrap_or(0),
            scenario: scenario.map(|s| s.into()),
            detail: res.as_ref().err().map(|e| e.to_string()),
        };
        let key = lab_coverage::CellKey {
            workspace: workspace.into(),
            command: command_id.into(),
            bucket,
        };
        let _ = self.ledger.record(key, rec);
        res
    }

    /// Run a whole scenario through the scenario runner (shares the ledger).
    pub fn run_scenario(&mut self, scenario: &lab_scenarios::Scenario) -> LabResult<lab_scenarios::ScenarioReport> {
        let manifest = self
            .manifests
            .values()
            .find(|m| {
                scenario
                    .steps
                    .iter()
                    .filter_map(|s| s.run.as_deref())
                    .next()
                    .map(|cmd| m.command(cmd).is_some())
                    .unwrap_or(false)
            })
            .ok_or_else(|| LabError::Manifest {
                path: scenario.name.clone(),
                problem: "no loaded manifest declares this scenario's first command".into(),
            })?
            .clone();
        let mut runner =
            lab_scenarios::ScenarioRunner::new(self.sink.as_ref(), &mut self.ledger, &manifest, self.confirm_destructive);
        Ok(runner.run(scenario))
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// The device port the lab drives. Exposed so the conformance suite runs
    /// its properties against the *same* fleet the console and scenarios use,
    /// rather than a lookalike built for the test.
    pub fn sink(&self) -> &dyn CommandSink {
        self.sink.as_ref()
    }

    pub fn ledger_mut(&mut self) -> &mut Ledger {
        &mut self.ledger
    }

    pub fn coverage_markdown(&self, workspace: &str) -> LabResult<String> {
        let m = self.manifest(workspace)?;
        Ok(self.ledger.to_markdown(m))
    }

    pub fn coverage_json(&self, workspace: &str) -> LabResult<serde_json::Value> {
        let m = self.manifest(workspace)?;
        Ok(self.ledger.to_json(m))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lab_mocks::MockFleet;

    const TV: &str = include_str!("../../../manifests/tv-rs.lab.toml");
    const CCTV: &str = include_str!("../../../manifests/cctv-rs.lab.toml");

    fn lab_with_manifests() -> Lab {
        let m = LabManifest::from_toml_str(TV).unwrap();
        let fleet = MockFleet::from_manifest(&m);
        let mut lab = Lab::new(Box::new(fleet), false);
        lab.load_manifest(LabManifest::from_toml_str(TV).unwrap()).unwrap();
        lab.load_manifest(LabManifest::from_toml_str(CCTV).unwrap()).unwrap();
        lab
    }

    #[test]
    fn invoke_books_coverage_and_enforces_confirm() {
        let mut lab = lab_with_manifests();
        let res = lab.invoke("tv-rs", "tv.power.wake", BTreeMap::new(), None);
        assert!(res.is_ok());
        // destructive without confirmation
        let res = lab.invoke("tv-rs", "tv.power.off", BTreeMap::new(), None);
        assert!(res.is_err());
        let md = lab.coverage_markdown("tv-rs").unwrap();
        assert!(md.contains("`tv.power.wake`"));
    }

    #[test]
    fn drift_detection() {
        let local = LocalWorkspace {
            name: "tv-rs".into(),
            root: "/tmp/tv-rs".into(),
            members: vec![CrateInfo {
                name: "tv-rs".into(),
                version: "0.6.1".into(),
                path: String::new(),
            }],
        };
        let m = LabManifest::from_toml_str(TV).unwrap();
        let matches = match_workspaces(&[local], &[m], None);
        assert_eq!(matches[0].drift, Drift::ManifestBehind);
        assert_eq!(matches[0].local_version.as_deref(), Some("0.6.1"));
        assert_eq!(matches[0].crates_found, 1);
    }
}
