//! Read-only CircleCI operations summary for the Fleet dashboard.
//!
//! This module only issues CircleCI API GET requests. It never starts a
//! pipeline, edits project settings, or writes repository files.

use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const API: &str = "https://circleci.com/api/v2";
const MAX_PROJECTS: usize = 12;

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn valid_slug(slug: &str) -> bool {
    let parts: Vec<_> = slug.split('/').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
}

fn get(agent: &ureq::Agent, token: &str, path: &str) -> Result<Value, u16> {
    match agent
        .get(&format!("{API}{path}"))
        .set("Circle-Token", token)
        .call()
    {
        Ok(response) => response.into_json().map_err(|_| 502),
        Err(ureq::Error::Status(code, _)) => Err(code),
        Err(ureq::Error::Transport(_)) => Err(0),
    }
}

fn project_summary(agent: &ureq::Agent, token: &str, slug: &str) -> Value {
    let project = match get(agent, token, &format!("/project/{slug}")) {
        Ok(value) => value,
        Err(code) => {
            return json!({
                "slug": slug,
                "name": slug.rsplit('/').next().unwrap_or(slug),
                "connected": false,
                "state": if code == 404 { "not-connected" } else { "unavailable" },
                "error": if code == 404 { "Project is not connected in CircleCI or is not visible to this token." } else if code == 0 { "CircleCI could not be reached." } else { "CircleCI project lookup failed." },
                "http_status": if code == 0 { Value::Null } else { json!(code) },
                "pipelines": [],
                "latest_workflows": []
            });
        }
    };

    let pipelines = match get(agent, token, &format!("/project/{slug}/pipeline")) {
        Ok(value) => value["items"].as_array().cloned().unwrap_or_default(),
        Err(code) => {
            return json!({
                "slug": slug,
                "name": project["name"].as_str().unwrap_or_else(|| slug.rsplit('/').next().unwrap_or(slug)),
                "connected": true,
                "state": "pipeline-unavailable",
                "error": if code == 0 { "CircleCI could not be reached while loading pipelines." } else { "Connected, but CircleCI did not return pipeline history." },
                "http_status": if code == 0 { Value::Null } else { json!(code) },
                "pipelines": [],
                "latest_workflows": []
            });
        }
    };

    let pipeline_rows: Vec<Value> = pipelines
        .iter()
        .take(5)
        .map(|p| {
            json!({
                "id": p["id"],
                "number": p["number"],
                "state": p["state"],
                "created_at": p["created_at"],
                "branch": p["vcs"]["branch"],
                "revision": p["vcs"]["revision"],
                "trigger": p["trigger"]["type"]
            })
        })
        .collect();

    let workflows = pipelines
        .first()
        .and_then(|p| p["id"].as_str())
        .and_then(|id| get(agent, token, &format!("/pipeline/{id}/workflow")).ok())
        .and_then(|value| value["items"].as_array().cloned())
        .unwrap_or_default();
    let workflow_rows: Vec<Value> = workflows
        .iter()
        .take(8)
        .map(|w| {
            json!({
                "id": w["id"],
                "name": w["name"],
                "status": w["status"],
                "created_at": w["created_at"],
                "stopped_at": w["stopped_at"]
            })
        })
        .collect();
    let pipeline_state = pipelines
        .first()
        .and_then(|p| p["state"].as_str())
        .unwrap_or("no-pipelines");
    let latest_workflow_state = workflow_rows.first().and_then(|w| w["status"].as_str());
    let state = latest_workflow_state.unwrap_or(pipeline_state);

    json!({
        "slug": slug,
        "name": project["name"].as_str().unwrap_or_else(|| slug.rsplit('/').next().unwrap_or(slug)),
        "connected": true,
        "state": state,
        "error": Value::Null,
        "http_status": 200,
        "pipelines": pipeline_rows,
        "latest_workflows": workflow_rows
    })
}

pub fn dashboard(token: &str, configured_slugs: &str, fleet_dir: &Path) -> Result<Value, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(12))
        .build();
    let me = get(&agent, token, "/me").map_err(|code| match code {
        0 => "Could not reach CircleCI. Check network access and try again.".to_string(),
        401 | 403 => {
            "CircleCI rejected this token. Check its validity and project access in Fleet Settings."
                .to_string()
        }
        _ => format!("CircleCI account check failed (HTTP {code})."),
    })?;
    let collaborations = get(&agent, token, "/me/collaborations").unwrap_or(Value::Null);
    let organizations: Vec<Value> = collaborations
        .as_array()
        .into_iter()
        .flatten()
        .map(|org| {
            json!({
                "name": org["name"],
                "slug": org["slug"],
                "vcs_type": org["vcs_type"]
            })
        })
        .collect();

    let mut slugs = Vec::new();
    let mut seen = HashSet::new();
    for value in configured_slugs.split(|c: char| c == ',' || c == ';' || c.is_whitespace()) {
        let slug = value.trim();
        if !slug.is_empty() && valid_slug(slug) && seen.insert(slug.to_string()) {
            slugs.push(slug.to_string());
            if slugs.len() == MAX_PROJECTS {
                break;
            }
        }
    }
    let projects: Vec<Value> = slugs
        .iter()
        .map(|slug| project_summary(&agent, token, slug))
        .collect();
    let connected_count = projects.iter().filter(|p| p["connected"] == true).count();

    let template_path = fleet_dir.join("circleci/config.yml");
    let target_root = std::env::var_os("TV_RS_ROOT")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::fs::read_to_string(fleet_dir.join(".env"))
                .ok()
                .and_then(|raw| {
                    raw.lines().find_map(|line| {
                        let (key, value) = line.trim().split_once('=')?;
                        (key.trim() == "TV_RS_ROOT").then(|| {
                            std::path::PathBuf::from(value.trim().trim_matches(['\'', '"']))
                        })
                    })
                })
        })
        .unwrap_or_else(|| Path::new(r"C:\laragon\www\tv-rs").to_path_buf());
    let target_config = target_root.join(".circleci/config.yml");

    Ok(json!({
        "checked_at": now_epoch(),
        "account": { "name": me["name"], "login": me["login"] },
        "organizations": organizations,
        "projects": projects,
        "configured_count": slugs.len(),
        "connected_count": connected_count,
        "limits": { "max_projects": MAX_PROJECTS, "pipelines_per_project": 5 },
        "setup": {
            "project_slugs_configured": !configured_slugs.trim().is_empty(),
            "template_present": template_path.is_file(),
            "template_path": template_path.display().to_string(),
            "target_config_present": target_config.is_file(),
            "target_config_path": target_config.display().to_string(),
            "target_root": target_root.display().to_string()
        }
    }))
}
