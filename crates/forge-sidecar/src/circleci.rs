//! The read-only CircleCI gate dashboard.
//!
//! The forge treats CI as a *gate*, not an alarm: what the dashboard answers is
//! "is the gate green on the branch I am about to evolve?" All calls here are
//! `GET`s against the CircleCI v2 API. This module never starts a pipeline,
//! edits a project, or writes a file.
//!
//! Credentials are named, never stored: `CIRCLECI_TOKEN` holds the token and
//! `CIRCLECI_PROJECT_SLUGS` holds a comma-separated watch list of
//! `vcs/org/repo` slugs.

use forge_core::{ForgeError, ForgeResult};
use serde_json::{json, Value};

use crate::env::Env;
use crate::transport::{HttpTransport, TransportError};

/// The CircleCI v2 API root.
pub const API: &str = "https://circleci.com/api/v2";

/// The most projects a dashboard will load in one pass.
pub const MAX_PROJECTS: usize = 12;

/// How many recent pipelines are summarised per project.
pub const PIPELINES_PER_PROJECT: usize = 5;

/// How many workflows are summarised for the newest pipeline.
pub const WORKFLOWS_PER_PIPELINE: usize = 8;

/// Whether `slug` is a well-formed `vcs/org/repo`.
fn valid_slug(slug: &str) -> bool {
    let parts: Vec<&str> = slug.split('/').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
}

/// Split a watch list into validated, de-duplicated slugs.
fn slugs(configured: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for value in configured.split([' ', ',', ';']) {
        let slug = value.trim();
        if !slug.is_empty() && valid_slug(slug) && !out.iter().any(|seen| seen == slug) {
            out.push(slug.to_string());
            if out.len() == MAX_PROJECTS {
                break;
            }
        }
    }
    out
}

/// One CircleCI JSON path, surfacing the HTTP status on failure.
fn get(transport: &dyn HttpTransport, token: &str, path: &str) -> Result<Value, u16> {
    let headers = vec![
        ("Circle-Token", token.to_string()),
        ("Accept", "application/json".to_string()),
    ];
    match transport.get(&format!("{API}{path}"), &headers) {
        Ok(response) => Ok(response.body),
        Err(TransportError::Status { status, .. }) => Err(status),
        Err(TransportError::Network(_)) => Err(0),
    }
}

