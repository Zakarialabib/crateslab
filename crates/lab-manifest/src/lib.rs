//! # lab-manifest — the schema that drives everything
//!
//! A **lab manifest** is the per-workspace declaration that turns a set of
//! published crates into a testable, UI-generatable surface. It is produced
//! in two layers and merged:
//!
//! 1. **Generated layer** — `lab-generate` reads the workspace's typed
//!    `CommandId` / event enums (doc comments included) and emits a JSON
//!    skeleton: stable ids, source variants, argument shapes.
//! 2. **Authoring layer** — a hand-written `*.lab.toml` per workspace adds
//!    what no compiler can know: labels, danger levels, timeouts, rate
//!    limits, mock script bindings, server health routes, limits, quirks.
//!
//! The merge rule is simple: the generated layer proposes, the authored
//! layer disposes. Any command present in both keeps the authored metadata
//! and gains the generated `variant` / `doc` provenance.

use lab_errors::{ErrorMapping, LabError, LabResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// Top-level manifest for one workspace (e.g. `tv-rs.lab.toml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabManifest {
    pub schema_version: u32,
    pub workspace: WorkspaceRef,
    #[serde(default)]
    pub errors: ErrorsSection,
    #[serde(default)]
    pub device_classes: Vec<DeviceClass>,
    #[serde(default)]
    pub commands: Vec<CommandSpec>,
    #[serde(default)]
    pub events: Vec<EventSpec>,
    #[serde(default)]
    pub servers: Vec<ServerSpec>,
    #[serde(default)]
    pub limits: Vec<LimitSpec>,
    #[serde(default)]
    pub quirks: Option<QuirksRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceRef {
    /// e.g. `"tv-rs"` — must match the crates.io/GitHub workspace name.
    pub name: String,
    /// Short id namespace used as the first segment of every command/event
    /// id, e.g. `"tv"` → `tv.power.off`.
    pub prefix: String,
    pub repo: String,
    /// The facade crate users depend on (`tv-rs`, `cctv-rs`).
    pub facade: String,
    /// The device-model crate that owns types + error taxonomy.
    pub core: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorsSection {
    /// Where the workspace's typed errors live, e.g. `"tv-rs-core::Error"`.
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub mapping: Vec<lab_errors::MappingRule>,
}

impl ErrorsSection {
    /// Build the runtime mapping table used to normalize workspace errors.
    pub fn to_mapping(&self) -> ErrorMapping {
        ErrorMapping {
            source: self.source.clone().unwrap_or_else(|| "unknown".into()),
            rules: self.mapping.clone(),
        }
    }
}

/// A family of devices the workspace can drive (`tv`, `camera`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceClass {
    pub id: String,
    /// Path of the scriptable mock backend, e.g. `"tv-rs-core::mock"`.
    pub mock_backend: String,
    /// Trait whose capability set drives refusals, e.g. `"DeviceHandle"`.
    pub capability_source: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// When the command mutates the world, how loudly must the UI ask first?
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Danger {
    #[default]
    Safe,
    Attention,
    Destructive,
}

/// What kind of surface this command is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandKind {
    /// Mutates device or fleet state.
    #[default]
    Action,
    /// Reads state; never mutates.
    Query,
    /// Changes lab-side configuration (limits, pools), not the device.
    Config,
}

/// The argument types the generated UI knows how to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgType {
    Str,
    Int,
    Float,
    Bool,
    Enum,
    Ipv4,
    Mac,
    Url,
    Path,
    Json,
}

/// One typed, UI-renderable argument.
///
/// The declared type is also the argument's *generator*: the conformance
/// suite reads these fields to produce in-range and deliberately hostile
/// inputs, so the zero-panic rule becomes executable rather than a promise.
/// A field with no bounds still generates — it just generates around a
/// type-wide envelope instead of a declared one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgSpec {
    pub name: String,
    pub r#type: ArgType,
    #[serde(default = "default_true")]
    pub required: bool,
    /// Only for `ArgType::Enum`.
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub help: Option<String>,
    /// Inclusive lower bound for `Int`/`Float`.
    #[serde(default)]
    pub min: Option<f64>,
    /// Inclusive upper bound for `Int`/`Float`.
    #[serde(default)]
    pub max: Option<f64>,
    /// Shortest accepted byte length for string-shaped types.
    #[serde(default)]
    pub min_len: Option<usize>,
    /// Longest accepted byte length for string-shaped types.
    #[serde(default)]
    pub max_len: Option<usize>,
}

