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
use self::runs::*;
use self::sessions::*;
use self::tasks::*;
use crate::system::v1::exec::svc::run_manager::RunManagerCmd;

use super::AppState;

pub mod error;
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
        get_task,
        get_task_logs,
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
        ListTasksQueryParams,
        ListTasksResponse,
        LogSource,
        Run,
        RunOutputsResponse,
        RunResponse,
        RunStatus,
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
        (name = "sessions", description = "Session management endpoints")
    )
)]
pub struct ApiDoc;

/// Create the V1 API router.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/runs", post(submit_run).get(list_runs))
        .route("/runs/{id}", get(get_run))
        .route("/runs/{id}/cancel", post(cancel_run))
        .route("/runs/{id}/outputs", get(get_run_outputs))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{id}", get(get_session))
        .route("/tasks", get(list_tasks))
        .route("/tasks/{name}", get(get_task))
        .route("/tasks/{name}/logs", get(get_task_logs))
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
