//! `lab` — the headless entry point. What the Tauri shell does with pixels,
//! this binary does with stdout, so the whole lab is CI-drivable:
//!
//! ```text
//! lab discover  ~/code/tv-rs
//! lab manifests manifests/
//! lab run       scenarios/tv-morning-loop.toml --confirm
//! lab coverage  manifests/tv-rs.lab.toml
//! lab gate      manifests/ scenarios/     # warn-only pre-release gate
//! ```

use lab_engine::discovery;
use lab_engine::Lab;
use lab_errors::LabError;
use lab_manifest::LabManifest;
use lab_mocks::MockFleet;
use lab_scenarios::Scenario;

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("remedy: {:?}", e.remedy());
            ExitCode::from(e.exit_code() as u8)
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, LabError> {
    match args.first().map(|s| s.as_str()) {
        Some("discover") => {
            let root = args
                .get(1)
                .ok_or_else(|| usage("discover <workspace-root>"))?;
            let ws = discovery::discover_local(Path::new(root))?;
            println!("workspace {} ({}) — {} member crates:", ws.name, ws.root, ws.members.len());
            for m in &ws.members {
                println!("  {:<28} {}", m.name, m.version);
            }
            Ok(ExitCode::SUCCESS)
        }
        Some("manifests") => {
            let dir = args.get(1).ok_or_else(|| usage("manifests <dir>"))?;
            let mut loaded = 0;
            for file in list_toml(dir)? {
                let raw = std::fs::read_to_string(&file)
                    .map_err(|e| LabError::Manifest { path: file.display().to_string(), problem: e.to_string() })?;
                match LabManifest::from_toml_str(&raw) {
                    Ok(m) => {
                        println!(
                            "ok   {:<26} v{}  commands={} events={} servers={} limits={}",
                            m.workspace.name,
                            m.workspace.version,
                            m.commands.len(),
                            m.events.len(),
                            m.servers.len(),
                            m.limits.len(),
                        );
                        loaded += 1;
                    }
                    Err(e) => println!("FAIL {}", e),
                }
            }
            println!("{loaded} manifest(s) loaded");
            Ok(ExitCode::SUCCESS)
        }
        Some("run") => {
            let scenario_path = args.get(1).ok_or_else(|| usage("run <scenario.toml> [--manifest <file>] [--confirm]"))?;
            let confirm = args.iter().any(|a| a == "--confirm");
            let manifest_file = args
                .iter()
                .position(|a| a == "--manifest")
                .and_then(|i| args.get(i + 1))
                .cloned();
            let scenario_raw = std::fs::read_to_string(scenario_path).map_err(|e| LabError::Scenario {
                step: "<load>".into(),
                cause: e.to_string(),
            })?;
            let scenario = Scenario::from_toml_str(&scenario_raw)?;
            let manifest = match manifest_file {
                Some(f) => load_manifest(&f)?,
                None => auto_pick_manifest(Path::new(scenario_path).parent().unwrap_or(Path::new(".")), &scenario)?,
            };
            let ws_name = manifest.workspace.name.clone();
            let fleet = MockFleet::from_manifest(&manifest);
            let mut lab = Lab::new(Box::new(fleet), confirm);
            lab.load_manifest(manifest)?;
            let report = lab.run_scenario(&scenario)?;
            println!("scenario '{}' — {}", report.name, if report.ok { "PASS" } else { "FAIL" });
            for s in &report.steps {
                println!(
                    "  {:<28} {:<8} {}{}",
                    s.name,
                    s.status,
                    s.outcome.map(|o| o.as_str()).unwrap_or("-"),
                    s.detail
                        .as_ref()
                        .map(|d| format!("  ({d})"))
                        .unwrap_or_default(),
                );
            }
            println!();
            print!("{}", lab.coverage_markdown(&ws_name)?);
            Ok(ExitCode::from(if report.ok { 0 } else { 2 }))
        }
        Some("coverage") => {
            let file = args.get(1).ok_or_else(|| usage("coverage <manifest.toml>"))?;
            let m = load_manifest(file)?;
            let fleet = MockFleet::from_manifest(&m);
            let mut lab = Lab::new(Box::new(fleet), false);
            lab.load_manifest(m)?;
            print!("{}", lab.coverage_markdown(&lab.manifest_ids()[0])?);
            Ok(ExitCode::SUCCESS)
        }
        Some("conform") => {
            let file = args.get(1).ok_or_else(|| usage("conform <manifest.toml>"))?;
            let m = load_manifest(file)?;
            let fleet = MockFleet::from_manifest(&m);
            let mut lab = Lab::new(Box::new(fleet), false);
            lab.load_manifest(m.clone())?;
            let report = lab_conformance::run(&m, lab.sink());
            print!("{}", report.to_markdown());
            if report.violations() > 0 {
                println!("\nremedy: fix the violations above, then re-run 'lab conform'");
            }
            Ok(ExitCode::SUCCESS)
        }
        Some("gate") => gate(args),
        Some(other) => Err(usage(&format!("unknown command '{other}'"))),
        None => Err(usage(
            "discover | manifests | run | coverage | conform | gate",
        )),
    }
}

