# 01 · Architecture

The whole-system view: the two planes of cratelab, the manifest-driven spine they share, the layering of the bench and forge crates, the design invariants that hold across every component, and where the forge attaches.

> **cratelab docs** · SPDX `MIT` · security: this file must never contain credentials, tokens, private-repo content, or absolute user paths.

**Status:** reference · **Last verified:** 2026-11-04 · **Related:** [00-index](00-index.md), [02-manifest](02-manifest.md), [03-cli-and-workflows](03-cli-and-workflows.md), [04-shell-and-ui](04-shell-and-ui.md), [06-crate-intelligence](06-crate-intelligence.md), [07-the-forge](07-the-forge.md)

## 1. Two planes, one premise

cratelab is one repository serving two distinct planes. Keeping them separate is the organising principle of the whole codebase.

```mermaid
flowchart TB
  subgraph BENCH["The crate bench — local, offline, deterministic"]
    M["manifests\n(generated + authored)"]
    E["lab-engine\nrunner · ledger · CLI"]
    S["shell\nTauri v2 + Svelte 5"]
    M --> E
    E --> S
    S -->|"commands / scenarios"| E
  end

  subgraph TARGET["Target workspaces — the crates under test"]
    TV["tv-rs"]
    CC["cctv-rs"]
  end

  subgraph FORGE["The forge — the agentic evolution engine"]
    REG["one registry\nforge.workflows.toml"]
    CR["forge-core · forge-rag · forge-sidecar"]
    HOME["runtime home\nforge/ (state · reports · data)"]
    CR --> REG
    CR --> HOME
  end

  TV -. "typed enums +\ndoc comments" .-> M
  CC -. "typed enums +\ndoc comments" .-> M
  BENCH == "evidence: manifests · scenarios · coverage" ==> FORGE
  FORGE == "proposals: study · retrieval · features · fixes · docs\n(human-reviewed evolution artifacts)" ==> TARGET
```

