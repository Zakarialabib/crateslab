//! # lab-scenarios — playlists the lab can perform
//!
//! A **scenario** is a timeline of steps. Two authoring styles, mixed
//! freely inside one playlist:
//!
//! 1. **Declarative steps** (`[[steps]]` in TOML): run a command, expect
//!    success / an error kind / a state, poll until it holds, with a
//!    per-step timeout. This is what the UI timeline renders and what the
//!    scenario composer edits.
//! 2. **Rhai steps** (`script = "..."` inline or `rhai = "file.rhai"`):
//!    generative logic — synthesize fleets, compute argument matrices,
//!    fuzz a limit. The sandbox is hard: bounded operations, no I/O, and
//!    the step's contract is a JSON verdict.
//!
//! Variables flow forward: any step (or script) may add to `vars`, and
//! later argument values of the form `"$vars.name"` are substituted.

use lab_coverage::{CellKey, CellRecord, Ledger, Outcome};
use lab_errors::{LabError, LabResult};
use lab_manifest::LabManifest;
use lab_mocks::{CommandSink, EmittedEvent, Invocation};
use rhai::{Engine, Scope};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Instant;

/// What a step asserts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expect {
    /// Command returns `Ok`.
    Ok,
    /// Command returns a specific error kind (capability, timeout, …).
    ErrorKind(String),
    /// After `Ok`, the device state must hold `key == value`.
    State { key: String, equals: Value },
}

/// Poll policy for eventually-consistent devices (the agent's world).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PollSpec {
    pub attempts: u32,
    pub interval_ms: u64,
}

/// One declarative step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Human name; defaults to the command id.
    #[serde(default)]
    pub name: Option<String>,
    /// Command id to invoke (`tv.power.wake`).
    #[serde(default)]
    pub run: Option<String>,
    /// Device id (must exist in the fleet the runner resolves).
    #[serde(default)]
    pub device: Option<String>,
    #[serde(default)]
    pub args: BTreeMap<String, Value>,
    #[serde(default)]
    pub expect: Option<Expect>,
    /// Per-step timeout in ms (over the manifest command's own budget).
    #[serde(default = "default_within")]
    pub within_ms: u64,
    #[serde(default)]
    pub poll: Option<PollSpec>,
    /// Inline Rhai step (ignored if `rhai` is set).
    #[serde(default)]
    pub script: Option<String>,
    /// Rhai file path, relative to the scenario file.
    #[serde(default)]
    pub rhai: Option<String>,
}

fn default_within() -> u64 {
    5_000
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnFail {
    #[default]
    Abort,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    pub name: String,
    /// `mock` or `lan` (hardware toggle lives in the shell, not the file).
    #[serde(default = "default_target")]
    pub target: String,
    #[serde(default)]
    pub on_fail: OnFail,
    #[serde(default)]
    pub vars: BTreeMap<String, Value>,
    #[serde(default)]
    pub steps: Vec<Step>,
}

fn default_target() -> String {
    "mock".into()
}

impl Scenario {
    /// Parse the file format:
    ///
    /// ```toml
    /// [scenario]
    /// name = "…"        # + target / on_fail / vars
    ///
    /// [[steps]]
    /// run = "…"
    /// ```
    pub fn from_toml_str(s: &str) -> LabResult<Self> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ScenarioDoc {
            scenario: ScenarioHead,
            #[serde(default)]
            steps: Vec<Step>,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ScenarioHead {
            name: String,
            #[serde(default = "default_target")]
            target: String,
            #[serde(default)]
            on_fail: OnFail,
            #[serde(default)]
            vars: BTreeMap<String, Value>,
        }
        let doc: ScenarioDoc = toml::from_str(s).map_err(|e| LabError::Scenario {
            step: "<parse>".into(),
            cause: e.to_string(),
        })?;
        Ok(Scenario {
            name: doc.scenario.name,
            target: doc.scenario.target,
            on_fail: doc.scenario.on_fail,
            vars: doc.scenario.vars,
            steps: doc.steps,
        })
    }

    /// Load a Rhai step from a file path (relative to the scenario root).
    pub fn load_script(&self, root: &std::path::Path, rel: &str) -> LabResult<String> {
        std::fs::read_to_string(root.join(rel)).map_err(|e| LabError::Script {
            script: rel.into(),
            message: e.to_string(),
        })
    }
}

/// One executed step, ready for a UI timeline row.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepResult {
    pub name: String,
    #[serde(default)]
    pub command: Option<String>,
    /// `pass | fail | skipped`
    pub status: &'static str,
    #[serde(default)]
    pub outcome: Option<Outcome>,
    #[serde(default)]
    pub detail: Option<String>,
    pub duration_ms: u64,
    #[serde(default)]
    pub events: Vec<EmittedEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScenarioReport {
    pub name: String,
    pub ok: bool,
    pub steps: Vec<StepResult>,
    pub duration_ms: u64,
    #[serde(default)]
    pub vars: BTreeMap<String, Value>,
}

/// Runs scenarios against any [`CommandSink`] and books coverage.
pub struct ScenarioRunner<'a> {
    sink: &'a dyn CommandSink,
    ledger: &'a mut Ledger,
    manifest: &'a LabManifest,
    confirm_destructive: bool,
}