/// The pre-release gate. Runs the whole lab over every manifest and scenario
/// and reports the result, but — while the scenario set is still expanding —
/// it is **warn-only**: it never fails the build. Tighten this by returning a
/// non-zero code once the coverage floor is meaningful.
///
/// ```text
/// lab gate [manifests-dir] [scenarios-dir]
/// ```
fn gate(args: &[String]) -> Result<ExitCode, LabError> {
    let manifests_dir = args.get(1).map(String::as_str).unwrap_or("manifests");
    let scenarios_dir = args.get(2).map(String::as_str).unwrap_or("scenarios");

    let mut manifests = Vec::new();
    let mut bad_manifests = 0;
    for file in list_toml(manifests_dir)? {
        match load_manifest(&file.to_string_lossy()) {
            Ok(m) => {
                println!(
                    "manifest ok    {:<12} v{}  commands={} events={} servers={} limits={}",
                    m.workspace.name,
                    m.workspace.version,
                    m.commands.len(),
                    m.events.len(),
                    m.servers.len(),
                    m.limits.len(),
                );
                manifests.push(m);
            }
            Err(e) => {
                println!("manifest FAIL  {}  —  {e}", file.display());
                bad_manifests += 1;
            }
        }
    }

    if manifests.is_empty() {
        println!("\ngate: no manifests loaded from '{manifests_dir}' — nothing to exercise");
        return Ok(ExitCode::SUCCESS);
    }

    // One fleet, every workspace — the same wiring the shell uses, so a green
    // gate means the console has real devices behind it too.
    let fleet = MockFleet::from_manifests(&manifests);
    let mut lab = Lab::new(Box::new(fleet), true);
    for m in manifests {
        lab.load_manifest(m)?;
    }

    let mut passed = 0;
    let mut failed = 0;
    println!();
    for file in list_toml(scenarios_dir)? {
        let raw = std::fs::read_to_string(&file).map_err(|e| LabError::Scenario {
            step: file.display().to_string(),
            cause: e.to_string(),
        })?;
        let scenario = match Scenario::from_toml_str(&raw) {
            Ok(s) => s,
            Err(e) => {
                println!("scenario FAIL  {}  —  {e}", file.display());
                failed += 1;
                continue;
            }
        };
        match lab.run_scenario(&scenario) {
            Ok(report) if report.ok => {
                println!("scenario pass  {:<24} {} step(s)", report.name, report.steps.len());
                passed += 1;
            }
            Ok(report) => {
                let ok_steps = report.steps.iter().filter(|s| s.status == "pass").count();
                println!(
                    "scenario FAIL  {:<24} {}/{} step(s) ok",
                    report.name,
                    ok_steps,
                    report.steps.len()
                );
                for s in report.steps.iter().filter(|s| s.status != "pass") {
                    println!(
                        "                 ↳ {:<22} {:<8} {}{}",
                        s.name,
                        s.status,
                        s.outcome.map(|o| o.as_str()).unwrap_or("-"),
                        s.detail
                            .as_ref()
                            .map(|d| format!("  ({d})"))
                            .unwrap_or_default(),
                    );
                }
                failed += 1;
            }
            Err(e) => {
                println!("scenario FAIL  {}  —  {e}", file.display());
                failed += 1;
            }
        }
    }

    println!();
    for id in lab.manifest_ids() {
        print!("{}", lab.coverage_markdown(&id)?);
    }

    // --- conformance properties (C2) -------------------------------------
    // Runs against the same fleet the console and the scenarios use, so a
    // green property suite means the real wiring is exercised.
    println!("\nconformance (property-based):");
    let mut total_cases = 0usize;
    let mut total_violations = 0usize;
    let mut total_must = 0usize;
    for id in lab.manifest_ids() {
        let manifest = lab.manifest(&id)?.clone();
        let report = lab_conformance::run(&manifest, lab.sink());
        total_cases += report.cases();
        total_violations += report.violations();
        total_must += report.must_violations();
        println!(
            "  {:<12} {} case(s)  {} violation(s)  ({} on 'must' commands)",
            id,
            report.cases(),
            report.violations(),
            report.must_violations(),
        );
        for c in report.commands.iter().filter(|c| !c.ok()) {
            for v in &c.violations {
                println!("      ↳ {} on `{}`: {}", format!("{:?}", v.kind).to_lowercase(), c.command, v.detail);
            }
        }
    }

    // --- thresholds -------------------------------------------------------
    // Explicit floors, printed alongside the measured value so a reader sees
    // how far off a workspace is instead of a bare pass/fail.
    println!("\nthresholds:");
    let mut below_floor = Vec::new();
    for id in lab.manifest_ids() {
        let cov = lab.coverage_json(&id)?;
        let pct = cov["coverage_percent"].as_f64().unwrap_or(0.0);
        let met = pct >= COVERAGE_FLOOR_PERCENT;
        println!(
            "  {:<12} coverage {:>5.1}%  (floor {:.1}%)  {}",
            id,
            pct,
            COVERAGE_FLOOR_PERCENT,
            if met { "met" } else { "BELOW" },
        );
        if !met {
            let uncovered: Vec<String> = cov["rows"]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .filter(|r| r["runs"].as_u64() == Some(0))
                        .filter_map(|r| r["command"].as_str().map(str::to_string))
                        .take(5)
                        .collect()
                })
                .unwrap_or_default();
            if !uncovered.is_empty() {
                below_floor.push(format!("{id}: {}", uncovered.join(", ")));
            }
        }
    }

    println!(
        "\ngate: {bad_manifests} bad manifest(s) · {passed} scenario(s) pass · {failed} scenario(s) fail",
    );
    println!(
        "gate: {total_cases} propert(y/ies) case(s) · {total_violations} violation(s) ({total_must} on 'must')",
    );

    let dirty = bad_manifests > 0 || failed > 0 || total_violations > 0 || !below_floor.is_empty();
    if dirty {
        println!("gate: WARN-ONLY — build not failed; tighten once the scenario set stabilises");
    } else {
        println!("gate: all green");
    }
    if !below_floor.is_empty() {
        println!("gate: next step — add scenarios covering:");
        for line in &below_floor {
            println!("         {line}");
        }
    }
    if total_must > 0 {
        println!("gate: next step — fix the 'must'-tier property violations before publishing");
    }
    Ok(ExitCode::SUCCESS)
}

