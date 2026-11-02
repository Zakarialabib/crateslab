//! cratelab shell state: one persistent [`Lab`] (mock fleet + manifests +
//! coverage ledger) lives in managed state, so every command the UI runs is
//! booked into the same heatmap. Hardware stays behind the explicit
//! `set_backend` toggle — the same [`CommandSink`] port serves both.

use lab_engine::{discovery, Lab};
use lab_errors::LabError;
use lab_manifest::LabManifest;
use lab_mocks::MockFleet;
use lab_scenarios::Scenario;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

pub struct LabState {
    lab: Mutex<Lab>,
}

fn load_all_manifests(dir: &Path) -> Vec<LabManifest> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut files: Vec<_> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "toml").unwrap_or(false))
            .collect();
        files.sort();
        for f in files {
            if let Ok(raw) = std::fs::read_to_string(&f) {
                if let Ok(m) = LabManifest::from_toml_str(&raw) {
                    out.push(m);
                }
            }
        }
    }
    out
}

/// Where the shell looks for manifests: a sibling `manifests/` when run from
/// the repo root, otherwise `../../manifests` relative to `src-tauri`.
pub fn manifests_dir() -> PathBuf {
    std::env::current_dir()
        .map(|d| {
            let a = d.join("manifests");
            let b = d.join("../../manifests");
            if a.is_dir() {
                a
            } else if b.is_dir() {
                b.canonicalize().unwrap_or(b)
            } else {
                a
            }
        })
        .unwrap_or_else(|_| PathBuf::from("manifests"))
}

/// Build the lab exactly the way the shell does: every manifest in `dir`, one
/// fleet spanning all workspaces. Shared with the IPC integration test so the
/// test exercises the real wiring instead of a copy of it.
pub fn build_lab(dir: &Path) -> Lab {
    let manifests = load_all_manifests(dir);
    let fleet = MockFleet::from_manifests(&manifests);
    let mut lab = Lab::new(Box::new(fleet), false);
    for m in manifests {
        let _ = lab.load_manifest(m);
    }
    lab
}

fn lab_error_value(e: &LabError) -> serde_json::Value {
    serde_json::json!({
        "kind": e.kind().as_str(),
        "message": e.to_string(),
        "retryable": e.retryable(),
        "remedy": e.remedy(),
        "severity": e.severity(),
    })
}

#[tauri::command]
fn workspaces(state: State<'_, LabState>) -> Result<serde_json::Value, serde_json::Value> {
    let lab = state.lab.lock().expect("lab lock");
    let mut list = Vec::new();
    for id in lab.manifest_ids() {
        let m = lab.manifest(&id).map_err(|e| lab_error_value(&e))?;
        list.push(serde_json::json!({
            "name": m.workspace.name,
            "prefix": m.workspace.prefix,
            "version": m.workspace.version,
            "facade": m.workspace.facade,
            "core": m.workspace.core,
            "repo": m.workspace.repo,
            "commands": m.commands.len(),
            "events": m.events.len(),
            "servers": m.servers.len(),
            "limits": m.limits.len(),
            "device_classes": m.device_classes.iter().map(|d| serde_json::json!({
                "id": d.id, "capabilities": d.capabilities,
            })).collect::<Vec<_>>(),
        }));
    }
    Ok(serde_json::json!(list))
}

#[tauri::command]
fn manifest_detail(
    state: State<'_, LabState>,
    workspace: String,
) -> Result<serde_json::Value, serde_json::Value> {
    let lab = state.lab.lock().expect("lab lock");
    let m = lab.manifest(&workspace).map_err(|e| lab_error_value(&e))?;
    serde_json::to_value(m).map_err(|e| serde_json::json!({ "kind": "internal", "message": e.to_string() }))
}

/// What the UI's backend badge shows. Honest by design: mock until the
/// hardware adapter lands.
#[tauri::command]
fn devices(state: State<'_, LabState>) -> serde_json::Value {
    let lab = state.lab.lock().expect("lab lock");
    let ids = lab.manifest_ids();
    serde_json::json!({
        "backend": "mock",
        "manifests": ids,
        "note": "hardware toggle lands with the real TvClient adapter (v0.2)",
    })
}

/// One invocation from the command console. `confirm` comes from the UI's
/// confirmation dialog and is forwarded to the engine's destructive gate.
#[tauri::command]
fn run_command(
    state: State<'_, LabState>,
    app: AppHandle,
    workspace: String,
    command_id: String,
    args: BTreeMap<String, serde_json::Value>,
    confirm: bool,
) -> Result<serde_json::Value, serde_json::Value> {
    let mut lab = state.lab.lock().expect("lab lock");
    lab.set_confirm_destructive(confirm);
    let res = lab.invoke(&workspace, &command_id, args, None);
    lab.set_confirm_destructive(false);
    match res {
        Ok(out) => {
            let _ = app.emit("lab://event", serde_json::json!({
                "command": command_id,
                "events": out.events,
                "state": out.state,
            }));
            serde_json::to_value(&out)
                .map_err(|e| serde_json::json!({ "kind": "internal", "message": e.to_string() }))
        }
        Err(e) => Err(lab_error_value(&e)),
    }
}

#[tauri::command]
fn run_scenario(
    state: State<'_, LabState>,
    app: AppHandle,
    scenario_toml: String,
    confirm: bool,
) -> Result<serde_json::Value, serde_json::Value> {
    let scenario = Scenario::from_toml_str(&scenario_toml).map_err(|e| lab_error_value(&e))?;
    let mut lab = state.lab.lock().expect("lab lock");
    lab.set_confirm_destructive(confirm);
    let report = lab.run_scenario(&scenario);
    lab.set_confirm_destructive(false);
    let report = report.map_err(|e| lab_error_value(&e))?;
    let _ = app.emit("lab://scenario", &report);
    serde_json::to_value(&report)
        .map_err(|e| serde_json::json!({ "kind": "internal", "message": e.to_string() }))
}

#[tauri::command]
fn coverage(state: State<'_, LabState>, workspace: String) -> Result<serde_json::Value, serde_json::Value> {
    let lab = state.lab.lock().expect("lab lock");
    lab.coverage_json(&workspace).map_err(|e| lab_error_value(&e))
}

#[tauri::command]
fn discover_local(root: String) -> Result<serde_json::Value, serde_json::Value> {
    let ws = discovery::discover_local(std::path::Path::new(&root))
        .map_err(|e| lab_error_value(&e))?;
    serde_json::to_value(&ws)
        .map_err(|e| serde_json::json!({ "kind": "internal", "message": e.to_string() }))
}

/// The hardware toggle. Refused with the vocabulary the whole lab speaks —
/// a typed refusal, not a TODO panic.
#[tauri::command]
fn set_backend(_backend: String) -> Result<serde_json::Value, serde_json::Value> {
    Err(serde_json::json!({
        "kind": "capability",
        "message": "hardware adapter lands in v0.2: implement CommandSink over TvClient / the cctv facade",
        "remedy": { "action": "retry" },
    }))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // One fleet built from every manifest, so the command console and
            // the scenario runner have real devices to talk to.
            let lab = build_lab(&manifests_dir());
            app.manage(LabState {
                lab: Mutex::new(lab),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            workspaces,
            manifest_detail,
            devices,
            run_command,
            run_scenario,
            coverage,
            discover_local,
            set_backend
        ])
        .run(tauri::generate_context!())
        .expect("error while running cratelab shell");
}
