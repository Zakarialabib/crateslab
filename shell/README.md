# cratelab shell (Tauri v2)

Manifest-driven desktop UI over `lab-engine`. The panels, forms, timelines
and heatmap are **derived from the manifests** — adding a command to
`tv-rs.lab.toml` makes it appear here with a generated form, no frontend
change.

## Prerequisites

* Rust stable + the platform webview stack:
  * Linux: `libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`
  * macOS: Xcode CLT
  * Windows: WebView2 (preinstalled on Win11) + MSVC
* Node 18+

## Run

```bash
cd shell
npm install
npm run tauri dev
```

The window loads both sample manifests from `../../manifests/` into a
persistent `Lab` (mock fleet). Every run books coverage — switch to the
Coverage tab after playing with the command console to see the heatmap move.

## Layout

```
src/                 React + TypeScript UI
  App.tsx            views: commands · events · servers · limits · coverage · scenarios · discovery
  components/
    widgets.tsx      arg type → widget table (the generative-UI core)
    CommandForm.tsx  generated form + confirm flow for destructive commands
    Timeline.tsx     scenario report timeline
    CoverageHeatmap.tsx
src-tauri/           Rust side: one persistent Lab in managed state
  src/lib.rs         commands: workspaces · manifest_detail · run_command ·
                     run_scenario · coverage · discover_local · set_backend
```

The hardware toggle (`set_backend`) intentionally answers with a typed
`capability` refusal until the real `CommandSink` adapter over `TvClient` /
the cctv facade lands (v0.2).
