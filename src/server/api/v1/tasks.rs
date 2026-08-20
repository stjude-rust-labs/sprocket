//! Task API handlers.

use axum::Json;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::extract::rejection::QueryRejection;
use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use utoipa::IntoParams;
use utoipa::ToSchema;
use uuid::Uuid;

use super::AppState;
use super::LogSource;
use super::TaskStatus;
use super::error::Error;
use super::send_command;
use crate::system::v1::exec::svc::RunManagerCmd;
use crate::system::v1::exec::svc::run_manager::commands;

/// Query parameters for listing tasks.
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct ListTasksQueryParams {
    /// Filter by run UUID.
    #[serde(default)]
    pub run_uuid: Option<Uuid>,
    /// Filter by status.
    #[serde(default)]
    pub status: Option<TaskStatus>,
    /// Number of results to return (default: `100`).
    #[serde(default)]
    pub limit: Option<i64>,
    /// Token for pagination. It is expected that clients pass the value from a
    /// previous response to retrieve the next page.
    #[serde(default)]
    pub next_token: Option<String>,
}

/// Query parameters for listing the tasks of a specific run.
///
/// The run is identified by the path, so no `run_uuid` filter is accepted here.
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct ListRunTasksQueryParams {
    /// Filter by status.
    #[serde(default)]
    pub status: Option<TaskStatus>,
    /// Number of results to return (default: `100`).
    #[serde(default)]
    pub limit: Option<i64>,
    /// Token for pagination. It is expected that clients pass the value from a
    /// previous response to retrieve the next page.
    #[serde(default)]
    pub next_token: Option<String>,
}

/// Query parameters for listing task logs.
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct ListTaskLogsQueryParams {
    /// Filter by log source (stdout or stderr).
    #[serde(default)]
    pub source: Option<LogSource>,
    /// Number of results to return (default: `100`).
    #[serde(default)]
    pub limit: Option<i64>,
    /// Token for pagination. It is expected that clients pass the value from a
    /// previous response to retrieve the next page.
    #[serde(default)]
    pub next_token: Option<String>,
}

/// Task data for API responses.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Task {
    /// Task name from WDL.
    pub name: String,
    /// UUID of the run managing this task.
    pub run_uuid: Uuid,
    /// Current task status.
    pub status: TaskStatus,
    /// Exit status from task completion.
    pub exit_status: Option<i32>,
    /// Error message if task failed.
    pub error: Option<String>,
    /// Timestamp when task was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when task started executing.
    pub started_at: Option<DateTime<Utc>>,
    /// Timestamp when task reached terminal state.
    pub completed_at: Option<DateTime<Utc>>,
}

impl From<crate::system::v1::db::Task> for Task {
    fn from(task: crate::system::v1::db::Task) -> Self {
        Self {
            name: task.name,
            run_uuid: task.run_uuid,
            status: task.status,
            exit_status: task.exit_status,
            error: task.error,
            created_at: task.created_at,
            started_at: task.started_at,
            completed_at: task.completed_at,
        }
    }
}

/// The response for a "list tasks" query.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListTasksResponse {
    /// The tasks.
    pub tasks: Vec<Task>,
    /// Total count before pagination.
    pub total: i64,
    /// Next token for pagination. Pass this value as `next_token` in the next
    /// request to retrieve the next page of results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_token: Option<String>,
}

/// The response for a run's per-status task counts.
///
/// Every status is always present; statuses with no tasks report `0`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RunTaskCountsResponse {
    /// Number of tasks whose inputs, command, and requirements are being
    /// evaluated.
    pub initializing: i64,
    /// Number of tasks that are transferring their inputs.
    pub localizing: i64,
    /// Number of tasks that have been submitted to a backend but not yet
    /// started.
    pub pending: i64,
    /// Number of tasks that are currently executing.
    pub running: i64,
    /// Number of tasks that completed successfully.
    pub completed: i64,
    /// Number of tasks whose result was reused from the call cache.
    pub cached: i64,
    /// Number of tasks that failed.
    pub failed: i64,
    /// Number of tasks that were canceled.
    pub canceled: i64,
    /// Number of tasks that were preempted.
    pub preempted: i64,
    /// Number of tasks orphaned when the server that owned their run stopped
    /// reporting.
    pub orphaned: i64,
    /// Total number of tasks across all statuses.
    pub total: i64,
}

impl From<commands::RunTaskCountsResponse> for RunTaskCountsResponse {
    fn from(response: commands::RunTaskCountsResponse) -> Self {
        let mut counts = Self {
            initializing: 0,
            localizing: 0,
            pending: 0,
            running: 0,
            completed: 0,
            cached: 0,
            failed: 0,
            canceled: 0,
            preempted: 0,
            orphaned: 0,
            total: 0,
        };

        for (status, count) in response.counts {
            match status {
                TaskStatus::Initializing => counts.initializing = count,
                TaskStatus::Localizing => counts.localizing = count,
                TaskStatus::Pending => counts.pending = count,
                TaskStatus::Running => counts.running = count,
                TaskStatus::Completed => counts.completed = count,
                TaskStatus::Cached => counts.cached = count,
                TaskStatus::Failed => counts.failed = count,
                TaskStatus::Canceled => counts.canceled = count,
                TaskStatus::Preempted => counts.preempted = count,
                TaskStatus::Orphaned => counts.orphaned = count,
            }
            counts.total += count;
        }

        counts
    }
}

