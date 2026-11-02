//! # lab-conformance — the declared surface, executed as properties
//!
//! A manifest already says what a command accepts. That declaration is also a
//! **generator**: this crate reads each argument's type and bounds and emits a
//! deterministic set of invocations — the declared defaults, the boundaries
//! (`min`, `max`, `min_len`, `max_len`, and one step past each), every declared
//! enum variant, and a family of deliberately hostile values per type. Because
//! nothing here uses an RNG, a failing run replays identically.
//!
//! Two properties are then asserted for every command:
//!
//! * **Zero panic** — no argument combination may unwind the sink. This is the
//!   README's promise made executable.
//! * **Declared invariants** — today `idempotent`: two calls with the same
//!   arguments must have the same effect.
//!
//! The classification discipline matters as much as the generation. A *typed*
//! outcome — a capability refusal, a rate limit, a timeout, a script error — is
//! a **pass**, because refusals are part of the API and hostile input is
//! *supposed* to be rejected. Only an unwind, or [`LabErrorKind::Internal`]
//! (the lab admitting a bug), is a violation. Without that rule the suite would
//! drown in false positives, which is the failure mode the plan warns about.

use lab_errors::LabErrorKind;
use lab_manifest::{ArgSpec, ArgType, CommandSpec, ConformanceTier, Invariant, LabManifest};
use lab_mocks::{CommandSink, Invocation, SinkOutcome};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Upper bound on the cases generated for one command.
///
/// Generation is unbounded by nature (one case per boundary per argument), and
/// a wide surface must not turn a gate run into a soak test. Cases are ordered
/// most-informative-first, so truncation keeps the signal.
pub const MAX_CASES: usize = 96;

/// Which family of inputs a case came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseKind {
    /// Declared defaults / typical in-range values.
    Baseline,
    /// At, and one step past, a declared bound.
    Boundary,
    /// One case per declared enum variant.
    EnumSweep,
    /// Deliberately wrong, oversized or malformed input.
    Hostile,
}

/// One generated invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyCase {
    pub kind: CaseKind,
    /// Human-readable reason this case exists, e.g. `level: one past declared max`.
    pub rationale: String,
    pub args: BTreeMap<String, Value>,
}

/// A property that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    /// The sink unwound on a manifest-shaped input. Zero-panic is broken.
    Panic,
    /// The lab reported [`LabErrorKind::Internal`]: a bug, not a refusal.
    InternalError,
    /// A declared invariant did not hold.
    Invariant,
}

/// One failed property, with the input that triggered it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Violation {
    pub kind: ViolationKind,
    /// Set when `kind` is [`ViolationKind::Invariant`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invariant: Option<Invariant>,
    pub case: CaseKind,
    pub rationale: String,
    pub args: BTreeMap<String, Value>,
    pub detail: String,
}

/// Property results for one command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandReport {
    pub command: String,
    pub tier: ConformanceTier,
    /// Total cases generated (after truncation).
    pub cases: usize,
    /// Cases the sink refused or limited in a typed way — expected, not a bug.
    pub typed_outcomes: usize,
    /// Cases where an invariant could not be asserted (e.g. the second call of
    /// an idempotence check was itself refused).
    pub not_asserted: usize,
    pub violations: Vec<Violation>,
}

impl CommandReport {
    pub fn ok(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Property results for one workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyReport {
    pub workspace: String,
    pub commands: Vec<CommandReport>,
}

impl PropertyReport {
    pub fn cases(&self) -> usize {
        self.commands.iter().map(|c| c.cases).sum()
    }

    pub fn violations(&self) -> usize {
        self.commands.iter().map(|c| c.violations.len()).sum()
    }

    /// Violations on commands the workspace marked `must` — the ones a release
    /// gate should treat as blocking.
    pub fn must_violations(&self) -> usize {
        self.commands
            .iter()
            .filter(|c| c.tier == ConformanceTier::Must)
            .map(|c| c.violations.len())
            .sum()
    }

    pub fn ok(&self) -> bool {
        self.violations() == 0
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|e| json!({ "error": e.to_string() }))
    }

    /// A compact console report: one line per command, then any violation with
    /// the input that caused it.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "# conformance: {} — {} case(s), {} violation(s)",
            self.workspace,
            self.cases(),
            self.violations()
        );
        for c in &self.commands {
            let _ = writeln!(
                out,
                "{} `{}` [{:?}] — {} case(s), {} typed outcome(s), {} violation(s)",
                if c.ok() { "ok  " } else { "FAIL" },
                c.command,
                c.tier,
                c.cases,
                c.typed_outcomes,
                c.violations.len()
            );
        }
        for c in &self.commands {
            for v in &c.violations {
                let _ = writeln!(
                    out,
                    "  ! {} on `{}` ({}): {}\n    input: {}",
                    match v.kind {
                        ViolationKind::Panic => "panic",
                        ViolationKind::InternalError => "internal error",
                        ViolationKind::Invariant => "invariant",
                    },
                    c.command,
                    v.rationale,
                    v.detail,
                    serde_json::to_string(&v.args).unwrap_or_else(|_| "{}".into()),
                );
            }
        }
        out
    }
}

