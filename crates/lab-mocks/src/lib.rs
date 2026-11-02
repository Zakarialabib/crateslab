//! # lab-mocks — a fleet that never sleeps (and never surprises you)
//!
//! The lab is **mock-first**: every command, event and limit is exercised
//! against virtual devices before any real LAN hardware is touched. The
//! design mirrors what the workspaces already ship:
//!
//! * `tv-rs-core` carries a scriptable mock backend + conformance suite —
//!   [`MockFleet`] is the lab-side fleet that drives it generically.
//! * Capability sets refuse unsupported commands **before** any I/O, so a
//!   refusal in the lab is exactly a refusal in production
//!   ([`LabError::Capability`], never a panic).
//! * Chaos is **deterministic** (counter-based, seed-free) so a failing run
//!   replays identically — the property a test bench needs.
//!
//! ## Behaviour scripts
//!
//! Per-command behaviour can be overridden with a Rhai function:
//!
//! ```rhai
//! // fn on_command(cmd_id, args_json) -> String (JSON)
//! fn on_command(cmd_id, args) {
//!     #{
//!         status: "ok",
//!         state: #{ power: "On" },
//!         events: [#{ id: "tv.power.changed", payload: "On" }],
//!         delay_ms: 40,
//!     }.to_json()
//! }
//! ```
//!
//! The JSON contract is `status` ∈ `ok | refuse | error | timeout`, plus
//! optional `state` (device state updates), `events` (bus emissions) and
//! `delay_ms` (simulated device latency).

use lab_errors::{LabError, LabResult};
use lab_manifest::{CommandSpec, LabManifest};
use rhai::{Engine, Scope};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Instant;

/// One virtual device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VirtualDevice {
    pub id: String,
    /// Device class id from the manifest (`tv`, `camera`).
    pub class: String,
    pub vendor: String,
    pub capabilities: Vec<String>,
    pub state: BTreeMap<String, Value>,
}

impl VirtualDevice {
    pub fn new(id: &str, class: &str, vendor: &str, capabilities: &[&str]) -> Self {
        Self {
            id: id.into(),
            class: class.into(),
            vendor: vendor.into(),
            capabilities: capabilities.iter().map(|s| s.to_string()).collect(),
            state: BTreeMap::new(),
        }
    }
}

/// Everything the sink needs to know about one call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Invocation {
    pub device: String,
    pub command_id: String,
    #[serde(default)]
    pub args: BTreeMap<String, Value>,
    #[serde(default)]
    pub scenario: Option<String>,
}

impl Invocation {
    /// Capability name by lab convention: the command id's *second* segment
    /// (`tv.power.off` → `power`).
    pub fn capability(&self) -> &str {
        capability_of(&self.command_id)
    }
}

/// Capability name by lab convention: the command id's *second* segment
/// (`tv.power.off` → `power`).
pub fn capability_of(command_id: &str) -> &str {
    command_id.split('.').nth(1).unwrap_or("core")
}

/// Events a command emitted, ready for the lab bus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmittedEvent {
    pub id: String,
    pub payload: Value,
}

/// What a sink call produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SinkOutcome {
    pub value: Value,
    pub events: Vec<EmittedEvent>,
    pub duration_ms: u64,
    /// State snapshot after the call (device-model observability).
    pub state: BTreeMap<String, Value>,
}

/// The lab's device-facing port. The real (hardware) adapter will implement
/// the same trait against `TvClient` / the cctv facade; mocks and hardware
/// are therefore interchangeable *inside every scenario*.
pub trait CommandSink: Send + Sync {
    fn execute(&self, inv: &Invocation) -> LabResult<SinkOutcome>;
    fn capabilities_of(&self, device: &str) -> Option<Vec<String>>;
    fn list_devices(&self) -> Vec<String>;