/// The response for a "get task" query.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GetTaskResponse {
    /// The task.
    #[serde(flatten)]
    pub task: Task,
}

impl From<commands::GetTaskResponse> for GetTaskResponse {
    fn from(response: commands::GetTaskResponse) -> Self {
        Self {
            task: response.task.into(),
        }
    }
}

/// Task log data for API responses.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TaskLog {
    /// Auto-increment ID.
    pub id: i64,
    /// Foreign key: task name.
    pub task_name: String,
    /// Log source.
    pub source: LogSource,
    /// Raw log data chunk.
    pub chunk: Box<[u8]>,
    /// Timestamp when log was received.
    pub created_at: DateTime<Utc>,
}

impl From<crate::system::v1::db::TaskLog> for TaskLog {
    fn from(log: crate::system::v1::db::TaskLog) -> Self {
        Self {
            id: log.id,
            task_name: log.task_name,
            source: log.source,
            chunk: log.chunk,
            created_at: log.created_at,
        }
    }
}

/// The response for a "list task logs" query.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListTaskLogsResponse {
    /// The task logs.
    pub logs: Vec<TaskLog>,
    /// Total count before pagination.
    pub total: i64,
    /// Next token for pagination. Pass this value as `next_token` in the next
    /// request to retrieve the next page of results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_token: Option<String>,
}

/// List all tasks with optional filtering.
#[utoipa::path(
    get,
    path = super::paths::LIST_TASKS,
    params(ListTasksQueryParams),
    responses(
        (status = 200, description = "Tasks retrieved", body = ListTasksResponse),
    ),
    tag = "tasks"
)]
pub async fn list_tasks(
    State(state): State<AppState>,
    query: Result<Query<ListTasksQueryParams>, QueryRejection>,
) -> Result<Json<ListTasksResponse>, Error> {
    let Query(query) = query.map_err(|rejection| match rejection {
        QueryRejection::FailedToDeserializeQueryString(err) => {
            Error::BadRequest(format!("invalid query parameters: {}", err))
        }
        _ => Error::BadRequest("invalid query parameters".to_string()),
    })?;

    let (limit, offset) = super::validate_pagination(query.limit, query.next_token.as_deref())?;

    let response = send_command(&state.run_manager_tx, |rx| RunManagerCmd::ListTasks {
        run_id: query.run_uuid,
        status: query.status,
        limit: Some(limit),
        offset: Some(offset),
        rx,
    })
    .await?;

    let next_offset = offset + limit;
    let next_token = if next_offset < response.total {
        Some(next_offset.to_string())
    } else {
        None
    };

    Ok(Json(ListTasksResponse {
        tasks: response.tasks.into_iter().map(Into::into).collect(),
        total: response.total,
        next_token,
    }))
}

/// List all tasks for a specific run.
#[utoipa::path(
    get,
    path = super::paths::LIST_RUN_TASKS,
    params(
        ("id" = String, Path, description = "Run ID"),
        ListRunTasksQueryParams
    ),
    responses(
        (status = 200, description = "Tasks retrieved", body = ListTasksResponse),
    ),
    tag = "tasks"
)]
pub async fn list_run_tasks(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    query: Result<Query<ListRunTasksQueryParams>, QueryRejection>,
) -> Result<Json<ListTasksResponse>, Error> {
    let Query(query) = query.map_err(|rejection| match rejection {
        QueryRejection::FailedToDeserializeQueryString(err) => {
            Error::BadRequest(format!("invalid query parameters: {}", err))
        }
        _ => Error::BadRequest("invalid query parameters".to_string()),
    })?;

    let (limit, offset) = super::validate_pagination(query.limit, query.next_token.as_deref())?;

    let response = send_command(&state.run_manager_tx, |rx| RunManagerCmd::ListTasks {
        run_id: Some(id),
        status: query.status,
        limit: Some(limit),
        offset: Some(offset),
        rx,
    })
    .await?;

    let next_offset = offset + limit;
    let next_token = if next_offset < response.total {
        Some(next_offset.to_string())
    } else {
        None
    };

    Ok(Json(ListTasksResponse {
        tasks: response.tasks.into_iter().map(Into::into).collect(),
        total: response.total,
        next_token,
    }))
}