fn default_true() -> bool {
    true
}

/// How much a workspace is willing to bet on this command.
///
/// The suite exercises every command regardless of tier — mock properties are
/// cheap and deterministic. The tier decides how loudly a violation is
/// reported, which is what the release gate keys on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConformanceTier {
    /// A violation blocks a release.
    Must,
    /// A violation is reported and reviewed, but does not block.
    #[default]
    Should,
    /// Exercised for information only.
    May,
}

/// A declared behaviour the conformance suite asserts on every run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Invariant {
    /// Calling twice with the same arguments yields the same result.
    Idempotent,
}

/// Token-bucket settings enforced by the runner *before* the call.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimit {
    pub per_second: f64,
    #[serde(default = "default_burst")]
    pub burst: u32,
}

fn default_burst() -> u32 {
    1
}

/// One callable command on the lab surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandSpec {
    /// Dotted, namespaced, stable: `tv.power.off`.
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub kind: CommandKind,
    #[serde(default)]
    pub danger: Danger,
    /// Where the variant lives, e.g. `"tv_rs_core::CommandId::PowerOff"`.
    /// Populated by the generator; authored manifests may leave it out.
    #[serde(default)]
    pub source_enum: Option<String>,
    /// The Rust variant name this authored command corresponds to
    /// (`"PowerOff"`). Optional bridge between authored ids (pretty, dotted)
    /// and generated ids (derived from the variant name).
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub args: Vec<ArgSpec>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub rate_limit: Option<RateLimit>,
    /// Event ids this command may emit (must exist in `events`).
    #[serde(default)]
    pub emits: Vec<String>,
    /// How much the workspace bets on this command. Drives how loudly the
    /// conformance suite and the release gate report a violation.
    #[serde(default)]
    pub conformance: Option<ConformanceTier>,
    /// Behaviours the conformance suite asserts on every run. Zero-panic is
    /// asserted for every command and does not need declaring.
    #[serde(default)]
    pub invariants: Vec<Invariant>,
    /// Mock behaviour script, relative to the mock scripts root.
    #[serde(default)]
    pub mock: Option<String>,
    #[serde(default)]
    pub help: Option<String>,
}

fn default_timeout() -> u64 {
    8_000
}

impl CommandSpec {
    /// Destructive commands must be confirmed in the UI and are refused by
    /// the headless runner unless `--yes` is passed.
    pub fn requires_confirm(&self) -> bool {
        self.danger == Danger::Destructive
    }

    /// The declared tier, or the neutral default when the author said nothing.
    pub fn tier(&self) -> ConformanceTier {
        self.conformance.unwrap_or_default()
    }

    /// Does this command declare the given invariant?
    pub fn has_invariant(&self, inv: Invariant) -> bool {
        self.invariants.contains(&inv)
    }
}

/// Where an event comes from — maps 1:1 to the workspaces' real sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    /// The agent polls; there is no push channel.
    Poll,
    /// UPnP GENA subscription (DLNA eventing).
    Gena,
    /// Vendor WebSocket push (Tizen / webOS).
    Websocket,
    /// MQTT subscription (agent feature).
    Mqtt,
    /// In-process event bus (mediamaster-style `EventBus`).
    Bus,
}

/// How the lab trusts an observed state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Read back from the device (`power_observation` → `High`).
    Reported,
    /// Probed / inferred (`power_observation` → `Inferred`).
    Inferred,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventSpec {
    pub id: String,
    pub source: EventSource,
    /// Type name of the payload, e.g. `"PowerState"`.
    pub payload: String,
    #[serde(default)]
    pub confidence: Option<Confidence>,
    #[serde(default)]
    pub help: Option<String>,
}

/// A server the workspace can start inside the lab (`tv-rs-media`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerSpec {
    pub id: String,
    /// Crate that owns the server (`crate` is a Rust keyword, hence the
    /// field name + serde rename).
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub default_port: u16,
    /// Health route used by the lab's readiness probe.
    #[serde(default)]
    pub health_path: Option<String>,
    #[serde(default)]
    pub routes: Vec<String>,
}