/// Summarise one project: connection state, recent pipelines and workflows.
fn project_summary(transport: &dyn HttpTransport, token: &str, slug: &str) -> Value {
    let fallback_name = slug.rsplit('/').next().unwrap_or(slug);
    let project = match get(transport, token, &format!("/project/{slug}")) {
        Ok(value) => value,
        Err(code) => {
            return json!({
                "slug": slug,
                "name": fallback_name,
                "connected": false,
                "state": if code == 404 { "not-connected" } else { "unavailable" },
                "error": if code == 404 {
                    "Project is not connected in CircleCI or is not visible to this token."
                } else if code == 0 {
                    "CircleCI could not be reached."
                } else {
                    "CircleCI project lookup failed."
                },
                "http_status": if code == 0 { Value::Null } else { json!(code) },
                "pipelines": [],
                "latest_workflows": []
            });
        }
    };

    let pipelines = match get(transport, token, &format!("/project/{slug}/pipeline")) {
        Ok(value) => value["items"].as_array().cloned().unwrap_or_default(),
        Err(code) => {
            return json!({
                "slug": slug,
                "name": project["name"].as_str().unwrap_or(fallback_name),
                "connected": true,
                "state": "pipeline-unavailable",
                "error": if code == 0 {
                    "CircleCI could not be reached while loading pipelines."
                } else {
                    "Connected, but CircleCI did not return pipeline history."
                },
                "http_status": if code == 0 { Value::Null } else { json!(code) },
                "pipelines": [],
                "latest_workflows": []
            });
        }
    };

    let pipeline_rows: Vec<Value> = pipelines
        .iter()
        .take(PIPELINES_PER_PROJECT)
        .map(|pipeline| {
            json!({
                "id": pipeline["id"],
                "number": pipeline["number"],
                "state": pipeline["state"],
                "created_at": pipeline["created_at"],
                "branch": pipeline["vcs"]["branch"],
                "revision": pipeline["vcs"]["revision"],
                "trigger": pipeline["trigger"]["type"]
            })
        })
        .collect();

    let workflow_rows: Vec<Value> = pipelines
        .first()
        .and_then(|pipeline| pipeline["id"].as_str())
        .and_then(|id| get(transport, token, &format!("/pipeline/{id}/workflow")).ok())
        .and_then(|value| value["items"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .take(WORKFLOWS_PER_PIPELINE)
        .map(|workflow| {
            json!({
                "id": workflow["id"],
                "name": workflow["name"],
                "status": workflow["status"],
                "created_at": workflow["created_at"],
                "stopped_at": workflow["stopped_at"]
            })
        })
        .collect();

    let pipeline_state = pipelines
        .first()
        .and_then(|pipeline| pipeline["state"].as_str())
        .unwrap_or("no-pipelines");
    let state = workflow_rows
        .first()
        .and_then(|workflow| workflow["status"].as_str())
        .unwrap_or(pipeline_state);

    json!({
        "slug": slug,
        "name": project["name"].as_str().unwrap_or(fallback_name),
        "connected": true,
        "state": state,
        "error": Value::Null,
        "http_status": 200,
        "pipelines": pipeline_rows,
        "latest_workflows": workflow_rows
    })
}

/// The read-only CI gate dashboard.
///
/// Refuses before any network use when `CIRCLECI_TOKEN` is absent; a single
/// project being unavailable degrades that row, not the whole dashboard.
pub fn circleci_dashboard(env: &Env, transport: &dyn HttpTransport) -> ForgeResult<Value> {
    let token = env.require("circleci", "CIRCLECI_TOKEN")?;
    let configured = env.get("CIRCLECI_PROJECT_SLUGS").unwrap_or("");

    let me = get(transport, token, "/me").map_err(|code| ForgeError::Remote {
        remote: "circleci".to_string(),
        problem: match code {
            0 => "could not reach CircleCI; check network access".to_string(),
            401 | 403 => "CircleCI rejected this token (check its validity and project access)"
                .to_string(),
            _ => format!("account check failed (HTTP {code})"),
        },
    })?;

    let organizations: Vec<Value> = get(transport, token, "/me/collaborations")
        .ok()
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .map(|org| {
            json!({
                "name": org["name"],
                "slug": org["slug"],
                "vcs_type": org["vcs_type"]
            })
        })
        .collect();

    let watch: Vec<String> = slugs(configured);
    let projects: Vec<Value> = watch
        .iter()
        .map(|slug| project_summary(transport, token, slug))
        .collect();
    let connected_count = projects
        .iter()
        .filter(|project| project["connected"] == true)
        .count();

    Ok(json!({
        "account": { "name": me["name"], "login": me["login"] },
        "organizations": organizations,
        "projects": projects,
        "configured_count": watch.len(),
        "connected_count": connected_count,
        "limits": { "max_projects": MAX_PROJECTS, "pipelines_per_project": PIPELINES_PER_PROJECT },
        "setup": { "project_slugs_configured": !configured.trim().is_empty() }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{HttpResponse, MockTransport};

    fn configured() -> Env {
        let mut env = Env::new();
        env.set("CIRCLECI_TOKEN", "cc_test");
        env.set("CIRCLECI_PROJECT_SLUGS", "gh/cratelab/cratelab, gh/lab/tv-rs");
        env
    }

    #[test]
    fn the_watch_list_is_validated_deduplicated_and_capped() {
        assert!(valid_slug("gh/cratelab/cratelab"));
        assert!(!valid_slug("cratelab"), "a slug is vcs/org/repo");
        assert!(!valid_slug("gh/cratelab/"), "an empty part is not a slug");
        assert_eq!(slugs("gh/o/r, gh/o/r; gh/o/r2"), vec!["gh/o/r", "gh/o/r2"]);
        assert_eq!(slugs("bogus"), Vec::<String>::new());
        let many: Vec<String> = (0..20).map(|n| format!("gh/o/r{n}")).collect();
        assert_eq!(slugs(&many.join(",")).len(), MAX_PROJECTS);
    }

    #[test]
    fn a_missing_token_refuses_before_any_network_use() {
        let transport = MockTransport::new();
        let error = circleci_dashboard(&Env::new(), &transport).expect_err("no token");
        assert!(matches!(error, ForgeError::Credential { ref key, .. } if key == "CIRCLECI_TOKEN"));
        assert!(transport.calls().is_empty());
    }

    #[test]
    fn the_dashboard_summarises_account_projects_and_workflows() {
        let transport = MockTransport::new()
            .route(
                "/me/collaborations",
                HttpResponse::json(json!([{"name": "CrateLab", "slug": "gh/cratelab", "vcs_type": "github"}])),
            )
            .route("/me", HttpResponse::json(json!({"name": "Zak", "login": "zak"})))
            .route(
                "/project/gh/lab/tv-rs/pipeline",
                HttpResponse::json(json!({"items": [{"id": "p2", "number": 2, "state": "created"}]})),
            )
            .route(
                "/project/gh/lab/tv-rs",
                HttpResponse::json(json!({"name": "tv-rs", "slug": "gh/lab/tv-rs"})),
            )
            .route(
                "/project/gh/cratelab/cratelab/pipeline",
                HttpResponse::json(json!({"items": [
                    {"id": "p1", "number": 7, "state": "created",
                     "created_at": "2026-01-01T00:00:00Z",
                     "vcs": {"branch": "main", "revision": "deadbeef"},
                     "trigger": {"type": "webhook"}}
                ]})),
            )
            .route(
                "/pipeline/p1/workflow",
                HttpResponse::json(json!({"items": [
                    {"id": "w1", "name": "rust-gates", "status": "success",
                     "created_at": "2026-01-01T00:00:00Z", "stopped_at": "2026-01-01T00:02:00Z"}
                ]})),
            )
            .route(
                "/project/gh/cratelab/cratelab",
                HttpResponse::json(json!({"name": "cratelab", "slug": "gh/cratelab/cratelab"})),
            );

        let dashboard = circleci_dashboard(&configured(), &transport).expect("reads");
        assert_eq!(dashboard["account"]["login"], "zak");
        assert_eq!(dashboard["organizations"][0]["slug"], "gh/cratelab");
        assert_eq!(dashboard["configured_count"], 2);
        assert_eq!(dashboard["connected_count"], 2);
        assert_eq!(dashboard["setup"]["project_slugs_configured"], true);

        let cratelab = &dashboard["projects"][0];
        assert_eq!(cratelab["name"], "cratelab");
        assert_eq!(cratelab["state"], "success", "the workflow status wins");
        assert_eq!(cratelab["pipelines"][0]["branch"], "main");
        assert_eq!(cratelab["latest_workflows"][0]["name"], "rust-gates");

        // The token rides in the Circle-Token header, never in the URL.
        assert!(transport.calls().iter().all(|call| {
            call.url.starts_with(API)
                && call
                    .headers
                    .iter()
                    .any(|(name, value)| name == "Circle-Token" && value == "cc_test")
        }));
    }

    #[test]
    fn an_unreachable_project_degrades_its_row_not_the_dashboard() {
        let transport = MockTransport::new()
            .route("/me/collaborations", HttpResponse::json(json!([])))
            .route("/me", HttpResponse::json(json!({"name": "Zak", "login": "zak"})))
            .route(
                "/project/gh/lab/tv-rs",
                HttpResponse {
                    status: 404,
                    body: json!({"message": "not found"}),
                },
            );

        let dashboard = circleci_dashboard(&configured(), &transport).expect("reads");
        let tv = &dashboard["projects"][1];
        assert_eq!(tv["connected"], false);
        assert_eq!(tv["state"], "not-connected");
        assert_eq!(tv["http_status"], json!(404));
        assert_eq!(dashboard["connected_count"], 0);
    }

    #[test]
    fn a_rejected_token_is_a_remote_error_not_a_panic() {
        let transport = MockTransport::new().route(
            "/me",
            HttpResponse {
                status: 401,
                body: json!({"message": "unauthorized"}),
            },
        );
        let error = circleci_dashboard(&configured(), &transport).expect_err("rejected");
        assert!(matches!(error, ForgeError::Remote { .. }));
        assert!(error.to_string().contains("rejected"));
    }
}