    /// Forget any rate-limit history.
    ///
    /// The conformance suite calls this between property cases so a spent
    /// token bucket cannot mask the command body it is trying to exercise —
    /// a rate-limit refusal would otherwise "pass" every hostile input
    /// without the input ever being handled. Sinks that do not limit
    /// lab-side keep the no-op default.
    fn reset_limits(&self) {}
}

/// Pick a device that actually exposes the command's capability.
///
/// A fleet merged from several manifests must not route `tv.power.off` at
/// whichever camera happens to come first in fleet order — that reports a
/// capability refusal the workspace never had. When nothing in the fleet can
/// serve the capability we still fall back to the first device, so the
/// refusal stays honest (a refusal is data).
pub fn pick_device_for(sink: &dyn CommandSink, command_id: &str) -> Option<String> {
    let capability = capability_of(command_id);
    let devices = sink.list_devices();
    devices
        .iter()
        .find(|d| {
            sink.capabilities_of(d)
                .map(|caps| caps.iter().any(|c| c == capability))
                .unwrap_or(false)
        })
        .cloned()
        .or_else(|| devices.first().cloned())
}

/// Deterministic failure injection — no RNG, reproducible runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ChaosPolicy {
    /// Drop every `nth` call (1-based counting; `Some(3)` = every third).
    #[serde(default)]
    pub fail_every_nth: Option<u32>,
    /// Extra latency added to every call (simulated device sluggishness).
    #[serde(default)]
    pub extra_latency_ms: u64,
    /// First `n` calls behave, then everything fails (`None` = never).
    #[serde(default)]
    pub poison_after: Option<u32>,
}

impl ChaosPolicy {
    fn inspect(&self, call_index: u64, duration_ms: &mut u64) -> Option<&'static str> {
        *duration_ms += self.extra_latency_ms;
        if let Some(n) = self.poison_after {
            if call_index > n as u64 {
                return Some("poisoned");
            }
        }
        if let Some(n) = self.fail_every_nth {
            if n > 0 && call_index.is_multiple_of(n as u64) {
                return Some("chaos");
            }
        }
        None
    }
}

/// Token bucket, one per command that declares a `rate_limit`.
#[derive(Debug, Clone)]
pub struct TokenBucket {
    per_second: f64,
    burst: f64,
    tokens: f64,
    last: Instant,
}

impl TokenBucket {
    pub fn new(per_second: f64, burst: u32) -> Self {
        Self { per_second, burst: burst as f64, tokens: burst as f64, last: Instant::now() }
    }

    /// Try to take one token; on refusal returns the retry delay in ms.
    pub fn acquire(&mut self) -> Result<(), u64> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.per_second).min(self.burst);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(())
        } else {
            let deficit = 1.0 - self.tokens;
            Err((deficit / self.per_second * 1000.0).ceil() as u64)
        }
    }
}

#[derive(Debug)]
struct Scripted {
    ast: rhai::AST,
}

/// serde_json::Value → rhai::Dynamic (maps stay maps, so scripts can do
/// `args.app_id`).
fn json_to_dynamic(v: &Value) -> rhai::Dynamic {
    match v {
        Value::Null => rhai::Dynamic::UNIT,
        Value::Bool(b) => (*b).into(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into()
            } else if let Some(f) = n.as_f64() {
                f.into()
            } else {
                rhai::Dynamic::UNIT
            }
        }
        Value::String(s) => s.clone().into(),
        Value::Array(items) => items
            .iter()
            .map(json_to_dynamic)
            .collect::<Vec<rhai::Dynamic>>()
            .into(),
        Value::Object(map) => {
            let mut m = rhai::Map::new();
            for (k, val) in map {
                m.insert(k.as_str().into(), json_to_dynamic(val));
            }
            m.into()
        }
    }
}

