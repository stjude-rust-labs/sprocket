//! Manager command types and responses.

use anyhow::Result;
use serde_json::Value;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::system::v1::db::DatabaseError;
use crate::system::v1::db::LogSource;
use crate::system::v1::db::Run;
use crate::system::v1::db::RunStatus;
use crate::system::v1::db::Session;
use crate::system::v1::db::Task;
use crate::system::v1::db::TaskLog;
use crate::system::v1::db::TaskStatus;

/// Response for run submission.
#[derive(Debug)]
pub struct SubmitResponse {
    /// The run ID.
    pub id: Uuid,
    /// The generated run name.
    pub name: String,
    /// Events for this run execution.
    pub events: wdl::engine::Events,
    /// Join handle for the run execution task.
    pub handle: JoinHandle<()>,
}

/// Response for run status query.
#[derive(Debug)]
pub struct RunResponse {
    /// The run data.
    pub run: Run,
}

/// Response for run list query.
#[derive(Debug)]
pub struct ListRunsResponse {
    /// The runs.
    pub runs: Vec<Run>,
    /// Total count before pagination.
    pub total: i64,
}

/// Response for run cancellation.
#[derive(Debug)]
pub struct CancelRunResponse {
    /// The run ID.
    pub id: Uuid,
}

/// Response for run outputs query.
#[derive(Debug)]
pub struct RunOutputsResponse {
    /// The run outputs as JSON.
    pub outputs: Option<Value>,
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

/// Response for task list query.
#[derive(Debug)]
pub struct ListTasksResponse {
    /// The tasks.
    pub tasks: Vec<Task>,
    /// Total count before pagination.
    pub total: i64,
}

/// Response for task query.
#[derive(Debug)]
pub struct GetTaskResponse {
    /// The task.
    pub task: Task,
}

/// Response for task logs query.
#[derive(Debug)]
pub struct ListTaskLogsResponse {
    /// The task logs.
    pub logs: Vec<TaskLog>,
    /// Total count before pagination.
    pub total: i64,
}

/// Commands sent to the run manager.
#[derive(Debug)]
pub enum RunManagerCmd {
    /// Ping the manager to check if it's ready.
    Ping {
        /// Channel to send the response back.
        rx: oneshot::Sender<Result<()>>,
    },

    /// Submit a new run for execution.
    Submit {
        /// WDL source path (local file path or HTTP/HTTPS URL).
        source: String,
        /// Run inputs as JSON.
        inputs: Value,
        /// Optional target workflow or task name to execute.
        target: Option<String>,
        /// Optional output directory to index on.
        index_on: Option<String>,
        /// Channel to send the response back.
        rx: oneshot::Sender<Result<SubmitResponse, super::SubmitRunError>>,
    },

    /// Get run status by ID.
    GetStatus {
        /// Run ID.
        id: Uuid,
        /// Channel to send the response back.
        rx: oneshot::Sender<Result<RunResponse, super::GetRunError>>,
    },

    /// List runs with optional filtering.
    List {
        /// Filter by status.
        status: Option<RunStatus>,
        /// Number of results to return.
        limit: Option<i64>,
        /// Number of results to skip.
        offset: Option<i64>,
        /// Channel to send the response back.
        rx: oneshot::Sender<Result<ListRunsResponse, DatabaseError>>,
    },

    /// Cancel a running run.
    Cancel {
        /// Run ID to cancel.
        id: Uuid,
        /// Channel to send the response back.
        rx: oneshot::Sender<Result<CancelRunResponse, super::CancelRunError>>,
    },

    /// Get run outputs.
    GetOutputs {
        /// Run ID.
        id: Uuid,
        /// Channel to send the response back.
        rx: oneshot::Sender<Result<RunOutputsResponse, super::GetRunOutputsError>>,
    },

    /// Get session by ID.
    GetSession {
        /// Session ID.
        id: Uuid,
        /// Channel to send the response back.
        rx: oneshot::Sender<Result<SessionResponse, super::GetSessionError>>,
    },

    /// List sessions.
    ListSessions {
        /// Number of results to return.
        limit: Option<i64>,
        /// Number of results to skip.
        offset: Option<i64>,
        /// Channel to send the response back.
        rx: oneshot::Sender<Result<ListSessionsResponse, DatabaseError>>,
    },

    /// List tasks with optional filtering.
    ListTasks {
        /// Filter by run ID.
        run_id: Option<Uuid>,
        /// Filter by status.
        status: Option<TaskStatus>,
        /// Number of results to return.
        limit: Option<i64>,
        /// Number of results to skip.
        offset: Option<i64>,
        /// Channel to send the response back.
        rx: oneshot::Sender<Result<ListTasksResponse, DatabaseError>>,
    },

    /// Get task by name.
    GetTask {
        /// Task name.
        name: String,
        /// Channel to send the response back.
        rx: oneshot::Sender<Result<GetTaskResponse, DatabaseError>>,
    },

    /// Get task logs.
    GetTaskLogs {
        /// Task name.
        name: String,
        /// Filter by stream.
        stream: Option<LogSource>,
        /// Number of results to return.
        limit: Option<i64>,
        /// Number of results to skip.
        offset: Option<i64>,
        /// Channel to send the response back.
        rx: oneshot::Sender<Result<ListTaskLogsResponse, DatabaseError>>,
    },

    /// Shutdown the manager gracefully.
    Shutdown {
        /// Channel to send the response back.
        rx: oneshot::Sender<Result<()>>,
    },
}