/// A declared limit the lab should exercise and fuzz against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitKind {
    /// Byte/frame budgets (budgeted frame readers).
    Budget,
    /// Token buckets / per-second ceilings.
    Rate,
    /// Connection pools with a bounded number of sockets.
    Pool,
    /// Max concurrent operations.
    Concurrency,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitSpec {
    pub id: String,
    pub kind: LimitKind,
    /// Crate or component the limit belongs to.
    pub applies_to: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuirksRef {
    pub file: String,
    /// Feature flag that activates quirks handling.
    pub feature: String,
}

// ---------------------------------------------------------------------------
// Generated layer
// ---------------------------------------------------------------------------

/// One command as emitted by `lab-generate` from the workspace's typed enum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedCommand {
    pub id: String,
    /// Rust variant, e.g. `"PowerOff"`.
    pub variant: String,
    #[serde(default)]
    pub doc: Option<String>,
    #[serde(default)]
    pub crate_path: Option<String>,
    #[serde(default)]
    pub args: Vec<ArgSpec>,
}

/// The JSON file `lab-generate` writes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedManifest {
    /// Source location, e.g. `"crates/core/src/command.rs::CommandId"`.
    pub source: String,
    pub generated_at: String,
    pub commands: Vec<GeneratedCommand>,
}

impl LabManifest {
    /// Parse the authoring layer (TOML).
    pub fn from_toml_str(s: &str) -> LabResult<Self> {
        let m: LabManifest =
            toml::from_str(s).map_err(|e| LabError::Manifest { path: "<toml>".into(), problem: e.to_string() })?;
        m.validate()?;
        Ok(m)
    }

    /// Merge the generated layer **into** this authored manifest.
    ///
    /// * Generated commands the authored manifest already has → fill in
    ///   `source_enum` (from `variant`) and `help` (from `doc`) when missing.
    /// * Generated commands the authored manifest lacks → appended as
    ///   `CommandKind::Action` skeletons so the UI still shows them (the lab
    ///   never hides a surface the workspace exposes).
    pub fn apply_generated(&mut self, gen: &GeneratedManifest) -> LabResult<()> {
        let prefix = format!("{}.", self.workspace.prefix);
        for gc in &gen.commands {
            if !gc.id.starts_with(&prefix) {
                return Err(LabError::Manifest {
                    path: gen.source.clone(),
                    problem: format!("generated id '{}' does not use prefix '{}'", gc.id, self.workspace.prefix),
                });
            }
            let existing = self
                .commands
                .iter()
                .position(|c| c.id == gc.id || c.variant.as_deref() == Some(gc.variant.as_str()));
            if let Some(pos) = existing {
                let cmd = &mut self.commands[pos];
                if cmd.source_enum.is_none() {
                    cmd.source_enum = Some(format!("{}::{}", gen.source, gc.variant));
                }
                if cmd.variant.is_none() {
                    cmd.variant = Some(gc.variant.clone());
                }
                if cmd.help.is_none() {
                    cmd.help = gc.doc.clone();
                }
            } else {
                self.commands.push(CommandSpec {
                    id: gc.id.clone(),
                    label: humanize(&gc.variant),
                    kind: CommandKind::Action,
                    danger: Danger::Attention,
                    source_enum: Some(format!("{}::{}", gen.source, gc.variant)),
                    variant: Some(gc.variant.clone()),
                    args: gc.args.clone(),
                    timeout_ms: 8_000,
                    rate_limit: None,
                    emits: Vec::new(),
                    conformance: None,
                    invariants: Vec::new(),
                    mock: None,
                    help: gc.doc.clone(),
                });
            }
        }
        self.validate()?;
        Ok(())
    }

