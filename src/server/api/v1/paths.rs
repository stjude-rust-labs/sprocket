//! Single source of truth for the v1 API route templates and URL builders.
//!
//! Each public route in the v1 API is defined here as a `const` template
//! (with `{id}` / `{name}` placeholders, suitable for router registration and
//! OpenAPI annotation) plus a typed builder function that substitutes the
//! placeholder for use by clients and tests.
//!
//! Centralizing the templates here ensures the Axum router, the OpenAPI
//! documentation, the CLI client, and the integration tests all refer to the
//! same canonical path; a route-shape change that misses any of those surfaces
//! becomes a compile error or a single, easy-to-grep edit.

use uuid::Uuid;

/// Swagger UI mount path.
pub const SWAGGER_UI: &str = "/api/v1/swagger-ui";

/// OpenAPI JSON document path.
pub const OPENAPI_JSON: &str = "/api/v1/openapi.json";

// === Server ==========================================================

/// Path template for the server metadata endpoint.
pub const SERVER_INFO: &str = "/api/v1/info";

// === Runs ============================================================

/// Path template for listing all runs (`GET`) and for submitting a new run
/// (`POST`).
pub const LIST_RUNS: &str = "/api/v1/runs";

/// Path template for submitting a new run (`POST`).
///
/// Aliases [`LIST_RUNS`] for clarity at call sites.
pub const SUBMIT_RUN: &str = LIST_RUNS;

/// Path template for getting a single run by id.
pub const GET_RUN: &str = "/api/v1/runs/{id}";

/// Path template for canceling a run.
pub const CANCEL_RUN: &str = "/api/v1/runs/{id}/cancel";

/// Path template for getting a run's outputs.
pub const GET_RUN_OUTPUTS: &str = "/api/v1/runs/{id}/outputs";

/// Path template for localizing a run's remote artifacts.
pub const LOCALIZE_RUN: &str = "/api/v1/runs/{id}/localize";

/// Path template for listing a run's tasks.
pub const LIST_RUN_TASKS: &str = "/api/v1/runs/{id}/tasks";

/// Path template for getting a run's per-status task counts.
pub const RUN_TASK_COUNTS: &str = "/api/v1/runs/{id}/tasks/counts";

// === Sessions ========================================================

/// Path template for listing sessions.
pub const LIST_SESSIONS: &str = "/api/v1/sessions";

/// Path template for getting a single session by id.
pub const GET_SESSION: &str = "/api/v1/sessions/{id}";

// === Tasks ===========================================================

/// Path template for listing all tasks.
pub const LIST_TASKS: &str = "/api/v1/tasks";

/// Path template for getting a single task by name.
pub const GET_TASK: &str = "/api/v1/tasks/{name}";

/// Path template for getting a task's logs.
pub const GET_TASK_LOGS: &str = "/api/v1/tasks/{name}/logs";

// === Builders ========================================================

/// Build the path for [`GET_RUN`].
pub fn get_run(id: Uuid) -> String {
    GET_RUN.replace("{id}", &id.to_string())
}

/// Build the path for [`CANCEL_RUN`].
pub fn cancel_run(id: Uuid) -> String {
    CANCEL_RUN.replace("{id}", &id.to_string())
}

/// Build the path for [`GET_RUN_OUTPUTS`].
pub fn get_run_outputs(id: Uuid) -> String {
    GET_RUN_OUTPUTS.replace("{id}", &id.to_string())
}

/// Build the path for [`LOCALIZE_RUN`].
pub fn localize_run(id: Uuid) -> String {
    LOCALIZE_RUN.replace("{id}", &id.to_string())
}

/// Build the path for [`LIST_RUN_TASKS`].
pub fn list_run_tasks(id: Uuid) -> String {
    LIST_RUN_TASKS.replace("{id}", &id.to_string())
}

/// Build the path for [`RUN_TASK_COUNTS`].
pub fn run_task_counts(id: Uuid) -> String {
    RUN_TASK_COUNTS.replace("{id}", &id.to_string())
}

/// Build the path for [`GET_SESSION`].
pub fn get_session(id: Uuid) -> String {
    GET_SESSION.replace("{id}", &id.to_string())
}

/// Build the path for [`GET_TASK`].
pub fn get_task(name: &str) -> String {
    GET_TASK.replace("{name}", name)
}

/// Build the path for [`GET_TASK_LOGS`].
pub fn get_task_logs(name: &str) -> String {
    GET_TASK_LOGS.replace("{name}", name)
}

// === Router helper ===================================================