/// Coverage floor a workspace must clear before the gate stops complaining.
///
/// Deliberately a named constant rather than a flag: a threshold nobody can
/// find is a threshold nobody tightens.
const COVERAGE_FLOOR_PERCENT: f64 = 60.0;

fn usage(hint: &str) -> LabError {
    LabError::Scenario {
        step: "<cli>".into(),
        cause: format!("usage: lab {hint}"),
    }
}

fn load_manifest(path: &str) -> Result<LabManifest, LabError> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| LabError::Manifest { path: path.into(), problem: e.to_string() })?;
    LabManifest::from_toml_str(&raw)
}

/// Pick the manifest that declares the scenario's first command.
fn auto_pick_manifest(dir: &Path, scenario: &Scenario) -> Result<LabManifest, LabError> {
    let first_cmd = scenario
        .steps
        .iter()
        .filter_map(|s| s.run.clone())
        .next()
        .ok_or_else(|| LabError::Scenario {
            step: scenario.name.clone(),
            cause: "scenario has no declarative steps to match a manifest with".into(),
        })?;
    for file in list_toml(dir.to_string_lossy().as_ref())? {
        if let Ok(m) = load_manifest(&file.to_string_lossy()) {
            if m.command(&first_cmd).is_some() {
                return Ok(m);
            }
        }
    }
    Err(LabError::Manifest {
        path: dir.display().to_string(),
        problem: format!("no manifest in this directory declares '{first_cmd}'"),
    })
}

fn list_toml(dir: &str) -> Result<Vec<std::path::PathBuf>, LabError> {
    let entries = std::fs::read_dir(dir).map_err(|e| LabError::Manifest {
        path: dir.into(),
        problem: e.to_string(),
    })?;
    let mut files: Vec<std::path::PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "toml").unwrap_or(false))
        .collect();
    files.sort();
    Ok(files)
}