/// The mock fleet: virtual devices + spec table + optional Rhai overrides.
pub struct MockFleet {
    devices: Mutex<BTreeMap<String, VirtualDevice>>,
    /// Declaration order (manifest order) — `list_devices` preserves it.
    order: Vec<String>,
    specs: BTreeMap<String, CommandSpec>,
    buckets: Mutex<BTreeMap<String, TokenBucket>>,
    chaos: ChaosPolicy,
    scripts: BTreeMap<String, Scripted>,
    engine: Engine,
    calls: Mutex<u64>,
    clock_ms: Mutex<u64>,
}

impl MockFleet {
    /// Build a fleet from a manifest: every device class gets one virtual
    /// device per declared vendor-less "sample"; specs come from commands.
    pub fn from_manifest(manifest: &LabManifest) -> Self {
        Self::from_manifests(std::slice::from_ref(manifest))
    }

    /// Build one fleet from several workspaces' manifests: every device
    /// class becomes a device and every command a spec, so a single lab can
    /// drive `tv-rs` and `cctv-rs` side by side.
    pub fn from_manifests(manifests: &[LabManifest]) -> Self {
        let mut devices = BTreeMap::new();
        let mut order = Vec::new();
        let mut specs = BTreeMap::new();
        for manifest in manifests {
            for (i, class) in manifest.device_classes.iter().enumerate() {
                let id = format!("{}-{:02}", class.id, i + 1);
                let mut dev = VirtualDevice::new(&id, &class.id, "virtual", &[]);
                dev.capabilities = class.capabilities.clone();
                dev.state.insert("power".into(), Value::String("Standby".into()));
                devices.insert(id.clone(), dev);
                order.push(id);
            }
            for c in &manifest.commands {
                specs.insert(c.id.clone(), c.clone());
            }
        }
        let mut fleet = Self::with_parts(devices, specs);
        fleet.order = order;
        fleet
    }

    pub fn with_parts(
        devices: BTreeMap<String, VirtualDevice>,
        specs: BTreeMap<String, CommandSpec>,
    ) -> Self {
        let mut engine = Engine::new();
        // Hard sandbox: scripts are data, not programs we trust.
        engine.set_max_operations(50_000);
        engine.set_max_string_size(1_000_000);
        engine.set_max_expr_depths(64, 64);
        Self {
            devices: Mutex::new(devices),
            order: Vec::new(),
            specs,
            buckets: Mutex::new(BTreeMap::new()),
            chaos: ChaosPolicy::default(),
            scripts: BTreeMap::new(),
            engine,
            calls: Mutex::new(0),
            clock_ms: Mutex::new(0),
        }
    }

    /// An empty fleet — the shell builds one and loads manifests into it.
    pub fn new_empty() -> Self {
        Self::with_parts(BTreeMap::new(), BTreeMap::new())
    }

    pub fn set_chaos(&mut self, chaos: ChaosPolicy) {
        self.chaos = chaos;
    }

    pub fn set_device_state(&mut self, device: &str, key: &str, value: Value) -> LabResult<()> {
        let mut map = self.devices.lock().expect("devices lock");
        let dev = map.get_mut(device).ok_or_else(|| LabError::Discovery {
            phase: "lookup".into(),
            message: format!("unknown device '{device}'"),
        })?;
        dev.state.insert(key.into(), value);
        Ok(())
    }

    /// Register (or replace) the behaviour script for one command id.
    pub fn with_script(mut self, command_id: &str, source: &str) -> LabResult<Self> {
        let ast = self.engine.compile(source).map_err(|e| LabError::Script {
            script: command_id.into(),
            message: e.to_string(),
        })?;
        self.scripts.insert(command_id.into(), Scripted { ast });
        Ok(self)
    }

    fn next_call(&self) -> u64 {
        let mut c = self.calls.lock().expect("calls lock");
        *c += 1;
        *c
    }

    fn tick(&self, add_ms: u64) -> u64 {
        let mut t = self.clock_ms.lock().expect("clock lock");
        *t += add_ms.max(1);
        *t
    }

