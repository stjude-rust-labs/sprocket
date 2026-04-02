//! Read-only database queries for the execution subsystem.

use std::sync::Arc;

use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::system::v1::db::Database;
use crate::system::v1::db::DatabaseError;
use crate::system::v1::db::LogSource;
use crate::system::v1::db::Run;
use crate::system::v1::db::RunStatus;
use crate::system::v1::db::Session;
use crate::system::v1::db::Task;
use crate::system::v1::db::TaskLog;
use crate::system::v1::db::TaskStatus;

/// Response for run status query.
#[derive(Debug)]
pub struct RunResponse {
    /// The run data.
    pub run: Run,
}

/// Error type for getting a run.
#[derive(Debug, Error)]
pub enum GetRunError {
    /// Database error.
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// Run not found.
    #[error("run not found: `{0}`")]
    NotFound(Uuid),
}

/// Gets a run by ID.
pub(crate) async fn get_run(db: &Arc<dyn Database>, id: Uuid) -> Result<RunResponse, GetRunError> {
    let run = db.get_run(id).await?;
    match run {
        Some(run) => Ok(RunResponse { run }),
        None => Err(GetRunError::NotFound(id)),
    }
}

/// Response for run list query.
#[derive(Debug)]
pub struct ListRunsResponse {
    /// The runs.
    pub runs: Vec<Run>,
    /// Total count before pagination.
    pub total: i64,
}

pub(crate) async fn list_runs(
    db: &Arc<dyn Database>,
    status: Option<RunStatus>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<ListRunsResponse, DatabaseError> {
    let runs = db.list_runs(status, limit, offset).await?;
    let total = db.count_runs(status).await?;
    Ok(ListRunsResponse { runs, total })
}

/// Response for run outputs query.
#[derive(Debug)]
pub struct RunOutputsResponse {
    /// The run outputs as JSON.
    pub outputs: Option<Value>,
}

/// Error type for getting run outputs.
#[derive(Debug, Error)]
pub enum GetRunOutputsError {
    /// Database error.
    #[error("database error: {0}")]
    Database(#[from] crate::system::v1::db::DatabaseError),
    /// Run not found.
    #[error("the run with id `{0}` was not found")]
    NotFound(Uuid),
}

/// Attempts to get the outputs for a run.
pub(crate) async fn get_run_outputs(
    db: &Arc<dyn Database>,
    id: Uuid,
) -> Result<RunOutputsResponse, GetRunOutputsError> {
    let run = db
        .get_run(id)
        .await?
        .ok_or(GetRunOutputsError::NotFound(id))?;

    let outputs = run
        .outputs
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok());

    Ok(RunOutputsResponse { outputs })
}

/// Response for session query.
#[derive(Debug)]
pub struct SessionResponse {
    /// The session data.
    pub session: Session,
}

/// Response for session list query.
#[derive(Debug)]
pub struct ListSessionsResponse {
    /// The sessions.
    pub sessions: Vec<Session>,
    /// Total count before pagination.
    pub total: i64,
}

/// Gets all sessions given the filter criteria.
pub(crate) async fn list_sessions(
    db: &Arc<dyn Database>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<ListSessionsResponse, DatabaseError> {
    let sessions = db.list_sessions(limit, offset).await?;
    let total = db.count_sessions().await?;
    Ok(ListSessionsResponse { sessions, total })
}

/// Error type for getting an session.
#[derive(Debug, Error)]
pub enum GetSessionError {
    /// Database error.
    #[error("database error: {0}")]
    Database(#[from] crate::system::v1::db::DatabaseError),
    /// Session not found.
    #[error("the run with id `{0}` was not found")]
    NotFound(Uuid),
}

/// Gets the session entry associated with a run.
pub(crate) async fn get_session_for_run(
    db: &Arc<dyn Database>,
    id: Uuid,
) -> Result<SessionResponse, GetSessionError> {
    let session = db
        .get_session(id)
        .await?
        .ok_or(GetSessionError::NotFound(id))?;

    Ok(SessionResponse { session })
}

/// Response for task list query.
#[derive(Debug)]
pub struct ListTasksResponse {
    /// The tasks.
    pub tasks: Vec<Task>,
    /// Total count before pagination.
    pub total: i64,
}

/// Gets all tasks given the filter criteria.
pub(crate) async fn list_tasks(
    db: &Arc<dyn Database>,
    run_id: Option<Uuid>,
    status: Option<TaskStatus>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<ListTasksResponse, DatabaseError> {
    let tasks = db.list_tasks(run_id, status, limit, offset).await?;
    let total = db.count_tasks(run_id, status).await?;
    Ok(ListTasksResponse { tasks, total })
}

/// Response for task query.
#[derive(Debug)]
pub struct GetTaskResponse {
    /// The task.
    pub task: Task,
}

/// Gets a task with a given name.
pub(crate) async fn get_task(
    db: &Arc<dyn Database>,
    name: String,
) -> Result<GetTaskResponse, DatabaseError> {
    let task = db.get_task(&name).await?;
    Ok(GetTaskResponse { task })
}

/// Response for task logs query.
#[derive(Debug)]
pub struct ListTaskLogsResponse {
    /// The task logs.
    pub logs: Vec<TaskLog>,
    /// Total count before pagination.
    pub total: i64,
}

/// Gets the logs for a task with a name given the filter criteria.
pub(crate) async fn get_task_logs(
    db: &Arc<dyn Database>,
    name: String,
    stream: Option<LogSource>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<ListTaskLogsResponse, DatabaseError> {
    let logs = db.get_task_logs(&name, stream, limit, offset).await?;
    let total = db.count_task_logs(&name, stream).await?;
    Ok(ListTaskLogsResponse { logs, total })
}
