//! Verifies the data plane behind the shell's IPC commands against the real
//! repo manifests (`../../manifests`, `../../scenarios`).
//!
//! The window cannot be driven headlessly, so this exercises the *same*
//! [`build_lab`] wiring `run()` installs and the same `Lab` calls each
//! `#[tauri::command]` wraps. A green run means the desktop UI is reading
//! genuine engine state rather than fixtures.

use cratelab_shell::build_lab;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn lab() -> lab_engine::Lab {
    build_lab(&repo_root().join("manifests"))
}

/// `workspaces` + `manifest_detail`: both authored manifests load, with the
/// versions the TOML declares.
#[test]
fn workspaces_and_manifest_detail_read_the_real_manifests() {
    let lab = lab();
    assert_eq!(
        lab.manifest_ids(),
        vec!["cctv-rs".to_string(), "tv-rs".to_string()],
        "the shell must see both authored manifests"
    );

    let tv = lab.manifest("tv-rs").unwrap();
    assert_eq!(tv.workspace.version, "0.6.0");
    assert_eq!(tv.workspace.facade, "tv-rs");
    assert_eq!(tv.commands.len(), 10);

    // `manifest_detail` serialises the manifest straight to the webview.
    let v = serde_json::to_value(tv).unwrap();
    assert_eq!(v["workspace"]["name"], "tv-rs");
    assert_eq!(v["commands"].as_array().unwrap().len(), 10);
}

/// `run_command`: a real invocation books a real outcome, and the destructive
/// gate refuses without confirmation — then allows it once armed.
#[test]
fn run_command_invokes_and_enforces_the_destructive_gate() {
    let mut lab = lab();

    let out = lab.invoke("tv-rs", "tv.power.wake", BTreeMap::new(), None);
    assert!(out.is_ok(), "safe command must execute: {:?}", out.err());

    let refused = lab.invoke("tv-rs", "tv.power.off", BTreeMap::new(), None);
    assert!(refused.is_err(), "destructive command must be refused unconfirmed");

    lab.set_confirm_destructive(true);
    let allowed = lab.invoke("tv-rs", "tv.power.off", BTreeMap::new(), None);
    lab.set_confirm_destructive(false);
    assert!(allowed.is_ok(), "confirmed destructive command must execute: {:?}", allowed.err());
}

/// `coverage`: the shared ledger has real rows for the invocations above.
#[test]
fn coverage_json_reports_the_shared_ledger() {
    let mut lab = lab();
    lab.invoke("tv-rs", "tv.power.wake", BTreeMap::new(), None).unwrap();

    let cov = lab.coverage_json("tv-rs").unwrap();
    assert_eq!(cov["workspace"], "tv-rs");
    assert_eq!(cov["version"], "0.6.0");
    let rows = cov["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 10, "one heatmap row per declared command");
    let wake = rows.iter().find(|r| r["command"] == "tv.power.wake").unwrap();
    assert_eq!(wake["pass"], 1);
    assert!(cov["coverage_percent"].as_f64().unwrap() > 0.0);
}

/// `run_scenario`: the authored playlist runs end to end in the engine.
#[test]
fn run_scenario_executes_the_authored_playlist() {
    let scenario_path = repo_root().join("scenarios/tv-morning-loop.toml");
    let raw = std::fs::read_to_string(&scenario_path).unwrap();
    let scenario = lab_scenarios::Scenario::from_toml_str(&raw).unwrap();

    let mut lab = lab();
    let report = lab.run_scenario(&scenario).unwrap();

    assert_eq!(report.name, "tv-morning-loop");
    assert!(report.ok, "authored scenario must pass: {:#?}", report.steps);
    assert_eq!(report.steps.len(), 5);
    assert!(report.steps.iter().all(|s| s.status == "pass"));
}

/// `discover_local`: the discovery bug fix reaches the shell too — workspace
/// members using `version.workspace = true` report the inherited version.
#[test]
fn discover_local_reports_inherited_versions() {
    let ws = lab_engine::discovery::discover_local(&repo_root()).unwrap();
    assert!(ws.members.len() >= 7);
    assert!(
        ws.members.iter().all(|m| m.version != "0.0.0"),
        "inherited versions must resolve: {:?}",
        ws.members.iter().map(|m| (&m.name, &m.version)).collect::<Vec<_>>()
    );
    assert_eq!(ws.member("lab-engine").unwrap().version, "0.1.0");
}