/// Run every declared command's properties against a sink.
///
/// Refusals are classified before assertion, so a typed refusal passes.
pub fn run(manifest: &LabManifest, sink: &dyn CommandSink) -> PropertyReport {
    PropertyReport {
        workspace: manifest.workspace.name.clone(),
        commands: manifest.commands.iter().map(|spec| run_command(spec, sink)).collect(),
    }
}

/// Run one command's properties.
pub fn run_command(spec: &CommandSpec, sink: &dyn CommandSink) -> CommandReport {
    let cases = cases_for(spec);
    let device = lab_mocks::pick_device_for(sink, &spec.id);
    let mut violations = Vec::new();
    let mut typed_outcomes = 0usize;
    let mut not_asserted = 0usize;

    for case in &cases {
        let Some(device) = device.clone() else {
            violations.push(Violation {
                kind: ViolationKind::InternalError,
                invariant: None,
                case: case.kind,
                rationale: case.rationale.clone(),
                args: case.args.clone(),
                detail: "no device in the fleet can serve this command".into(),
            });
            continue;
        };
        let inv = Invocation {
            device,
            command_id: spec.id.clone(),
            args: case.args.clone(),
            scenario: Some("conformance".into()),
        };

        // A spent bucket would refuse every remaining case, so each case gets a
        // clean limiter and the input actually reaches the command body.
        sink.reset_limits();
        match call(sink, &inv) {
            Call::Panicked(payload) => violations.push(Violation {
                kind: ViolationKind::Panic,
                invariant: None,
                case: case.kind,
                rationale: case.rationale.clone(),
                args: case.args.clone(),
                detail: payload,
            }),
            Call::Returned(Err(e)) if e.kind() == LabErrorKind::Internal => {
                violations.push(Violation {
                    kind: ViolationKind::InternalError,
                    invariant: None,
                    case: case.kind,
                    rationale: case.rationale.clone(),
                    args: case.args.clone(),
                    detail: e.to_string(),
                });
            }
            Call::Returned(Err(_)) => typed_outcomes += 1,
            Call::Returned(Ok(first)) => {
                if !spec.has_invariant(Invariant::Idempotent) {
                    continue;
                }
                sink.reset_limits();
                match call(sink, &inv) {
                    Call::Panicked(payload) => violations.push(Violation {
                        kind: ViolationKind::Panic,
                        invariant: Some(Invariant::Idempotent),
                        case: case.kind,
                        rationale: case.rationale.clone(),
                        args: case.args.clone(),
                        detail: payload,
                    }),
                    // An internal error is a violation wherever it lands — the
                    // repeat call is not exempt just because it is a repeat.
                    Call::Returned(Err(e)) if e.kind() == LabErrorKind::Internal => {
                        violations.push(Violation {
                            kind: ViolationKind::InternalError,
                            invariant: Some(Invariant::Idempotent),
                            case: case.kind,
                            rationale: case.rationale.clone(),
                            args: case.args.clone(),
                            detail: e.to_string(),
                        });
                    }
                    // The repeat was refused: the invariant simply does not
                    // apply here, so it is not asserted rather than "passed".
                    Call::Returned(Err(_)) => not_asserted += 1,
                    Call::Returned(Ok(second)) => {
                        if !same_effect(&first, &second) {
                            violations.push(Violation {
                                kind: ViolationKind::Invariant,
                                invariant: Some(Invariant::Idempotent),
                                case: case.kind,
                                rationale: case.rationale.clone(),
                                args: case.args.clone(),
                                detail: format!(
                                    "idempotent, but the second call differed: {} -> {}",
                                    short(&first),
                                    short(&second)
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    CommandReport {
        command: spec.id.clone(),
        tier: spec.tier(),
        cases: cases.len(),
        typed_outcomes,
        not_asserted,
        violations,
    }
}

enum Call {
    /// Unwound; carries the panic payload rendered as text.
    Panicked(String),
    Returned(lab_errors::LabResult<SinkOutcome>),
}

fn call(sink: &dyn CommandSink, inv: &Invocation) -> Call {
    match catch_unwind(AssertUnwindSafe(|| sink.execute(inv))) {
        Ok(res) => Call::Returned(res),
        Err(payload) => {
            let detail = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "panic with a non-string payload".into());
            Call::Panicked(detail)
        }
    }
}

/// What "the same result" means: the observable effect, minus wall-clock
/// duration, which is never reproducible.
fn same_effect(a: &SinkOutcome, b: &SinkOutcome) -> bool {
    a.value == b.value && a.events == b.events && a.state == b.state
}

fn short(o: &SinkOutcome) -> String {
    serde_json::to_string(&o.value).unwrap_or_else(|_| "<unprintable>".into())
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

/// Generate the deterministic case set for one command.
pub fn cases_for(command: &CommandSpec) -> Vec<PropertyCase> {
    let baseline: BTreeMap<String, Value> = command
        .args
        .iter()
        .map(|a| (a.name.clone(), baseline_value(a)))
        .collect();

    let mut cases = vec![PropertyCase {
        kind: CaseKind::Baseline,
        rationale: "declared defaults".into(),
        args: baseline.clone(),
    }];

    // Worst case first, so truncation cannot drop it: every argument hostile
    // at once is the combination most likely to find a panic.
    if !command.args.is_empty() {
        let args = command
            .args
            .iter()
            .map(|a| {
                let (_, v) = hostile_values(a).into_iter().next().expect("every type is hostile-able");
                (a.name.clone(), v)
            })
            .collect();
        cases.push(PropertyCase {
            kind: CaseKind::Hostile,
            rationale: "every argument hostile at once".into(),
            args,
        });
    }

    for arg in &command.args {
        let single = |kind: CaseKind, rationale: String, value: Value| {
            let mut args = baseline.clone();
            args.insert(arg.name.clone(), value);
            PropertyCase { kind, rationale, args }
        };

        for (why, value) in boundary_values(arg) {
            cases.push(single(CaseKind::Boundary, format!("{}: {why}", arg.name), value));
        }
        for option in &arg.options {
            cases.push(single(
                CaseKind::EnumSweep,
                format!("{}: declared variant '{option}'", arg.name),
                json!(option),
            ));
        }
        for (why, value) in hostile_values(arg) {
            cases.push(single(CaseKind::Hostile, format!("{}: {why}", arg.name), value));
        }
    }

    cases.truncate(MAX_CASES);
    cases
}

/// A typical valid value: the declared default when there is one, otherwise
/// something comfortably inside the declared bounds.
fn baseline_value(arg: &ArgSpec) -> Value {
    if let Some(d) = &arg.default {
        return d.clone();
    }
    match arg.r#type {
        ArgType::Str => fill("sample", arg),
        ArgType::Int => f64_value(inside(arg, 1.0)),
        ArgType::Float => f64_value(inside(arg, 1.0)),
        ArgType::Bool => json!(true),
        ArgType::Enum => arg.options.first().map(|o| json!(o)).unwrap_or(Value::Null),
        ArgType::Ipv4 => fill("192.168.1.40", arg),
        ArgType::Mac => fill("aa:bb:cc:dd:ee:ff", arg),
        ArgType::Url => fill("http://127.0.0.1/", arg),
        ArgType::Path => fill("/tmp/sample", arg),
        ArgType::Json => json!({}),
    }
}

/// Clamp `wanted` into any declared numeric bounds.
fn inside(arg: &ArgSpec, wanted: f64) -> f64 {
    let lo = arg.min.unwrap_or(f64::NEG_INFINITY);
    let hi = arg.max.unwrap_or(f64::INFINITY);
    wanted.clamp(lo, hi)
}

/// Pad or trim a seed string so it satisfies any declared length bounds.
fn fill(seed: &str, arg: &ArgSpec) -> Value {
    let min = arg.min_len.unwrap_or(0);
    let max = arg.max_len.unwrap_or(usize::MAX);
    if max == 0 {
        return json!("");
    }
    let mut s = seed.to_string();
    while s.len() < min {
        s.push('x');
    }
    if s.len() > max {
        s.truncate(max);
    }
    Value::String(s)
}

/// At, and one step past, every declared bound.
fn boundary_values(arg: &ArgSpec) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    if let Some(hi) = arg.max {
        out.push(("at declared max".into(), f64_value(hi)));
        out.push(("one past declared max".into(), f64_value(hi + 1.0)));
    }
    if let Some(lo) = arg.min {
        out.push(("at declared min".into(), f64_value(lo)));
        out.push(("one below declared min".into(), f64_value(lo - 1.0)));
    }
    if let Some(n) = arg.max_len {
        out.push((format!("length {n} (max_len)"), json!("x".repeat(n))));
        out.push((format!("length {} (one past max_len)", n + 1), json!("x".repeat(n + 1))));
    }
    if let Some(n) = arg.min_len {
        if n > 0 {
            out.push((format!("length {} (one below min_len)", n - 1), json!("x".repeat(n - 1))));
        }
        out.push((format!("length {n} (min_len)"), json!("x".repeat(n))));
    }
    out
}

/// Deliberately wrong, oversized or malformed values for one argument.
fn hostile_values(arg: &ArgSpec) -> Vec<(&'static str, Value)> {
    match arg.r#type {
        ArgType::Str => vec![
            ("empty string", json!("")),
            ("control bytes", json!("\u{0}\u{1}\u{7f}")),
            ("oversized payload", json!("x".repeat(4096))),
            ("non-ascii", json!("漢字😀")),
        ],
        ArgType::Int => vec![
            ("i64::MAX", json!(i64::MAX)),
            ("i64::MIN", json!(i64::MIN)),
            ("negative", json!(-1)),
        ],
        ArgType::Float => vec![
            ("f64::MAX", json!(f64::MAX)),
            ("f64::MIN", json!(f64::MIN)),
            ("negative zero", json!(-0.0)),
            ("subnormal", json!(5e-324)),
        ],
        ArgType::Bool => vec![
            ("null", Value::Null),
            ("string instead of bool", json!("true")),
        ],
        ArgType::Enum => vec![
            ("empty variant", json!("")),
            ("unknown variant", json!("__not_a_declared_variant__")),
        ],
        ArgType::Ipv4 => vec![
            ("octet overflow", json!("999.999.999.999")),
            ("empty string", json!("")),
            ("ipv6 literal", json!("::1")),
            ("cidr suffix", json!("192.168.1.40/24")),
        ],
        ArgType::Mac => vec![
            ("non-hex digits", json!("zz:zz:zz:zz:zz:zz")),
            ("empty string", json!("")),
            ("no separators", json!("aabbccddeeff")),
        ],
        ArgType::Url => vec![
            ("empty string", json!("")),
            ("not a url", json!("not a url")),
            ("file scheme", json!("file:///etc/passwd")),
            ("port out of range", json!("http://[::1]:99999/")),
        ],
        ArgType::Path => vec![
            ("empty string", json!("")),
            ("filesystem root", json!("/")),
            ("directory traversal", json!("../../../../etc/passwd")),
            ("embedded nul", json!("a\u{0}b")),
        ],
        ArgType::Json => vec![
            ("unterminated array", json!("[")),
            ("null", Value::Null),
            ("array where object expected", json!([1, 2, 3])),
        ],
    }
}

/// Render an `f64` as JSON, preferring integers so `int` arguments stay
/// integers on the wire.
fn f64_value(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 9_007_199_254_740_992.0 {
        Value::Number(serde_json::Number::from(f as i64))
    } else {
        serde_json::Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lab_errors::{LabError, LabResult};
    use std::sync::atomic::{AtomicUsize, Ordering};

    const TV: &str = include_str!("../../../manifests/tv-rs.lab.toml");

    fn manifest() -> LabManifest {
        LabManifest::from_toml_str(TV).unwrap()
    }

    fn command(id: &str) -> CommandSpec {
        manifest().command(id).unwrap().clone()
    }

    /// A sink that can be told to misbehave in exactly one way, so the
    /// classifier can be tested without hunting for a real bug.
    struct Stub {
        mode: Mode,
        calls: AtomicUsize,
        fail_on: usize,
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Refuse,
        Panic,
        Internal,
        NonIdempotent,
    }

    impl Stub {
        fn new(mode: Mode) -> Self {
            Self { mode, calls: AtomicUsize::new(0), fail_on: 1 }
        }
        fn failing_on(mode: Mode, fail_on: usize) -> Self {
            Self { mode, calls: AtomicUsize::new(0), fail_on }
        }
    }

    impl CommandSink for Stub {
        fn execute(&self, _inv: &Invocation) -> LabResult<SinkOutcome> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            let hit = n == self.fail_on;
            match self.mode {
                Mode::Refuse => Err(LabError::Capability {
                    device: "stub".into(),
                    command: _inv.command_id.clone(),
                }),
                Mode::Panic if hit => panic!("sink exploded on hostile input"),
                Mode::Internal if hit => Err(LabError::Internal { bug: "counted wrong".into() }),
                Mode::NonIdempotent if !hit => Ok(SinkOutcome {
                    value: json!({ "call": n }),
                    events: Vec::new(),
                    duration_ms: 0,
                    state: BTreeMap::new(),
                }),
                _ => Ok(SinkOutcome {
                    value: json!({ "call": 1 }),
                    events: Vec::new(),
                    duration_ms: 0,
                    state: BTreeMap::new(),
                }),
            }
        }

        fn capabilities_of(&self, _device: &str) -> Option<Vec<String>> {
            Some(vec!["power".into(), "volume".into(), "key".into(), "launch".into(), "media".into(), "panel".into()])
        }

        fn list_devices(&self) -> Vec<String> {
            vec!["tv-01".into()]
        }
    }

    #[test]
    fn bounds_drive_boundary_cases() {
        let cmd = command("tv.volume.set");
        let level = cmd.args.iter().find(|a| a.name == "level").unwrap();
        // The authored manifest bounds volume at 0..=100.
        assert_eq!(level.min, Some(0.0));
        assert_eq!(level.max, Some(100.0));

        let cases = cases_for(&cmd);
        let boundaries: Vec<&PropertyCase> =
            cases.iter().filter(|c| c.kind == CaseKind::Boundary).collect();
        let values: Vec<Value> =
            boundaries.iter().map(|c| c.args["level"].clone()).collect();
        for expected in [json!(0), json!(100), json!(-1), json!(101)] {
            assert!(values.contains(&expected), "missing boundary {expected}: {values:?}");
        }
    }

    #[test]
    fn hostile_cases_are_generated_for_every_type() {
        let cmd = command("tv.power.off");
        let cases = cases_for(&cmd);
        assert!(cases.iter().any(|c| c.kind == CaseKind::Hostile), "no hostile cases");
        assert!(
            cases.iter().any(|c| c.args.get("ip").is_some_and(|v| v == &json!("999.999.999.999"))),
            "ipv4 must get an octet-overflow hostile case"
        );
        assert!(cases.iter().any(|c| c.rationale.contains("every argument hostile at once")));
    }

    #[test]
    fn enum_sweep_visits_every_declared_variant() {
        let cmd = command("tv.key.press");
        let cases = cases_for(&cmd);
        let swept: Vec<String> = cases
            .iter()
            .filter(|c| c.kind == CaseKind::EnumSweep && c.rationale.starts_with("key:"))
            .map(|c| c.args["key"].as_str().unwrap().to_string())
            .collect();
        let declared = cmd.args.iter().find(|a| a.name == "key").unwrap().options.clone();
        assert_eq!(swept, declared);
    }

    #[test]
    fn typed_refusals_are_not_violations() {
        let report = run_command(&command("tv.power.off"), &Stub::new(Mode::Refuse));
        assert!(report.ok(), "a refusal must pass the property: {:?}", report.violations);
        assert!(report.typed_outcomes > 0);
    }

    #[test]
    fn a_panicking_sink_is_caught_as_a_violation() {
        let stub = Stub::failing_on(Mode::Panic, 2);
        let report = run_command(&command("tv.power.wake"), &stub);
        assert_eq!(report.violations.len(), 1, "'{:?}'", report.violations);
        assert_eq!(report.violations[0].kind, ViolationKind::Panic);
    }

    #[test]
    fn internal_errors_are_violations() {
        let stub = Stub::failing_on(Mode::Internal, 2);
        let report = run_command(&command("tv.power.wake"), &stub);
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.violations[0].kind, ViolationKind::InternalError);
    }

    #[test]
    fn idempotence_violation_is_reported_once_declared() {
        // `tv.volume.mute` declares no invariant, so a varying sink is invisible.
        let plain = run_command(&command("tv.volume.mute"), &Stub::new(Mode::NonIdempotent));
        assert!(plain.ok(), "{:?}", plain.violations);

        // Declare it and the same sink is caught.
        let mut cmd = command("tv.volume.mute");
        cmd.invariants = vec![Invariant::Idempotent];
        let report = run_command(&cmd, &Stub::new(Mode::NonIdempotent));
        assert!(!report.ok());
        assert!(report
            .violations
            .iter()
            .all(|v| v.kind == ViolationKind::Invariant && v.invariant == Some(Invariant::Idempotent)));
    }

    #[test]
    fn generation_is_deterministic() {
        let cmd = command("tv.volume.set");
        assert_eq!(cases_for(&cmd), cases_for(&cmd));
    }

    #[test]
    fn case_count_is_capped() {
        let cmd = command("tv.key.press");
        assert!(cases_for(&cmd).len() <= MAX_CASES);
    }
}