impl<'a> ScenarioRunner<'a> {
    pub fn new(
        sink: &'a dyn CommandSink,
        ledger: &'a mut Ledger,
        manifest: &'a LabManifest,
        confirm_destructive: bool,
    ) -> Self {
        Self { sink, ledger, manifest, confirm_destructive }
    }

    /// Prefer a device that actually exposes the command's capability — the
    /// same rule the engine's direct `invoke` uses, so the console and the
    /// scenario runner never disagree about which device serves a command.
    fn pick_device(&self, command_id: &str) -> Option<String> {
        lab_mocks::pick_device_for(self.sink, command_id)
    }

    fn bucket_for(_command_id: &str, args: &BTreeMap<String, Value>) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (k, v) in args {
            if v.is_string() || v.is_boolean() || v.is_i64() {
                parts.push(format!("{k}={v}"));
            }
        }
        if parts.is_empty() {
            "default".into()
        } else {
            parts.join(",")
        }
    }

    fn substitute(args: &BTreeMap<String, Value>, vars: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
        args.iter()
            .map(|(k, v)| {
                if let Some(s) = v.as_str().and_then(|s| s.strip_prefix("$vars.")) {
                    (k.clone(), vars.get(s).cloned().unwrap_or(Value::Null))
                } else {
                    (k.clone(), v.clone())
                }
            })
            .collect()
    }

    fn invoke_once(
        &self,
        step: &Step,
        vars: &BTreeMap<String, Value>,
    ) -> LabResult<lab_mocks::SinkOutcome> {
        let command_id = step.run.clone().ok_or_else(|| LabError::Scenario {
            step: step.name.clone().unwrap_or_default(),
            cause: "declarative step has no `run`".into(),
        })?;
        let spec = self.manifest.command(&command_id).ok_or_else(|| LabError::Manifest {
            path: format!("command '{command_id}'"),
            problem: "scenario references a command the manifest does not declare".into(),
        })?
        .clone();
        if spec.requires_confirm() && !self.confirm_destructive {
            return Err(LabError::Scenario {
                step: step.name.clone().unwrap_or_else(|| command_id.clone()),
                cause: format!("'{command_id}' is destructive; pass confirm_destructive=true to allow it"),
            });
        }
        let device = match step.device.clone() {
            Some(d) => d,
            None => self.pick_device(&command_id).ok_or_else(|| LabError::Discovery {
                phase: "fleet".into(),
                message: "no devices in fleet".into(),
            })?,
        };
        let args = Self::substitute(&step.args, vars);
        let inv = Invocation { device, command_id, args, scenario: None };
        self.sink.execute(&inv)
    }

    fn check_expect(
        &self,
        step_name: &str,
        expect: &Expect,
        out: &lab_mocks::SinkOutcome,
    ) -> Result<(), String> {
        match expect {
            Expect::Ok => Ok(()),
            Expect::State { key, equals } => match out.state.get(key) {
                Some(v) if v == equals => Ok(()),
                Some(v) => Err(format!(
                    "state '{key}' = {v}, expected {equals}"
                )),
                None => Err(format!("state key '{key}' missing after the call")),
            },
            Expect::ErrorKind(_) => Err(format!(
                "step '{step_name}' expected an error but the command succeeded"
            )),
        }
    }

    /// Execute one scenario end-to-end. Never panics; failures land in the
    /// report and (per `on_fail`) either abort or continue.
    pub fn run(&mut self, scenario: &Scenario) -> ScenarioReport {
        let started = Instant::now();
        let mut vars = scenario.vars.clone();
        let mut steps_out: Vec<StepResult> = Vec::new();
        let mut ok = true;

        let engine = {
            let mut e = Engine::new();
            e.set_max_operations(200_000);
            e.set_max_string_size(2_000_000);
            e
        };

        for (idx, step) in scenario.steps.iter().enumerate() {
            let step_name = step
                .name
                .clone()
                .or_else(|| step.run.clone())
                .unwrap_or_else(|| format!("step-{}", idx + 1));

            // ---- Rhai step -------------------------------------------------
            if step.rhai.is_some() || step.script.is_some() {
                let source = if let Some(rel) = &step.rhai {
                    scenario.load_script(std::path::Path::new("."), rel)
                } else {
                    Ok(step.script.clone().unwrap_or_default())
                };
                let result = source.and_then(|src| {
                    let ast = engine.compile(&src).map_err(|e| LabError::Script {
                        script: step_name.clone(),
                        message: e.to_string(),
                    })?;
                    let vars_json = serde_json::to_string(&vars).unwrap_or_else(|_| "{}".into());
                    let mut scope = Scope::new();
                    let out: String = engine
                        .call_fn(&mut scope, &ast, "step", (vars_json,))
                        .map_err(|e| LabError::Script {
                            script: step_name.clone(),
                            message: e.to_string(),
                        })?;
                    #[derive(Deserialize)]
                    struct Verdict {
                        #[serde(default)]
                        vars: BTreeMap<String, Value>,
                        #[serde(default)]
                        verdict: String,
                        #[serde(default)]
                        message: Option<String>,
                    }
                    let v: Verdict = serde_json::from_str(&out).map_err(|e| LabError::Script {
                        script: step_name.clone(),
                        message: format!("verdict is not lab-JSON: {e}"),
                    })?;
                    for (k, val) in v.vars {
                        vars.insert(k, val);
                    }
                    if v.verdict == "pass" {
                        Ok(())
                    } else {
                        Err(LabError::Scenario {
                            step: step_name.clone(),
                            cause: v.message.unwrap_or_else(|| "script verdict = fail".into()),
                        })
                    }
                });
                match result {
                    Ok(()) => steps_out.push(StepResult {
                        name: step_name,
                        command: None,
                        status: "pass",
                        outcome: None,
                        detail: None,
                        duration_ms: 0,
                        events: vec![],
                    }),
                    Err(e) => {
                        ok = false;
                        steps_out.push(StepResult {
                            name: step_name,
                            command: None,
                            status: "fail",
                            outcome: Some(Outcome::Failed),
                            detail: Some(e.to_string()),
                            duration_ms: 0,
                            events: vec![],
                        });
                        if scenario.on_fail == OnFail::Abort {
                            break;
                        }
                    }
                }
                continue;
            }

            // ---- Declarative step ------------------------------------------
            let mut attempts = 1;
            let mut interval_ms = 0;
            if let Some(p) = &step.poll {
                attempts = p.attempts.max(1);
                interval_ms = p.interval_ms;
            }

            let mut final_res: StepResult = StepResult {
                name: step_name.clone(),
                command: step.run.clone(),
                status: "fail",
                outcome: None,
                detail: None,
                duration_ms: 0,
                events: vec![],
            };

            for attempt in 0..attempts {
                if attempt > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(interval_ms));
                }
                let call = self.invoke_once(step, &vars);
                let elapsed = started.elapsed().as_millis() as u64;
                match call {
                    Ok(out) => {
                        let expect = step.expect.clone().unwrap_or(Expect::Ok);
                        match self.check_expect(&step_name, &expect, &out) {
                            Ok(()) => {
                                // book coverage: pass
                                if let Some(cmd) = &step.run {
                                    let key = CellKey {
                                        workspace: self.manifest.workspace.name.clone(),
                                        command: cmd.clone(),
                                        bucket: Self::bucket_for(cmd, &step.args),
                                    };
                                    let rec = CellRecord {
                                        outcome: Outcome::Pass,
                                        at_ms: elapsed,
                                        duration_ms: out.duration_ms,
                                        scenario: Some(scenario.name.clone()),
                                        detail: None,
                                    };
                                    let _ = self.ledger.record(key, rec);
                                }
                                for ev in &out.events {
                                    vars.insert(
                                        format!("last_event_{}", ev.id.replace('.', "_")),
                                        ev.payload.clone(),
                                    );
                                }
                                final_res = StepResult {
                                    name: step_name,
                                    command: step.run.clone(),
                                    status: "pass",
                                    outcome: Some(Outcome::Pass),
                                    detail: None,
                                    duration_ms: out.duration_ms,
                                    events: out.events,
                                };
                                break;
                            }
                            Err(msg) => {
                                if attempt + 1 == attempts {
                                    final_res.detail = Some(msg);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        // book coverage for the attempted command
                        if let Some(cmd) = &step.run {
                            let key = CellKey {
                                workspace: self.manifest.workspace.name.clone(),
                                command: cmd.clone(),
                                bucket: Self::bucket_for(cmd, &step.args),
                            };
                            let rec = CellRecord {
                                outcome: Outcome::from_error(&e),
                                at_ms: elapsed,
                                duration_ms: 0,
                                scenario: Some(scenario.name.clone()),
                                detail: Some(e.to_string()),
                            };
                            let _ = self.ledger.record(key, rec);
                        }
                        // error-kind expectation satisfied?
                        if let Some(Expect::ErrorKind(kind)) = &step.expect {
                            if e.kind().as_str() == kind.as_str() {
                                final_res = StepResult {
                                    name: step_name,
                                    command: step.run.clone(),
                                    status: "pass",
                                    outcome: Some(Outcome::from_error(&e)),
                                    detail: Some(format!("expected error observed: {e}")),
                                    duration_ms: 0,
                                    events: vec![],
                                };
                                break;
                            }
                        }
                        if attempt + 1 == attempts {
                            final_res.outcome = Some(Outcome::from_error(&e));
                            final_res.detail = Some(e.to_string());
                        }
                    }
                }
            }

            if final_res.status != "pass" {
                ok = false;
                if scenario.on_fail == OnFail::Abort {
                    steps_out.push(final_res);
                    break;
                }
            }
            steps_out.push(final_res);
        }

        ScenarioReport {
            name: scenario.name.clone(),
            ok,
            steps: steps_out,
            duration_ms: started.elapsed().as_millis() as u64,
            vars,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lab_mocks::MockFleet;

    const MANIFEST: &str = include_str!("../../../manifests/tv-rs.lab.toml");
    const SCENARIO: &str = r#"
[scenario]
name = "wake-check"
target = "mock"
on_fail = "abort"
vars = { mac = "AA:BB:CC:DD:EE:FF" }

[[steps]]
run = "tv.power.wake"
args = { mac = "$vars.mac" }
expect = "ok"

[[steps]]
run = "tv.power.state"
expect = { state = { key = "power", equals = "On" } }
poll = { attempts = 3, interval_ms = 1 }

[[steps]]
run = "tv.panel.orientation"
device = "tv-01"
expect = { error_kind = "capability" }
"#;

    #[test]
    fn declarative_scenario_passes_and_books_coverage() {
        let m = LabManifest::from_toml_str(MANIFEST).unwrap();
        let fleet = MockFleet::from_manifest(&m);
        let sc = Scenario::from_toml_str(SCENARIO).unwrap();
        let mut ledger = Ledger::new();
        let mut runner = ScenarioRunner::new(&fleet, &mut ledger, &m, false);
        let report = runner.run(&sc);
        assert!(report.ok, "{:?}", report.steps);
        assert_eq!(report.steps.len(), 3);
        assert!(report.steps.iter().all(|s| s.status == "pass"));
        // the refused capability step also books coverage
        let pct = ledger.coverage_percent(&m);
        assert!(pct > 0.0, "coverage must be > 0, got {pct}");
    }

    #[test]
    fn destructive_requires_confirmation() {
        let m = LabManifest::from_toml_str(MANIFEST).unwrap();
        let fleet = MockFleet::from_manifest(&m);
        let sc = Scenario::from_toml_str(
            r#"
[scenario]
name = "no-confirm"

[[steps]]
run = "tv.power.off"
"#,
        )
        .unwrap();
        let mut ledger = Ledger::new();
        let mut runner = ScenarioRunner::new(&fleet, &mut ledger, &m, false);
        let report = runner.run(&sc);
        assert!(!report.ok);
        assert!(report.steps[0].detail.as_deref().unwrap().contains("destructive"));
    }

    #[test]
    fn on_fail_continue_records_then_proceeds() {
        let m = LabManifest::from_toml_str(MANIFEST).unwrap();
        let fleet = MockFleet::from_manifest(&m);
        let sc = Scenario::from_toml_str(
            r#"
[scenario]
name = "continue-mode"
on_fail = "continue"

[[steps]]
run = "tv.does.not_exist"

[[steps]]
run = "tv.power.wake"
"#,
        )
        .unwrap();
        let mut ledger = Ledger::new();
        let mut runner = ScenarioRunner::new(&fleet, &mut ledger, &m, false);
        let report = runner.run(&sc);
        assert!(!report.ok);
        assert_eq!(report.steps.len(), 2);
        assert_eq!(report.steps[1].status, "pass");
    }

    #[test]
    fn unknown_step_field_fails_loudly() {
        // Regression: this exact typo used to be silently ignored, so the
        // state assertion never ran and the step passed for the wrong reason.
        let bad = r#"
[scenario]
name = "typo"

[[steps]]
run = "tv.power.wake"
expect_state = { key = "power", equals = "On" }
"#;
        let err = Scenario::from_toml_str(bad).unwrap_err();
        assert!(err.to_string().contains("expect_state"), "{err}");
    }

    #[test]
    fn rhai_step_feeds_vars_forward() {
        let m = LabManifest::from_toml_str(MANIFEST).unwrap();
        let fleet = MockFleet::from_manifest(&m);
        let sc = Scenario::from_toml_str(
            r#"
[scenario]
name = "generative"

[[steps]]
script = '''
fn step(vars) {
    #{
        vars: #{ vendor: "roku", ip: "192.168.1.40" },
        verdict: "pass",
    }.to_json()
}
'''

[[steps]]
run = "tv.key.press"
args = { key = "home" }
expect = "ok"
"#,
        )
        .unwrap();
        let mut ledger = Ledger::new();
        let mut runner = ScenarioRunner::new(&fleet, &mut ledger, &m, false);
        let report = runner.run(&sc);
        assert!(report.ok, "{:?}", report.steps);
        assert_eq!(report.vars.get("vendor"), Some(&Value::String("roku".into())));
    }
}
