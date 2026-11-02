//! Local workspace discovery: read a checkout's virtual-manifest `members`,
//! resolve glob patterns (`crates/*`) against the filesystem, and read each
//! member's `package.name` / `package.version`.
//!
//! Deliberately dependency-light (no `cargo metadata` subprocess, no
//! walkdir): the lab's discovery only needs what a manifest can express,
//! and it must work identically on CI machines without cargo installed.

use crate::{CrateInfo, LocalWorkspace};
use lab_errors::{LabError, LabResult};
use std::path::{Path, PathBuf};

/// Discover a workspace rooted at `root` (the directory holding the
/// virtual `Cargo.toml`).
pub fn discover_local(root: &Path) -> LabResult<LocalWorkspace> {
    let manifest_path = root.join("Cargo.toml");
    let raw = std::fs::read_to_string(&manifest_path).map_err(|e| LabError::Discovery {
        phase: "local".into(),
        message: format!("cannot read {}: {e}", manifest_path.display()),
    })?;
    let value: toml::Value = toml::from_str(&raw).map_err(|e| LabError::Discovery {
        phase: "local".into(),
        message: format!("{} is not valid TOML: {e}", manifest_path.display()),
    })?;

    let workspace_name = root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "workspace".into());

    let members_value = value
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .ok_or_else(|| LabError::Discovery {
            phase: "local".into(),
            message: format!("{} has no [workspace] members array", manifest_path.display()),
        })?;

    // `version.workspace = true` in a member resolves against the virtual
    // manifest's `[workspace.package] version`; without this, every inheriting
    // crate would be reported as 0.0.0.
    let inherited_version = value
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut members: Vec<CrateInfo> = Vec::new();
    for entry in members_value {
        match entry.as_str() {
            Some(pattern) => resolve_pattern(root, pattern, inherited_version.as_deref(), &mut members)?,
            None => {
                // Inline table form: { path = "crates/x" }
                if let Some(path) = entry.get("path").and_then(|p| p.as_str()) {
                    resolve_pattern(root, path, inherited_version.as_deref(), &mut members)?;
                }
            }
        }
    }
    members.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(LocalWorkspace {
        name: workspace_name,
        root: root.to_string_lossy().to_string(),
        members,
    })
}

fn resolve_pattern(
    root: &Path,
    pattern: &str,
    inherited_version: Option<&str>,
    out: &mut Vec<CrateInfo>,
) -> LabResult<()> {
    let (dir, tail) = match pattern.split_once('*') {
        Some((prefix, suffix)) => {
            let dir = prefix.trim_end_matches('/');
            let _ = suffix;
            (dir, true)
        }
        None => (pattern, false),
    };
    let base = root.join(dir);
    if tail {
        let entries = std::fs::read_dir(&base).map_err(|e| LabError::Discovery {
            phase: "local".into(),
            message: format!("cannot list {}: {e}", base.display()),
        })?;
        let mut subdirs: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.join("Cargo.toml").is_file())
            .collect();
        subdirs.sort();
        for sub in subdirs {
            if let Some(info) = read_member(&sub, inherited_version)? {
                out.push(info);
            }
        }
    } else if base.join("Cargo.toml").is_file() {
        if let Some(info) = read_member(&base, inherited_version)? {
            out.push(info);
        }
    }
    Ok(())
}

fn read_member(dir: &Path, inherited_version: Option<&str>) -> LabResult<Option<CrateInfo>> {
    let cargo = dir.join("Cargo.toml");
    let raw = std::fs::read_to_string(&cargo).map_err(|e| LabError::Discovery {
        phase: "local".into(),
        message: format!("cannot read {}: {e}", cargo.display()),
    })?;
    let value: toml::Value = toml::from_str(&raw).map_err(|e| LabError::Discovery {
        phase: "local".into(),
        message: format!("{} invalid TOML: {e}", cargo.display()),
    })?;
    let package = match value.get("package") {
        Some(p) => p,
        // Virtual manifests inside workspaces are skipped, not errors.
        None => return Ok(None),
    };
    let name = package
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| LabError::Discovery {
            phase: "local".into(),
            message: format!("{} has no package.name", cargo.display()),
        })?
        .to_string();
    let version = resolve_version(package.get("version"), inherited_version);
    Ok(Some(CrateInfo {
        name,
        version,
        path: dir.to_string_lossy().to_string(),
    }))
}

/// Resolve a member's `package.version`, honouring `version.workspace = true`
/// (which inherits `[workspace.package] version` from the root manifest).
fn resolve_version(value: Option<&toml::Value>, inherited: Option<&str>) -> String {
    match value {
        Some(toml::Value::String(s)) => s.clone(),
        Some(toml::Value::Table(t)) if t.get("workspace").and_then(|w| w.as_bool()) == Some(true) => {
            inherited.unwrap_or("0.0.0").to_string()
        }
        _ => inherited.unwrap_or("0.0.0").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn discovers_members_through_globs() {
        let root = std::env::temp_dir().join(format!("lab-disc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        write(
            &root.join("Cargo.toml"),
            "[workspace]\nresolver = \"2\"\nmembers = [\"crates/*\", \"xtask\"]\n",
        );
        write(
            &root.join("crates/core/Cargo.toml"),
            "[package]\nname = \"w-core\"\nversion = \"0.2.0\"\n",
        );
        write(
            &root.join("crates/facade/Cargo.toml"),
            "[package]\nname = \"w\"\nversion = \"0.6.0\"\n",
        );
        write(
            &root.join("xtask/Cargo.toml"),
            "[package]\nname = \"w-xtask\"\nversion = \"0.1.0\"\n",
        );

        let ws = discover_local(&root).unwrap();
        assert_eq!(ws.members.len(), 3);
        assert_eq!(ws.member("w").unwrap().version, "0.6.0");
        assert_eq!(ws.member("w-core").unwrap().version, "0.2.0");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn inherits_workspace_package_version() {
        let root = std::env::temp_dir().join(format!("lab-disc-inherit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        write(
            &root.join("Cargo.toml"),
            "[workspace]\nresolver = \"2\"\nmembers = [\"crates/*\"]\n\n[workspace.package]\nversion = \"0.6.0\"\n",
        );
        write(
            &root.join("crates/core/Cargo.toml"),
            "[package]\nname = \"w-core\"\nversion.workspace = true\n",
        );
        write(
            &root.join("crates/facade/Cargo.toml"),
            "[package]\nname = \"w\"\nversion = \"0.9.1\"\n",
        );

        let ws = discover_local(&root).unwrap();
        assert_eq!(
            ws.member("w-core").unwrap().version,
            "0.6.0",
            "version.workspace = true must resolve against [workspace.package]"
        );
        assert_eq!(ws.member("w").unwrap().version, "0.9.1");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_workspace_is_discovery_error() {
        let root = std::env::temp_dir().join(format!("lab-disc-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let err = discover_local(&root).unwrap_err();
        assert!(matches!(err, LabError::Discovery { .. }));
        let _ = std::fs::remove_dir_all(&root);
    }
}
