# cratelab

**A local lab that turns published crates into testable, UI-generatable surfaces.**

cratelab is a Rust workspace + Tauri v2 shell built around one idea: your
workspaces (`tv-rs`, `cctv-rs`) already declare their command surface in
**typed Rust enums** with doc comments, capability sets and error taxonomies.
cratelab reads or generates a **manifest** of that surface, merges it with
hand-written metadata, and everything downstream is *derived*: the UI panels,
the command console, the scenario playlists, the coverage heatmap, the limit
fuzzer. The crates stay the backend and the facade; cratelab is the bench
they sit on.

```
                 ┌────────────────────────────────────────────┐
                 │              shell (Tauri v2)              │
                 │   manifest-driven UI · timeline · heatmap  │
                 └──────────────────────┬─────────────────────┘
                                        │  commands / events
                 ┌──────────────────────┴─────────────────────┐
   manifests ───▶│                lab-engine                  │
   (TOML+JSON)   │  discovery · matching · runner · ledger    │
                 └───┬──────────┬──────────┬──────────────────┘
                     │          │          │
              lab-mocks   lab-scenarios  lab-coverage
              virtual     playlists +    coverage heatmap
              fleet       Rhai steps     as data
                     │
              mock-first; hardware behind an explicit toggle
```

## Workspace layout

| Crate | Role |
|---|---|
| `lab-errors` | The one error vocabulary: `LabError`, `Remedy`, manifest-declared mapping rules per workspace. |
| `lab-manifest` | The manifest schema: commands, events, servers, limits, device classes, quirks. Validates and merges the generated + authored layers. |
| `lab-generate` | `syn`-based generator: typed `CommandId` enum (+ doc comments) → generated JSON layer + authoring TOML skeleton. |
| `lab-mocks` | Virtual fleet with honest capability sets, deterministic chaos, token-bucket limits, Rhai-scriptable behaviour. |
| `lab-scenarios` | TOML playlists (steps, expects, polls, timeouts) + sandboxed Rhai generative steps. Produces step-level reports. |
| `lab-coverage` | The coverage ledger: `(workspace, command, arg-bucket)` cells, heatmap rows, markdown/JSON reports. |
| `lab-engine` | Discovery (local checkout / crates.io sparse index behind a feature), matching + drift, the runner, and the headless `lab` CLI. |
| `shell/` | Tauri v2 app: manifest-driven panels, command console, timeline, coverage heatmap, scenario composer. Standalone workspace. |

## Quick start

```bash
# core (no GUI needed, fully offline)
cargo test --workspace
cargo run -p lab-engine --bin lab -- manifests manifests/
cargo run -p lab-engine --bin lab -- run scenarios/tv-morning-loop.toml \
        --manifest manifests/tv-rs.lab.toml --confirm
cargo run -p lab-engine --bin lab -- run scenarios/cctv-zone-watch.toml \
        --manifest manifests/cctv-rs.lab.toml --confirm

# the pre-release gate: every manifest + every scenario + coverage.
# Warn-only for now — it reports but never fails while the scenarios expand.
cargo run -p lab-engine --bin lab -- gate manifests scenarios

# generate the manifest layer from a typed command enum
cargo run -p lab-generate -- <path-to>/command.rs CommandId \
        --prefix tv --out manifests/generated/tv-rs.commands.json

# online discovery (crates.io sparse index; needs network)
cargo run -p lab-engine --features registry --bin lab -- discover ~/code/tv-rs

# the GUI
cd shell && npm install && npm run tauri dev
```

## The manifest in one screen

```toml
schema_version = 1
[workspace]
name = "tv-rs"
prefix = "tv"          # every id is namespaced: tv.power.off
facade = "tv-rs"
core = "tv-rs-core"
version = "0.6.0"

[errors]
source = "tv-rs-core::Error"
[[errors.mapping]]     # one vocabulary for thirteen crates
matches = "Capability*"
to = "capability"

[[device_classes]]
id = "tv"
mock_backend = "tv-rs-core mock backend"
capability_source = "DeviceHandle"
capabilities = ["power", "volume", "key", "launch"]

[[commands]]
id = "tv.power.off"
label = "Power off"
danger = "destructive"     # the UI asks; the runner refuses without --confirm
variant = "PowerOff"       # bridge to the generated layer
args = [{ name = "ip", type = "ipv4" }, { name = "vendor", type = "enum", options = ["samsung", "lg", "roku"] }]
timeout_ms = 8000
rate_limit = { per_second = 1, burst = 2 }
emits = ["tv.power.changed"]

[[events]]
id = "tv.power.changed"
source = "poll"            # poll | gena | websocket | mqtt | bus
payload = "PowerState"
confidence = "reported"    # mirrors power_observation()'s High / Inferred

[[servers]]
id = "tv.media.hls"
crate = "tv-rs-media"
default_port = 8123
health_path = "/health"

[[limits]]
id = "tv.ws.frames"
kind = "budget"            # budget | rate | pool | concurrency
applies_to = "tv-rs-transport"
```

Generated layer (from the enum) proposes ids/variants/docs; this authored
layer disposes labels/danger/limits/mock bindings. Matching is by id **or**
by `variant`, so pretty authored ids (`tv.power.off`) coexist with generated
ids (`tv.power_off`).

## Design rules (inherited from the workspaces)

* **Refusals are data.** Capability refusals are `LabError::Capability`,
  severity `Info`, exit code 0 — the lab counts them as *coverage*.
* **Nothing panics in library code.** The lab has no `unwrap()` on any path
  a manifest or script can influence.
* **Mock-first, hardware behind a toggle.** The same `CommandSink` port
  serves the mock fleet today and a real `TvClient` / cctv facade tomorrow;
  scenarios never know the difference.
* **Deterministic chaos.** Failure injection is counter-based (no RNG), so a
  failing run replays identically.
* **Scripts are sandboxed.** Rhai runs with bounded operations, bounded
  strings and no I/O.

## Status

* Core crates: compile + 34 unit tests green on stable Rust (edition 2021).
* `shell/` is a scaffold: UI code is complete enough to run, but the Tauri
  build needs the platform webview packages (webkit2gtk-4.1 on Linux) and is
  therefore kept out of the main workspace.
* Registry discovery is feature-gated (`--features registry`) to keep the
  default test profile offline.