    fn default_behaviour(&self, inv: &Invocation, dev: &mut VirtualDevice) -> SinkOutcome {
        // Small built-in responder so the lab demos without any script:
        // verb heuristics over the command id's last segment.
        let verb = inv.command_id.rsplit('.').next().unwrap_or("call");
        match verb {
            "on" | "wake" => {
                dev.state.insert("power".into(), Value::String("On".into()));
            }
            "off" | "standby" | "power_off" => {
                dev.state.insert("power".into(), Value::String("Standby".into()));
            }
            "state" | "status" | "info" => {}
            _ => {}
        }
        if let Some(v) = inv.args.get("volume").and_then(|v| v.as_i64()) {
            dev.state.insert("volume".into(), Value::from(v));
        }
        SinkOutcome {
            value: serde_json::json!({
                "device": dev.id,
                "command": inv.command_id,
                "echo": inv.args,
            }),
            events: Vec::new(),
            duration_ms: 0,
            state: dev.state.clone(),
        }
    }

    fn run_script(
        &self,
        inv: &Invocation,
        scripted: &Scripted,
        dev: &mut VirtualDevice,
    ) -> LabResult<SinkOutcome> {
        // Args arrive as a real Rhai map, so scripts write `args.app_id`.
        let args_value: Value = Value::Object(inv.args.clone().into_iter().collect());
        let args_dyn = json_to_dynamic(&args_value);
        let mut scope = Scope::new();
        let out: String = self
            .engine
            .call_fn(&mut scope, &scripted.ast, "on_command", (inv.command_id.clone(), args_dyn))
            .map_err(|e| LabError::Script { script: inv.command_id.clone(), message: e.to_string() })?;

        #[derive(Deserialize)]
        struct ScriptReply {
            status: String,
            #[serde(default)]
            state: BTreeMap<String, Value>,
            #[serde(default)]
            events: Vec<EmittedEvent>,
            #[serde(default)]
            delay_ms: u64,
            #[serde(default)]
            message: Option<String>,
            #[serde(default)]
            value: Option<Value>,
        }
        let reply: ScriptReply = serde_json::from_str(&out).map_err(|e| LabError::Script {
            script: inv.command_id.clone(),
            message: format!("reply is not lab-JSON: {e}"),
        })?;
        for (k, v) in reply.state {
            dev.state.insert(k, v);
        }
        let value = reply.value.unwrap_or_else(|| {
            serde_json::json!({ "device": dev.id, "command": inv.command_id })
        });
        match reply.status.as_str() {
            "ok" => Ok(SinkOutcome {
                value,
                events: reply.events,
                duration_ms: reply.delay_ms,
                state: dev.state.clone(),
            }),
            "refuse" => Err(LabError::Capability {
                device: dev.id.clone(),
                command: reply.message.unwrap_or_else(|| inv.command_id.clone()),
            }),
            "timeout" => Err(LabError::Timeout {
                operation: inv.command_id.clone(),
                budget_ms: reply.delay_ms,
            }),
            _ => Err(LabError::Transport {
                target: dev.id.clone(),
                kind: "script_error".into(),
                message: reply.message.unwrap_or_else(|| "mock failure".into()),
                retryable: true,
            }),
        }
    }
}

