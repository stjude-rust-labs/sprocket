//! V1 API routes and models.

use axum::Router;
use axum::routing::get;
use axum::routing::post;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::error;
use tracing::warn;
use utoipa::OpenApi;

use self::error::Error;
use self::info::*;
use self::runs::*;
use self::sessions::*;
use self::tasks::*;
use super::AppState;
use crate::system::v1::exec::svc::run_manager::RunManagerCmd;

pub mod error;
pub mod info;
pub mod paths;
pub mod runs;
pub mod sessions;
pub mod tasks;

// Re-export enum types for OpenAPI schema.
pub use crate::system::v1::db::LogSource;
pub use crate::system::v1::db::RunStatus;
pub use crate::system::v1::db::SprocketCommand;
pub use crate::system::v1::db::TaskStatus;

/// OpenAPI documentation for V1 API.
#[derive(OpenApi)]
#[openapi(
    paths(
        submit_run,
        get_run,
        list_runs,
        cancel_run,
        get_run_outputs,
        list_sessions,
        get_session,
        list_tasks,
        list_run_tasks,
        get_run_task_counts,
        get_task,
        get_task_logs,
        get_server_info,
    ),
    components(schemas(
        CancelRunResponse,
        GetTaskResponse,
        ListRunsQueryParams,
        ListRunsResponse,
        ListSessionsQueryParams,
        ListSessionsResponse,
        ListTaskLogsQueryParams,
        ListTaskLogsResponse,
        ListRunTasksQueryParams,
        ListTasksQueryParams,
        ListTasksResponse,
        LogSource,
        Run,
        RunOutputsResponse,
        RunResponse,
        RunStatus,
        RunTaskCountsResponse,
        ServerFailureMode,
        ServerInfoResponse,
        Session,
        SessionResponse,
        SprocketCommand,
        SubmitResponse,
        SubmitRunRequest,
        Task,
        TaskLog,
        TaskStatus,
    )),
    tags(
        (name = "runs", description = "Run management endpoints"),
        (name = "tasks", description = "Task management endpoints"),
        (name = "sessions", description = "Session management endpoints"),
        (name = "server", description = "Server metadata endpoints")
    )
)]
pub struct ApiDoc;

/// Create the V1 API router.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route(
            paths::route_template(paths::LIST_RUNS),
            post(submit_run).get(list_runs),
        )
        .route(paths::route_template(paths::GET_RUN), get(get_run))
        .route(
            paths::route_template(paths::CANCEL_RUN),
            post(cancel_run),
        )
        .route(
            paths::route_template(paths::GET_RUN_OUTPUTS),
            get(get_run_outputs),
        )
        .route(
            paths::route_template(paths::LIST_RUN_TASKS),
            get(list_run_tasks),
        )
        .route(
            paths::route_template(paths::RUN_TASK_COUNTS),
            get(get_run_task_counts),
        )
        .route(
            paths::route_template(paths::LIST_SESSIONS),
            get(list_sessions),
        )
        .route(
            paths::route_template(paths::GET_SESSION),
            get(get_session),
        )
        .route(paths::route_template(paths::LIST_TASKS), get(list_tasks))
        .route(paths::route_template(paths::GET_TASK), get(get_task))
        .route(
            paths::route_template(paths::GET_TASK_LOGS),
            get(get_task_logs),
        )
        .route(
            paths::route_template(paths::SERVER_INFO),
            get(get_server_info),
        )
        .with_state(state)
}

/// Sends a command to the manager and receives the response.
///
/// This helper method is around to keep things DRY.
pub async fn send_command<T, E>(
    manager: &mpsc::Sender<RunManagerCmd>,
    build_command: impl FnOnce(oneshot::Sender<Result<T, E>>) -> RunManagerCmd,
) -> Result<T, Error>
where
    Error: From<E>,
    E: std::error::Error,
{
    let (tx, rx) = oneshot::channel();

    manager.send(build_command(tx)).await.map_err(|e| {
        error!("failed to send command to manager: {}", e);
        Error::Internal
    })?;

    match rx.await {
        Err(e) => {
            error!("manager dropped response channel: {:#}", e);
            Err(Error::Internal)
        }
        Ok(Err(e)) => {
            warn!("manager rejected command: {:#}", e);
            Err(Error::from(e))
        }
        Ok(Ok(response)) => Ok(response),
    }
}

/// The default page size used for paginated list endpoints when the caller does
/// not specify a `limit`.
pub const DEFAULT_PAGE_SIZE: i64 = 100;

/// Validates the `limit` and `next_token` query parameters for a paginated list
/// endpoint and returns the `(limit, offset)` pair that should be forwarded to
/// the run manager / database.
///
/// `limit` defaults to [`DEFAULT_PAGE_SIZE`] when unspecified and must be
/// positive. `next_token` is parsed as a non-negative integer offset; a
/// missing token is treated as offset `0`.
///
/// Returns a `400 BadRequest` error if either value is invalid. Centralizing
/// this validation prevents pathological values (e.g. SQLite's interpretation
/// of `LIMIT -1` as unbounded, or `limit = 0` producing a repeated pagination
/// token) from reaching the database layer.
pub fn validate_pagination(
    limit: Option<i64>,
    next_token: Option<&str>,
) -> Result<(i64, i64), Error> {
    let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE);
    if limit <= 0 {
        return Err(Error::BadRequest("`limit` must be positive".to_string()));
    }

    let offset = match next_token {
        Some(t) => {
            let parsed = t
                .parse::<i64>()
                .map_err(|_| Error::BadRequest(format!("invalid `next_token`: `{}`", t)))?;
            if parsed < 0 {
                return Err(Error::BadRequest(
                    "`next_token` must be non-negative".to_string(),
                ));
            }
            parsed
        }
        None => 0,
    };

    Ok((limit, offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asserts that a [`validate_pagination`] result is a `BadRequest` whose
    /// message contains the given substring.
    fn assert_bad_request(result: Result<(i64, i64), Error>, contains: &str) {
        match result {
            Err(Error::BadRequest(msg)) => assert!(
                msg.contains(contains),
                "expected message to contain `{contains}`, got `{msg}`"
            ),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn defaults_when_unspecified() {
        let (limit, offset) = validate_pagination(None, None).unwrap();
        assert_eq!(limit, DEFAULT_PAGE_SIZE);
        assert_eq!(offset, 0);
    }

    #[test]
    fn accepts_positive_limit_and_non_negative_token() {
        let (limit, offset) = validate_pagination(Some(50), Some("0")).unwrap();
        assert_eq!(limit, 50);
        assert_eq!(offset, 0);

        let (limit, offset) = validate_pagination(Some(1), Some("250")).unwrap();
        assert_eq!(limit, 1);
        assert_eq!(offset, 250);
    }

    #[test]
    fn rejects_zero_limit() {
        assert_bad_request(
            validate_pagination(Some(0), None),
            "`limit` must be positive",
        );
    }

    #[test]
    fn rejects_negative_limit() {
        assert_bad_request(
            validate_pagination(Some(-1), None),
            "`limit` must be positive",
        );
    }

    #[test]
    fn rejects_negative_next_token() {
        assert_bad_request(
            validate_pagination(None, Some("-5")),
            "`next_token` must be non-negative",
        );
    }

    #[test]
    fn rejects_unparseable_next_token() {
        assert_bad_request(
            validate_pagination(None, Some("nope")),
            "invalid `next_token`",
        );
        assert_bad_request(validate_pagination(None, Some("")), "invalid `next_token`");
    }
}