    /// Structural validation. Returns all problems, not just the first.
    pub fn validate(&self) -> LabResult<()> {
        let mut problems: Vec<String> = Vec::new();

        if self.schema_version != SUPPORTED_SCHEMA_VERSION {
            problems.push(format!(
                "schema_version {} not supported (lab speaks {})",
                self.schema_version, SUPPORTED_SCHEMA_VERSION
            ));
        }

        let prefix = format!("{}.", self.workspace.prefix);
        let mut ids: BTreeSet<&str> = BTreeSet::new();
        for c in &self.commands {
            if !ids.insert(&c.id) {
                problems.push(format!("duplicate command id '{}'", c.id));
            }
            if !c.id.starts_with(&prefix) {
                problems.push(format!(
                    "command id '{}' must start with workspace prefix '{}.'",
                    c.id, self.workspace.prefix
                ));
            }
            for a in &c.args {
                if a.r#type == ArgType::Enum && a.options.is_empty() {
                    problems.push(format!("command '{}': enum arg '{}' has no options", c.id, a.name));
                }
                if let (Some(lo), Some(hi)) = (a.min, a.max) {
                    if lo > hi {
                        problems.push(format!(
                            "command '{}': arg '{}' has min {lo} above max {hi}",
                            c.id, a.name
                        ));
                    }
                }
                if let (Some(lo), Some(hi)) = (a.min_len, a.max_len) {
                    if lo > hi {
                        problems.push(format!(
                            "command '{}': arg '{}' has min_len {lo} above max_len {hi}",
                            c.id, a.name
                        ));
                    }
                }
                if a.r#type == ArgType::Enum && (a.min.is_some() || a.max.is_some()) {
                    problems.push(format!(
                        "command '{}': arg '{}' is an enum; bound it with 'options', not 'min'/'max'",
                        c.id, a.name
                    ));
                }
            }
        }

        let event_ids: BTreeSet<&str> = self.events.iter().map(|e| e.id.as_str()).collect();
        for c in &self.commands {
            for e in &c.emits {
                if !event_ids.contains(e.as_str()) {
                    problems.push(format!("command '{}' emits unknown event '{}'", c.id, e));
                }
            }
        }

        let class_ids: BTreeSet<&str> = self.device_classes.iter().map(|d| d.id.as_str()).collect();
        for e in &self.events {
            // events live in the same namespace; cheap sanity check only
            if !e.id.starts_with(&prefix) {
                problems.push(format!(
                    "event id '{}' must start with workspace prefix '{}.'",
                    e.id, self.workspace.prefix
                ));
            }
        }
        if !self.device_classes.is_empty() && class_ids.len() != self.device_classes.len() {
            problems.push("duplicate device class id".into());
        }

        if problems.is_empty() {
            Ok(())
        } else {
            Err(LabError::Manifest {
                path: format!("{} lab manifest", self.workspace.name),
                problem: problems.join("; "),
            })
        }
    }

    pub fn command(&self, id: &str) -> Option<&CommandSpec> {
        self.commands.iter().find(|c| c.id == id)
    }

    pub fn event(&self, id: &str) -> Option<&EventSpec> {
        self.events.iter().find(|e| e.id == id)
    }
}

fn humanize(variant: &str) -> String {
    // PowerOff -> "Power off"
    let words: Vec<String> = split_camel(variant);
    let mut out = String::new();
    for (i, w) in words.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&w.to_ascii_lowercase());
    }
    if out.is_empty() {
        variant.to_string()
    } else {
        let mut c = out.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => out,
        }
    }
}