impl CommandSink for MockFleet {
    fn execute(&self, inv: &Invocation) -> LabResult<SinkOutcome> {
        let call_index = self.next_call();

        // 1. Device lookup (discovery-level failure).
        let mut dev = self
            .devices
            .lock()
            .expect("devices lock")
            .get(&inv.device)
            .cloned()
            .ok_or_else(|| LabError::Discovery {
                phase: "fleet".into(),
                message: format!("device '{}' is not in the fleet", inv.device),
            })?;

        // 2. Spec lookup (manifest-level failure).
        let spec = self
            .specs
            .get(&inv.command_id)
            .ok_or_else(|| LabError::Manifest {
                path: format!("command '{}'", inv.command_id),
                problem: "not declared in the manifest".into(),
            })?
            .clone();

        // 3. Capability refusal BEFORE any I/O — the workspaces' own rule.
        let cap = inv.capability().to_string();
        if !dev.capabilities.is_empty() && !dev.capabilities.contains(&cap) {
            return Err(LabError::Capability { device: dev.id.clone(), command: inv.command_id.clone() });
        }

        // 4. Rate limit from the manifest, enforced lab-side.
        if let Some(rl) = spec.rate_limit {
            let mut buckets = self.buckets.lock().expect("buckets lock");
            let bucket = buckets
                .entry(inv.command_id.clone())
                .or_insert_with(|| TokenBucket::new(rl.per_second, rl.burst));
            if let Err(after) = bucket.acquire() {
                return Err(LabError::RateLimited {
                    operation: inv.command_id.clone(),
                    retry_after_ms: after.max(1),
                });
            }
        }

        // 5. Deterministic chaos.
        let mut duration = 0u64;
        if let Some(why) = self.chaos.inspect(call_index, &mut duration) {
            return Err(LabError::Transport {
                target: dev.id.clone(),
                kind: why.into(),
                message: format!("chaos policy rejected call #{call_index}"),
                retryable: true,
            });
        }

        // 6. Behaviour: script override or built-in responder.
        let mut outcome = match self.scripts.get(&inv.command_id) {
            Some(s) => self.run_script(inv, s, &mut dev)?,
            None => self.default_behaviour(inv, &mut dev),
        };
        outcome.duration_ms += duration;
        self.tick(outcome.duration_ms);

        // 7. Persist state back.
        if let Some(d) = self.devices.lock().expect("devices lock").get_mut(&inv.device) {
            d.state = outcome.state.clone();
        }
        Ok(outcome)
    }

    fn capabilities_of(&self, device: &str) -> Option<Vec<String>> {
        self.devices
            .lock()
            .expect("devices lock")
            .get(device)
            .map(|d| d.capabilities.clone())
    }

    fn list_devices(&self) -> Vec<String> {
        self.order.clone()
    }