- **The crate bench** (`crates/lab-*`, `shell/`) is cratelab's own implementation: it loads manifests, drives a mock *device* fleet (`MockFleet`), runs scenarios, records coverage, and executes property conformance. It is offline and deterministic by default.
- **The forge** (`crates/forge-*`, runtime home `forge/`, the shell's forge views) is the agentic engine that replaced the scattered `fleet/` control plane. It studies sources, builds a knowledge base, retrieves grounded evidence, renders scripted prompts and hands tools to an agent, then emits a reviewable *evolution artifact* for a *target* crate. It never writes features unattended and never merges.

The arrow between them is one-directional in authority: the bench **produces evidence**, the forge **produces proposals**, and a human disposes. See [07-the-forge](07-the-forge.md) and [06-crate-intelligence](06-crate-intelligence.md).

## 2. The manifest spine

Every capability in the system descends from one artefact: the **manifest**. It is not a map of an API — it is a behavioural contract. The full schema lives in [02-manifest](02-manifest.md); the architectural rule is:

> **Generated proposes, authored disposes.**

```mermaid
flowchart LR
  SRC["Typed `CommandId` enum\n+ doc comments\n(target crate source)"] --> GEN["lab-generate\n(syn parse)"]
  GEN --> GL["Generated layer\nJSON: ids · variants · docs"]
  HUMAN["Human author"] --> AL["Authored layer\nTOML: labels · danger · limits · mocks"]
  GL --> MERGE["lab-manifest\nmerge + validate"]
  AL --> MERGE
  MERGE --> OUT["One merged manifest"]
  OUT --> UI["UI panels\n(derived, zero frontend code)"]
  OUT --> RUN["Scenario runner"]
  OUT --> COV["Coverage ledger"]
  OUT --> CONF["Conformance suite"]
```

- The **generated layer** is machine-produced from the crate's typed enum. It can be regenerated at any time.
- The **authored layer** is hand-written: labels, danger levels, limits, mock bindings, `why` rationale.
- The two are matched by `id` **or** by `variant`, so pretty authored ids (`tv.power.off`) coexist with generated ids (`tv.power_off`).
- `lab-manifest` validates and merges them, reporting **all** problems rather than the first.

This single artefact is the reason adding a crate adds *zero* frontend code: the UI is a renderer over the merged manifest, not an author of it.

## 3. Bench and engine layering

The bench is eight `lab-*` crates with a strict, mostly-acyclic dependency direction; the forge adds three `forge-*` crates on top of the same `lab-errors` root. The verified graphs and per-crate responsibilities are in [06-crate-intelligence](06-crate-intelligence.md); the architectural summary:

```mermaid
flowchart TD
  errors["lab-errors\none error vocabulary"]
  manifest["lab-manifest\nschema · validate · merge"]
  mocks["lab-mocks\nCommandSink · MockFleet"]
  coverage["lab-coverage\ncoverage ledger"]
  scenarios["lab-scenarios\nTOML + Rhai runner"]
  conformance["lab-conformance\nproperty case generator"]
  generate["lab-generate\nsyn: enum → generated layer"]
  engine["lab-engine\ndiscovery · matching · Lab · CLI"]

  errors --> manifest
  errors --> mocks
  errors --> coverage
  errors --> scenarios
  errors --> conformance
  errors --> generate
  errors --> engine
  manifest --> mocks
  manifest --> coverage
  manifest --> scenarios
  manifest --> conformance
  manifest --> generate
  manifest --> engine
  mocks --> scenarios
  mocks --> conformance
  mocks --> engine
  coverage --> scenarios
  coverage --> engine
  scenarios --> engine
  conformance --> engine
```

`lab-errors` is the root: every other crate depends on it and nothing depends on the crates above it. `lab-generate` is an **authoring tool**, not a runtime dependency — it has no dependents inside the workspace and sits off to the side of the runtime path.

The forge crates depend on the *same* `lab-errors` vocabulary, so a forge refusal and a lab refusal classify identically (see §5). The direction is strictly downward:

```mermaid
flowchart TD
  errors["lab-errors\none error vocabulary"]
  forge_core["forge-core\nSDK: workflow registry · pipeline · stages/gates · artifacts · provider ports · ForgeError"]
  forge_rag["forge-rag\ningest/chunk · BM25 index (k1=1.2, b=0.75) · evidence packets · prompt templates"]
  forge_sidecar["forge-sidecar\nstdio JSON-RPC binary (forge.ping / forge.workflows / forge.tokenize) + declared remotes"]

  errors --> forge_core
  errors --> forge_rag
  errors --> forge_sidecar
  forge_core --> forge_rag
  forge_core --> forge_sidecar
  forge_rag --> forge_sidecar
```

`forge-sidecar` is the single transport: the Tauri shell, a CLI and an editor all call the *same* workflows, retrieval plane and pipeline over the sidecar instead of each re-implementing them. The read-only engine assets live inside the crates (`crates/forge-core/forge.workflows.toml`, `crates/forge-rag/prompts/`, `crates/forge-sidecar/remotes/`); the mutable runtime home is the gitignored `forge/` (`.env`, `state/`, `reports/`, `data/`). See [08-forge-sdk-and-sidecar](08-forge-sdk-and-sidecar.md).

## 4. Runtime data flow

```mermaid
sequenceDiagram
  participant U as User (shell or CLI)
  participant E as lab-engine
  participant F as lab-mocks (MockFleet)
  participant C as lab-coverage (Ledger)

  U->>E: run scenario / invoke command
  E->>E: load + merge manifest, pick device by capability
  E->>F: execute(command, args)
  F-->>E: Outcome (Ok | Refused | Failed | TimedOut | RateLimited)
  E->>C: book cell (workspace, command, arg-bucket)
  E-->>U: step report + coverage heatmap
```

The outcome set is the crux: a **capability refusal is a first-class outcome** (severity `Info`, exit code 0) and counts as coverage, never as a red error. The `CommandSink` port that `lab-mocks` implements today is the same port a real hardware facade implements later — scenarios never know the difference.

## 5. Design invariants

These hold across every crate, the shell, the bench and the forge. They are the contract any new component must respect.

| Invariant | Meaning |
|---|---|
| **Refusals are data** | A capability refusal is a typed outcome (`LabError::Capability`), severity `Info`, exit 0 — counted, not crashed on. The forge preserves this: a stage that declines to act yields a `Refusal` with `Severity::Info` and still exits `0`, so a declined run is never a panic and never a generic 500. |
| **Zero panic** | No `unwrap()` / `panic!` on any path a manifest or script can influence. Conformance makes this executable. |
| **Mock-first** | The same `CommandSink` port serves the mock fleet and a real facade; hardware lands behind an explicit toggle. |
| **Deterministic chaos** | Failure injection is counter-based, never RNG, so a failing run replays identically. |
| **Sandboxed scripts** | Rhai runs with bounded operations, bounded strings and no I/O. |
| **One error vocabulary** | Everything branches on `LabError` + `Remedy`, nothing else. |
| **The UI is a renderer** | Adding a crate adds zero frontend code; the UI is derived from the manifest. |
| **Generated proposes, authored disposes** | Machine output is advisory; a human commits the semantic layer. |
| **One workflow registry** | The forge has one authored `forge.workflows.toml`, embedded at build time; the UI renders it, the engine executes it and the sidecar serves it. Nothing re-implements a workflow in a frontend. |
| **One ordered spine** | Every forge workflow walks the same seven stages in order — study → knowledge → retrieve → prompt → tools → pipeline → artifact — validated against `StageKind::order` at load. |

## 6. The shell in the system

`shell/` is a Tauri v2 desktop application built on **Svelte 5 runes** with **Bun** as the package manager. It is a **standalone Cargo workspace**, deliberately *not* a member of the root workspace, so that `cargo test --workspace` stays green on machines without webkit2gtk / Tauri system dependencies.

The shell holds the lab behind a single `LabState { lab: Mutex<Lab> }` and exposes a typed IPC surface (commands, scenarios, coverage, forge settings) to the Svelte frontend. Secret values never leave Rust — the UI only ever sees a boolean `configured`. See [04-shell-and-ui](04-shell-and-ui.md).

## 7. Where the forge attaches

The forge is a **proposer** layered over the bench's evidence, and the renamed successor to the scattered `fleet/` control plane. It has one governance boundary, enforced by the execution flags in the crate-hosted `crates/forge-core/forge.config.json`:

- `execution.github_writes: false` and `execution.target_writes: false` — the forge cannot push to GitHub or write to the target repos.
- `forbidden_paths` — the paths it may never modify (`Cargo.toml`, `Cargo.lock`, `.github/**`, `.circleci/**`).
- `execution.provider_calls` / `network` are `true`, but every provider call requires an explicit preview and confirmation, gates on a clean evidence packet, and fails over only along an explicit route — never silently. Credentials live in the ignored runtime home `forge/.env` and are referenced by name only.

The forge's one source of truth is the authored `crates/forge-core/forge.workflows.toml` (embedded at build time). It carries four workflows across the `intern` / `extern` / `ops` / `rag` domains; every one walks the same seven-stage spine and the three gates **research → propose → validate**:

```mermaid
flowchart LR
  S1["1 study\nsources"] --> S2["2 knowledge\nbuild the base"]
  S2 --> S3["3 retrieve\nevidence packet"]
  S3 --> S4["4 prompt\nscripted prompt"]
  S4 --> S5["5 tools\narm the agent"]
  S5 --> S6["6 pipeline\ngated steps"]
  S6 --> S7["7 artifact\nfeature · fix · doc"]
  G1{{"gate: research"}} -.-> S1
  G2{{"gate: propose"}} -.-> S5
  G3{{"gate: validate"}} -.-> S7
```

A gate produces a `RuleVerdict`; a decline is a `Refusal` carried as data (`Severity::Info`, exit `0`), not an exception. The engine uses no RNG and performs no network I/O — providers sit behind a `ProviderPort` that is attached explicitly, so the same workflow against the same rules replays identically.

The bench supplies the *facts* (manifests, scenarios, coverage); the forge supplies *evidence-grounded proposals*. [07-the-forge](07-the-forge.md) covers the domains and the factory model; [09-the-workflow-model](09-the-workflow-model.md) covers the stages and gates; [12-operations](12-operations.md) covers running it safely.

## 8. Consolidation notes

- Merged the architecture material that was previously split across the blueprint PDF, the stale `architecture.html` narrative, the playbook and the technical-implementation build sheet into one verified description.
- Kept the two-plane split explicit — an earlier draft documented the bench and the forge as if they were one system.
- **Reframed the second plane from "the fleet" to "the forge"** and added the three `crates/forge-*` crates (§3), the one-registry invariant and the seven-stage spine (§5, §7); the old `fleet/` paths and filenames are gone.
- The crate dependency graph is the corrected version from [06-crate-intelligence](06-crate-intelligence.md) (the pre-correction graph omitted three `lab-errors` edges and mis-stated `lab-generate`'s runtime position).
- Design invariants were collected from the root README and the concept plan; the wording now matches the code's actual behaviour (e.g. refusals as `Info` / exit 0).
