# cratelab shell (Tauri v2)

Manifest-driven desktop UI over `lab-engine`. The panels, forms, timelines
and heatmap are **derived from the manifests** — adding a command to
`tv-rs.lab.toml` makes it appear here with a generated form, no frontend
change.

Part of the **cratelab** project. Full documentation starts at
[`../docs/00-index.md`](../docs/00-index.md); the shell's architecture, IPC
surface and view inventory are in
[`../docs/04-shell-and-ui.md`](../docs/04-shell-and-ui.md).

## Stack

* **Tauri v2** desktop shell over the cratelab `lab-*` core crates.
* **Svelte 5 (runes)** + TypeScript UI (`$state`, `$props`, `$effect`, `$derived`).
* **Vite 8** dev server / bundler.
* **Bun** as the package manager and runtime.

## Prerequisites

* Rust stable + the platform webview stack:
  * Linux: `libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`
  * macOS: Xcode CLT
  * Windows: WebView2 (preinstalled on Win11) + MSVC
* [Bun](https://bun.sh)

## Run

```bash
cd shell
bun install
bun run tauri dev
```

The window loads both sample manifests from `../../manifests/` into a
persistent `Lab` (mock fleet). Every run books coverage — switch to the
**Coverage** view after playing with the command console to watch the heatmap
move.

## Layout

```
src/                       Svelte 5 (runes) + TypeScript UI
  App.svelte               view router / shell chrome
  api.ts                   typed wrappers over the Tauri IPC commands
  types.ts                 shared types mirroring the Rust payloads
  forge-workflows.ts       forge workflow templates
  bonsai-models.ts         Bonsai model ↔ free-hardware mapping
  components/
    ArgWidget.svelte       arg type → widget table (the generative-UI core)
    CommandForm.svelte     generated form + confirm flow for destructive commands
    Timeline.svelte        scenario report timeline
    CoverageHeatmap.svelte coverage heatmap
    *View.svelte           dashboard · commands · coverage · scenarios ·
                           discovery · forge · reports · automation
src-tauri/                 Rust side: one persistent Lab in managed state
  src/lib.rs               commands: workspaces · manifest_detail · run_command ·
                           run_scenario · coverage · discover_local · set_backend
  src/forge_knowledge.rs   forge knowledge / RAG IPC surface
  src/forge_circleci.rs    CircleCI IPC surface
  tests/ipc_engine.rs      headless integration test over the real build_lab wiring
```

The hardware toggle (`set_backend`) intentionally answers with a typed
`capability` refusal until the real `CommandSink` adapter over `TvClient` /
the cctv facade lands (v0.2). The device backend is `mock` in the current
build; provider-backed forge analysis is a separate, explicitly confirmed
switch — see [`../docs/12-operations.md`](../docs/12-operations.md).