/// The full prefix nested by the top-level router and the v1 nest.
///
/// All v1 templates begin with this prefix. The v1 router is mounted under
/// `/api/v1` (the outer router nests `/api`, then the API router nests `/v1`),
/// so registering a route requires a path relative to the v1 nest.
const V1_PREFIX: &str = "/api/v1";

/// Returns the v1-nest-relative form of an absolute v1 path template.
///
/// The v1 router is mounted under `/api/v1`, so [`Router::route`] expects
/// paths that start with `/...` (the portion after `/api/v1`). This helper
/// strips the full prefix from a canonical absolute template such as
/// [`LIST_RUNS`].
///
/// # Panics
///
/// Panics if `absolute` does not begin with `/api/v1`.
pub fn route_template(absolute: &str) -> &str {
    let rest = absolute
        .strip_prefix(V1_PREFIX)
        .expect("v1 path templates must begin with `/api/v1`");
    if rest.is_empty() { "/" } else { rest }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    /// All public path templates exposed by this module.
    ///
    /// Used by drift-detection tests to ensure every template is well-formed.
    fn all_templates() -> &'static [&'static str] {
        &[
            SWAGGER_UI,
            OPENAPI_JSON,
            SERVER_INFO,
            LIST_RUNS,
            GET_RUN,
            CANCEL_RUN,
            GET_RUN_OUTPUTS,
            LOCALIZE_RUN,
            LIST_RUN_TASKS,
            RUN_TASK_COUNTS,
            LIST_SESSIONS,
            GET_SESSION,
            LIST_TASKS,
            GET_TASK,
            GET_TASK_LOGS,
        ]
    }

    #[test]
    fn submit_aliases_list_runs() {
        assert_eq!(SUBMIT_RUN, LIST_RUNS);
    }

    #[test]
    fn templates_share_the_api_prefix() {
        for template in all_templates() {
            assert!(
                template.starts_with(V1_PREFIX),
                "template `{template}` must begin with `{V1_PREFIX}`"
            );
        }
    }

    #[test]
    fn route_template_strips_v1_prefix() {
        assert_eq!(route_template(LIST_RUNS), "/runs");
        assert_eq!(route_template(GET_RUN), "/runs/{id}");
        assert_eq!(route_template(RUN_TASK_COUNTS), "/runs/{id}/tasks/counts");
        assert_eq!(route_template(GET_TASK_LOGS), "/tasks/{name}/logs");
    }

    #[test]
    #[should_panic(expected = "must begin with `/api/v1`")]
    fn route_template_panics_on_bad_prefix() {
        let _ = route_template("/v1/runs");
    }

    #[test]
    fn id_builders_substitute_correctly() {
        let id = Uuid::nil();
        let id_str = id.to_string();
        assert_eq!(get_run(id), format!("/api/v1/runs/{id_str}"));
        assert_eq!(cancel_run(id), format!("/api/v1/runs/{id_str}/cancel"));
        assert_eq!(get_run_outputs(id), format!("/api/v1/runs/{id_str}/outputs"));
        assert_eq!(localize_run(id), format!("/api/v1/runs/{id_str}/localize"));
        assert_eq!(list_run_tasks(id), format!("/api/v1/runs/{id_str}/tasks"));
        assert_eq!(
            run_task_counts(id),
            format!("/api/v1/runs/{id_str}/tasks/counts")
        );
        assert_eq!(get_session(id), format!("/api/v1/sessions/{id_str}"));
    }

    #[test]
    fn name_builders_substitute_correctly() {
        assert_eq!(get_task("my_task"), "/api/v1/tasks/my_task");
        assert_eq!(get_task_logs("my_task"), "/api/v1/tasks/my_task/logs");
    }

    #[test]
    fn builders_match_their_templates() {
        // Each builder must produce the same string as the corresponding
        // template with its placeholder substituted by `Display` of the input.
        let id = Uuid::new_v4();
        let id_str = id.to_string();
        let cases: &[(&str, String)] = &[
            (GET_RUN, get_run(id)),
            (CANCEL_RUN, cancel_run(id)),
            (GET_RUN_OUTPUTS, get_run_outputs(id)),
            (LOCALIZE_RUN, localize_run(id)),
            (LIST_RUN_TASKS, list_run_tasks(id)),
            (RUN_TASK_COUNTS, run_task_counts(id)),
            (GET_SESSION, get_session(id)),
        ];
        for (template, built) in cases {
            assert_eq!(*built, template.replace("{id}", &id_str));
        }

        let name = "abc";
        let name_cases: &[(&str, String)] = &[
            (GET_TASK, get_task(name)),
            (GET_TASK_LOGS, get_task_logs(name)),
        ];
        for (template, built) in name_cases {
            assert_eq!(*built, template.replace("{name}", name));
        }
    }
}