    fn reset_limits(&self) {
        self.buckets.lock().expect("buckets lock").clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lab_manifest::LabManifest;

    const MANIFEST: &str = include_str!("../../../manifests/tv-rs.lab.toml");
    const CCTV: &str = include_str!("../../../manifests/cctv-rs.lab.toml");

    fn fleet() -> MockFleet {
        let m = LabManifest::from_toml_str(MANIFEST).expect("manifest parses");
        MockFleet::from_manifest(&m)
    }

    fn inv(cmd: &str) -> Invocation {
        Invocation {
            device: "tv-01".into(),
            command_id: cmd.into(),
            args: BTreeMap::new(),
            scenario: None,
        }
    }

    #[test]
    fn wake_turns_device_on_via_default_behaviour() {
        let f = fleet();
        let out = f.execute(&inv("tv.power.wake")).expect("wake works");
        assert_eq!(out.state.get("power"), Some(&Value::String("On".into())));
    }

    #[test]
    fn capability_refusal_precedes_everything() {
        let f = fleet();
        // 'tv.panel.orientation' needs the `panel` capability which the
        // declared device class does not carry.
        let err = f.execute(&inv("tv.panel.orientation")).unwrap_err();
        assert!(matches!(err, LabError::Capability { .. }), "{err}");
    }

    #[test]
    fn unknown_command_is_manifest_error() {
        let f = fleet();
        let err = f.execute(&inv("tv.does.not_exist")).unwrap_err();
        assert!(matches!(err, LabError::Manifest { .. }), "{err}");
    }

    #[test]
    fn unknown_device_is_discovery_error() {
        let f = fleet();
        let mut i = inv("tv.power.wake");
        i.device = "tv-ghost".into();
        let err = f.execute(&i).unwrap_err();
        assert!(matches!(err, LabError::Discovery { .. }), "{err}");
    }

    #[test]
    fn chaos_is_deterministic() {
        let mut f = fleet();
        f.set_chaos(ChaosPolicy { fail_every_nth: Some(2), ..Default::default() });
        let r1 = f.execute(&inv("tv.power.wake"));
        let r2 = f.execute(&inv("tv.power.wake"));
        assert!(r1.is_ok());
        assert!(matches!(r2.unwrap_err(), LabError::Transport { kind, .. } if kind == "chaos"));
    }

    #[test]
    fn rate_limit_refuses_within_window() {
        let mut f = fleet();
        // tv.power.state declares rate_limit in the sample manifest; if the
        // sample changes, this test still works by building a spec inline.
        if f.specs.get("tv.power.state").map(|s| s.rate_limit.is_some()) != Some(true) {
            let spec = CommandSpec {
                id: "tv.power.state".into(),
                label: "State".into(),
                kind: Default::default(),
                danger: Default::default(),
                source_enum: None,
                variant: None,
                args: vec![],
                timeout_ms: 1000,
                rate_limit: Some(lab_manifest::RateLimit { per_second: 0.001, burst: 1 }),
                emits: vec![],
                conformance: None,
                invariants: vec![],
                mock: None,
                help: None,
            };
            f.specs.insert("tv.power.state".into(), spec);
        }
        let r1 = f.execute(&inv("tv.power.state"));
        let r2 = f.execute(&inv("tv.power.state"));
        assert!(r1.is_ok());
        assert!(matches!(r2.unwrap_err(), LabError::RateLimited { .. }));
    }

    #[test]
    fn from_manifests_merges_classes_and_specs() {
        let tv = LabManifest::from_toml_str(MANIFEST).unwrap();
        let cctv = LabManifest::from_toml_str(CCTV).unwrap();
        let f = MockFleet::from_manifests(&[tv, cctv]);
        let ids = f.list_devices();
        assert!(ids.iter().any(|d| d.starts_with("tv-")), "{ids:?}");
        assert!(ids.iter().any(|d| d.starts_with("camera-")), "{ids:?}");
        // one fleet serves a command from each workspace
        assert!(f.execute(&inv("tv.power.wake")).is_ok());
        let camera = ids.iter().find(|d| d.starts_with("camera-")).unwrap().clone();
        let mut c = inv("cctv.stream.snapshot");
        c.device = camera;
        assert!(f.execute(&c).is_ok());
    }

    #[test]
    fn rhai_script_drives_state_and_events() {
        let f = fleet()
            .with_script(
                "tv.launch.app",
                r#"
                fn on_command(cmd_id, args) {
                    #{
                        status: "ok",
                        state: #{ foreground: args.app_id },
                        events: [#{ id: "tv.launch.started", payload: args.app_id }],
                        delay_ms: 25,
                    }.to_json()
                }
                "#,
            )
            .expect("script compiles");
        let mut i = inv("tv.launch.app");
        i.args.insert("app_id".into(), Value::String("netflix".into()));
        let out = f.execute(&i).expect("scripted command works");
        assert_eq!(out.state.get("foreground"), Some(&Value::String("netflix".into())));
        assert_eq!(out.events.len(), 1);
        assert_eq!(out.events[0].id, "tv.launch.started");
    }

    #[test]
    fn rhai_refusal_maps_to_capability() {
        let f = fleet()
            .with_script(
                "tv.volume.mute",
                r#"fn on_command(cmd, args) { #{ status: "refuse", message: "mute locked by quirk" }.to_json() }"#,
            )
            .expect("script compiles");
        let err = f.execute(&inv("tv.volume.mute")).unwrap_err();
        assert!(matches!(err, LabError::Capability { .. }), "{err}");
    }

    #[test]
    fn bad_script_reply_is_script_error() {
        let f = fleet()
            .with_script("tv.key.press", r#"fn on_command(c, a) { "nonsense" }"#)
            .expect("compiles");
        let err = f.execute(&inv("tv.key.press")).unwrap_err();
        assert!(matches!(err, LabError::Script { .. }), "{err}");
    }
}