fn split_camel(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch.is_uppercase() && !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
        cur.push(ch);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    const TV_TOML: &str = r#"
schema_version = 1

[workspace]
name = "tv-rs"
prefix = "tv"
repo = "https://github.com/Zakarialabib/tv-rs"
facade = "tv-rs"
core = "tv-rs-core"
version = "0.6.0"

[errors]
source = "tv-rs-core::Error"
[[errors.mapping]]
matches = "Capability*"
to = "capability"
retryable = false

[[device_classes]]
id = "tv"
mock_backend = "tv-rs-core mock backend"
capability_source = "DeviceHandle"
capabilities = ["power", "volume", "keys", "launch"]

[[commands]]
id = "tv.power.off"
label = "Power off"
kind = "action"
danger = "destructive"
variant = "PowerOff"
args = [{ name = "ip", type = "ipv4" }, { name = "vendor", type = "enum", options = ["samsung", "lg", "roku"] }]
timeout_ms = 8000
emits = ["tv.power.changed"]

[[events]]
id = "tv.power.changed"
source = "poll"
payload = "PowerState"
confidence = "reported"

[[servers]]
id = "tv.media.hls"
crate = "tv-rs-media"
default_port = 8123
health_path = "/health"

[[limits]]
id = "tv.ws.frames"
kind = "budget"
applies_to = "tv-rs-transport"
"#;

    #[test]
    fn parses_and_validates_tv_manifest() {
        let m = LabManifest::from_toml_str(TV_TOML).expect("valid manifest");
        assert_eq!(m.workspace.prefix, "tv");
        assert_eq!(m.commands.len(), 1);
        assert_eq!(m.commands[0].danger, Danger::Destructive);
        assert!(m.commands[0].requires_confirm());
        assert_eq!(m.commands[0].variant.as_deref(), Some("PowerOff"));
        assert_eq!(m.events[0].confidence, Some(Confidence::Reported));
        assert_eq!(m.errors.to_mapping().source, "tv-rs-core::Error");
    }

    #[test]
    fn rejects_duplicate_and_unprefixed_ids() {
        let bad = TV_TOML.replace("id = \"tv.power.off\"", "id = \"power.off\"");
        let err = LabManifest::from_toml_str(&bad).unwrap_err();
        assert!(err.to_string().contains("must start with workspace prefix"), "{err}");

        let dup = format!("{TV_TOML}\n[[commands]]\nid = \"tv.power.off\"\nlabel = \"again\"\n");
        let err = LabManifest::from_toml_str(&dup).unwrap_err();
        assert!(err.to_string().contains("duplicate command id"), "{err}");
    }

    #[test]
    fn rejects_unknown_emit() {
        let bad = TV_TOML.replace("emits = [\"tv.power.changed\"]", "emits = [\"tv.power.unknown\"]");
        let err = LabManifest::from_toml_str(&bad).unwrap_err();
        assert!(err.to_string().contains("unknown event"), "{err}");
    }

    #[test]
    fn generated_layer_merges_and_keeps_overrides() {
        let mut m = LabManifest::from_toml_str(TV_TOML).unwrap();
        let gen = GeneratedManifest {
            source: "crates/core/src/command.rs::CommandId".into(),
            generated_at: "2026-11-03T00:00:00Z".into(),
            commands: vec![
                GeneratedCommand {
                    id: "tv.power.off".into(),
                    variant: "PowerOff".into(),
                    doc: Some("Turn the panel off; WoL can bring it back.".into()),
                    crate_path: Some("tv-rs-core".into()),
                    args: vec![],
                },
                GeneratedCommand {
                    id: "tv.wake".into(),
                    variant: "Wake".into(),
                    doc: Some("Wake-on-LAN magic packet.".into()),
                    crate_path: Some("tv-rs-power".into()),
                    args: vec![ArgSpec {
                        name: "mac".into(),
                        r#type: ArgType::Mac,
                        required: true,
                        options: vec![],
                        default: None,
                        help: None,
                        min: None,
                        max: None,
                        min_len: None,
                        max_len: None,
                    }],
                },
            ],
        };
        m.apply_generated(&gen).unwrap();
        // existing command keeps authored label, gains source + help
        let off = m.command("tv.power.off").unwrap();
        assert_eq!(off.label, "Power off");
        assert_eq!(off.source_enum.as_deref(), Some("crates/core/src/command.rs::CommandId::PowerOff"));
        assert_eq!(off.help.as_deref(), Some("Turn the panel off; WoL can bring it back."));
        // new command becomes an attention-level skeleton
        let wake = m.command("tv.wake").unwrap();
        assert_eq!(wake.label, "Wake");
        assert_eq!(wake.danger, Danger::Attention);
        assert_eq!(wake.args[0].r#type, ArgType::Mac);
    }

    #[test]
    fn generated_layer_enforces_prefix() {
        let mut m = LabManifest::from_toml_str(TV_TOML).unwrap();
        let gen = GeneratedManifest {
            source: "other.rs::CommandId".into(),
            generated_at: "x".into(),
            commands: vec![GeneratedCommand {
                id: "cctv.ptz.stop".into(),
                variant: "PtzStop".into(),
                doc: None,
                crate_path: None,
                args: vec![],
            }],
        };
        let err = m.apply_generated(&gen).unwrap_err();
        assert!(err.to_string().contains("does not use prefix"), "{err}");
    }
}