/// Get the per-status task counts for a specific run.
///
/// Every status is always present in the response; statuses with no tasks
/// report `0`. Unknown runs report all-zero counts (no error).
#[utoipa::path(
    get,
    path = super::paths::RUN_TASK_COUNTS,
    params(
        ("id" = String, Path, description = "Run ID")
    ),
    responses(
        (status = 200, description = "Task counts retrieved", body = RunTaskCountsResponse),
    ),
    tag = "tasks"
)]
pub async fn get_run_task_counts(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<RunTaskCountsResponse>, Error> {
    let response = send_command(&state.run_manager_tx, |rx| {
        RunManagerCmd::CountRunTasksByStatus { run_id: id, rx }
    })
    .await?;

    Ok(Json(response.into()))
}

/// Get a specific task by name.
#[utoipa::path(
    get,
    path = super::paths::GET_TASK,
    params(
        ("name" = String, Path, description = "Task name")
    ),
    responses(
        (status = 200, description = "Task found", body = GetTaskResponse),
        (status = 404, description = "Task not found"),
    ),
    tag = "tasks"
)]
pub async fn get_task(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<GetTaskResponse>, Error> {
    let response = send_command(&state.run_manager_tx, |rx| RunManagerCmd::GetTask {
        name,
        rx,
    })
    .await?;

    Ok(Json(response.into()))
}

/// Get logs for a specific task.
#[utoipa::path(
    get,
    path = super::paths::GET_TASK_LOGS,
    params(
        ("name" = String, Path, description = "Task name"),
        ListTaskLogsQueryParams
    ),
    responses(
        (status = 200, description = "Task logs retrieved", body = ListTaskLogsResponse),
        (status = 404, description = "Task not found"),
    ),
    tag = "tasks"
)]
pub async fn get_task_logs(
    State(state): State<AppState>,
    Path(name): Path<String>,
    query: Result<Query<ListTaskLogsQueryParams>, QueryRejection>,
) -> Result<Json<ListTaskLogsResponse>, Error> {
    let Query(query) = query.map_err(|rejection| match rejection {
        QueryRejection::FailedToDeserializeQueryString(err) => {
            Error::BadRequest(format!("invalid query parameters: {}", err))
        }
        _ => Error::BadRequest("invalid query parameters".to_string()),
    })?;

    let (limit, offset) = super::validate_pagination(query.limit, query.next_token.as_deref())?;

    let response = send_command(&state.run_manager_tx, |rx| RunManagerCmd::GetTaskLogs {
        name,
        stream: query.source,
        limit: Some(limit),
        offset: Some(offset),
        rx,
    })
    .await?;

    let next_offset = offset + limit;
    let next_token = if next_offset < response.total {
        Some(next_offset.to_string())
    } else {
        None
    };

    Ok(Json(ListTaskLogsResponse {
        logs: response.logs.into_iter().map(Into::into).collect(),
        total: response.total,
        next_token,
    }))
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::*;
    use crate::server::ServerFailureMode;

    fn app_state() -> AppState {
        let (run_manager_tx, _run_manager_rx) = mpsc::channel(1);
        AppState::builder()
            .run_manager_tx(run_manager_tx)
            .failure_mode(ServerFailureMode::Slow)
            .output_dir(String::new())
            .build()
    }

    fn db_task() -> crate::system::v1::db::Task {
        let now = Utc::now();
        crate::system::v1::db::Task {
            name: "task-name".to_string(),
            run_uuid: Uuid::nil(),
            status: TaskStatus::Completed,
            exit_status: Some(0),
            error: None,
            created_at: now,
            started_at: Some(now),
            completed_at: Some(now),
        }
    }

    #[tokio::test]
    async fn list_tasks_rejects_invalid_next_token() {
        let query = ListTasksQueryParams {
            run_uuid: None,
            status: None,
            limit: None,
            next_token: Some("bad-token".to_string()),
        };

        let error = list_tasks(State(app_state()), Ok(Query(query)))
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::BadRequest(message) if message == "invalid `next_token`: `bad-token`")
        );
    }

    #[tokio::test]
    async fn get_task_logs_rejects_invalid_next_token() {
        let query = ListTaskLogsQueryParams {
            source: None,
            limit: None,
            next_token: Some("bad-token".to_string()),
        };

        let error = get_task_logs(
            State(app_state()),
            Path("task-name".to_string()),
            Ok(Query(query)),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, Error::BadRequest(message) if message == "invalid `next_token`: `bad-token`")
        );
    }

    #[test]
    fn task_response_conversions_preserve_fields() {
        let db_task = db_task();
        let response = GetTaskResponse::from(commands::GetTaskResponse {
            task: db_task.clone(),
        });

        assert_eq!(response.task.name, db_task.name);
        assert_eq!(response.task.run_uuid, db_task.run_uuid);
        assert_eq!(response.task.status, db_task.status);
        assert_eq!(response.task.exit_status, db_task.exit_status);
        assert_eq!(response.task.error, db_task.error);
        assert_eq!(response.task.created_at, db_task.created_at);
        assert_eq!(response.task.started_at, db_task.started_at);
        assert_eq!(response.task.completed_at, db_task.completed_at);

        let log = crate::system::v1::db::TaskLog {
            id: 7,
            task_name: "task-name".to_string(),
            source: LogSource::Stdout,
            chunk: Box::from(*b"hello"),
            created_at: Utc::now(),
        };
        let converted = TaskLog::from(log.clone());
        assert_eq!(converted.id, log.id);
        assert_eq!(converted.task_name, log.task_name);
        assert_eq!(converted.source, log.source);
        assert_eq!(converted.chunk, log.chunk);
        assert_eq!(converted.created_at, log.created_at);
    }
}
