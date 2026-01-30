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
    /// Number of results to skip (default: `0`).
    #[serde(default)]
    pub offset: Option<i64>,
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
    /// Number of results to skip (default: `0`).
    #[serde(default)]
    pub offset: Option<i64>,
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
}

impl From<commands::ListTasksResponse> for ListTasksResponse {
    fn from(response: commands::ListTasksResponse) -> Self {
        Self {
            tasks: response.tasks.into_iter().map(Into::into).collect(),
            total: response.total,
        }
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
}

impl From<commands::ListTaskLogsResponse> for ListTaskLogsResponse {
    fn from(response: commands::ListTaskLogsResponse) -> Self {
        Self {
            logs: response.logs.into_iter().map(Into::into).collect(),
            total: response.total,
        }
    }
}

/// List all tasks with optional filtering.
#[utoipa::path(
    get,
    path = "/api/v1/tasks",
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

    let response = send_command(&state.run_manager_tx, |rx| RunManagerCmd::ListTasks {
        run_id: query.run_uuid,
        status: query.status,
        limit: query.limit,
        offset: query.offset,
        rx,
    })
    .await?;

    Ok(Json(response.into()))
}

/// Get a specific task by name.
#[utoipa::path(
    get,
    path = "/api/v1/tasks/{name}",
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
    path = "/api/v1/tasks/{name}/logs",
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

    let response = send_command(&state.run_manager_tx, |rx| RunManagerCmd::GetTaskLogs {
        name,
        stream: query.source,
        limit: query.limit,
        offset: query.offset,
        rx,
    })
    .await?;

    Ok(Json(response.into()))
}
