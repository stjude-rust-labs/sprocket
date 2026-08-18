//! Database schema and operations for provenance tracking in v1.

use async_trait::async_trait;
use chrono::DateTime;
use chrono::Utc;
use thiserror::Error;
use uuid::Uuid;

pub mod models;
pub mod sqlite;

pub use models::IndexLogEntry;
pub use models::LogSource;
pub use models::Run;
pub use models::RunStatus;
pub use models::Session;
pub use models::SprocketCommand;
pub use models::Task;
pub use models::TaskLog;
pub use models::TaskStatus;
pub use sqlite::SqliteDatabase;

/// Database errors.
#[derive(Debug, Error)]
pub enum DatabaseError {
    /// A database error.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),

    /// A whoami error.
    #[error(transparent)]
    WhoAmI(#[from] whoami::Error),

    /// A migration error.
    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),

    /// An I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// A JSON serialization error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// Invalid database schema version.
    #[error("invalid database schema version: expected `{expected}`, found `{found}`")]
    InvalidVersion {
        /// Expected version.
        expected: String,
        /// Found version.
        found: String,
    },

    /// Resource not found.
    #[error("not found")]
    NotFound,
}

/// Result type for database operations.
pub type Result<T> = std::result::Result<T, DatabaseError>;

/// A database trait containing needed provenance operations.
#[async_trait]
pub trait Database: Send + Sync {
    /// Create a new session.
    async fn create_session(
        &self,
        id: Uuid,
        subcommand: SprocketCommand,
        created_by: &str,
    ) -> Result<Session>;

    /// Get a session by ID.
    async fn get_session(&self, id: Uuid) -> Result<Option<Session>>;

    /// List sessions.
    async fn list_sessions(&self, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<Session>>;

    /// Count total sessions.
    async fn count_sessions(&self) -> Result<i64>;

    /// Create a new run.
    ///
    /// The `target` parameter is `None` when the user did not provide a target.
    /// The resolved target should be set later via
    /// [`update_run_target`](Self::update_run_target).
    async fn create_run(
        &self,
        id: Uuid,
        session_id: Uuid,
        name: &str,
        source: &str,
        target: Option<&str>,
        inputs: &str,
    ) -> Result<Run>;

    /// Update run target after resolution.
    ///
    /// Returns `true` if a run was updated, `false` if the run was not found.
    #[must_use = "the return value indicates whether a run was updated"]
    async fn update_run_target(&self, id: Uuid, target: &str) -> Result<bool>;

    /// Update run status.
    async fn update_run_status(&self, id: Uuid, status: RunStatus) -> Result<()>;

    /// Update run started at.
    async fn update_run_started_at(
        &self,
        id: Uuid,
        started_at: Option<DateTime<Utc>>,
    ) -> Result<()>;

    /// Update run completed at.
    async fn update_run_completed_at(
        &self,
        id: Uuid,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<()>;

    /// Update run outputs.
    async fn update_run_outputs(&self, id: Uuid, outputs: &str) -> Result<()>;

    /// Update run error.
    async fn update_run_error(&self, id: Uuid, error: &str) -> Result<()>;

    /// Update run directory.
    ///
    /// Returns `true` if a run was updated, `false` if the run was not found.
    #[must_use = "the return value indicates whether a run was updated"]
    async fn update_run_directory(&self, id: Uuid, directory: &str) -> Result<bool>;

    /// Update run index directory.
    ///
    /// Returns `true` if a run was updated, `false` if the run was
    /// not found.
    #[must_use = "the return value indicates whether a run was updated"]
    async fn update_run_index_directory(&self, id: Uuid, index_directory: &str) -> Result<bool>;

    /// Get a run by ID.
    async fn get_run(&self, id: Uuid) -> Result<Option<Run>>;

    /// List runs with optional filtering and pagination.
    async fn list_runs(
        &self,
        status: Option<RunStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Run>>;

    /// Count runs with optional filtering.
    async fn count_runs(&self, status: Option<RunStatus>) -> Result<i64>;

    /// List runs by session ID.
    async fn list_runs_by_session(&self, session_id: Uuid) -> Result<Vec<Run>>;

    /// Create an index log entry.
    async fn create_index_log_entry(
        &self,
        run_id: Uuid,
        link_path: &str,
        target_path: &str,
    ) -> Result<IndexLogEntry>;

    /// List index log entries by run ID.
    async fn list_index_log_entries_by_run(&self, run_id: Uuid) -> Result<Vec<IndexLogEntry>>;

    /// List the latest index log entry for each unique index path.
    async fn list_latest_index_entries(&self) -> Result<Vec<IndexLogEntry>>;

    /// Create a task record in the given starting status.
    ///
    /// Creation is idempotent: if the task already exists, its current record
    /// is returned unchanged. Engine and Crankshaft events arrive on
    /// independent channels with no ordering between them, so either may be
    /// the first to observe a task.
    async fn create_task(&self, name: &str, run_id: Uuid, status: TaskStatus) -> Result<Task>;

    /// Advance a task to localizing its inputs.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found or
    /// has already advanced past initializing.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_localizing(&self, name: &str) -> Result<bool>;

    /// Advance a task to pending, meaning it has been submitted to a backend
    /// and is awaiting scheduling.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found or
    /// has already advanced past localizing.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_pending(&self, name: &str) -> Result<bool>;

    /// Update a task as served from the call cache.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found or
    /// has already reached a terminal status.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_cached(&self, name: &str, completed_at: DateTime<Utc>) -> Result<bool>;

    /// Update task with started timestamp.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found or
    /// has already advanced past pending.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_started(&self, name: &str, started_at: DateTime<Utc>) -> Result<bool>;

    /// Update task with completion data.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found or
    /// has already reached a terminal status.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_completed(
        &self,
        name: &str,
        exit_status: Option<i32>,
        completed_at: DateTime<Utc>,
    ) -> Result<bool>;

    /// Update task with failure data.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found or
    /// has already reached a terminal status.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_failed(
        &self,
        name: &str,
        error: &str,
        completed_at: DateTime<Utc>,
    ) -> Result<bool>;

    /// Update task as canceled.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found or
    /// has already reached a terminal status.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_canceled(&self, name: &str, completed_at: DateTime<Utc>) -> Result<bool>;

    /// Update task as preempted.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found or
    /// has already reached a terminal status.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_preempted(&self, name: &str, completed_at: DateTime<Utc>) -> Result<bool>;

    /// Get task by name.
    async fn get_task(&self, name: &str) -> Result<Task>;

    /// List all tasks with pagination and optional filters.
    async fn list_tasks(
        &self,
        run_id: Option<Uuid>,
        status: Option<TaskStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Task>>;

    /// Count total tasks with optional filters.
    async fn count_tasks(&self, run_id: Option<Uuid>, status: Option<TaskStatus>) -> Result<i64>;

    /// Count the tasks of a run grouped by status.
    ///
    /// Only statuses that have at least one task are returned.
    async fn count_tasks_by_status(&self, run_id: Uuid) -> Result<Vec<(TaskStatus, i64)>>;

    /// Insert a task log entry.
    async fn insert_task_log(&self, task_name: &str, source: LogSource, chunk: &[u8])
    -> Result<()>;

    /// Get task logs with pagination and optional source filter.
    async fn get_task_logs(
        &self,
        task_name: &str,
        source: Option<LogSource>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<TaskLog>>;

    /// Count task logs with optional source filter.
    async fn count_task_logs(&self, task_name: &str, source: Option<LogSource>) -> Result<i64>;

    /// Transition a run to `Running` status with `started_at` timestamp.
    async fn start_run(&self, id: Uuid, started_at: DateTime<Utc>) -> Result<()> {
        self.update_run_status(id, RunStatus::Running).await?;
        self.update_run_started_at(id, Some(started_at)).await?;
        Ok(())
    }

    /// Transition a run to `Completed` status with `completed_at` timestamp.
    async fn complete_run(&self, id: Uuid, completed_at: DateTime<Utc>) -> Result<()> {
        self.update_run_status(id, RunStatus::Completed).await?;
        self.update_run_completed_at(id, Some(completed_at)).await?;
        Ok(())
    }

    /// Transition a run to `Failed` status with error message and
    /// `completed_at` timestamp.
    async fn fail_run(&self, id: Uuid, error: &str, completed_at: DateTime<Utc>) -> Result<()> {
        self.update_run_status(id, RunStatus::Failed).await?;
        self.update_run_error(id, error).await?;
        self.update_run_completed_at(id, Some(completed_at)).await?;
        Ok(())
    }

    /// Transition a run to `Canceled` status with `completed_at` timestamp.
    async fn cancel_run(&self, id: Uuid, completed_at: DateTime<Utc>) -> Result<()> {
        self.update_run_status(id, RunStatus::Canceled).await?;
        self.update_run_completed_at(id, Some(completed_at)).await?;
        Ok(())
    }

    /// Transition a run to `Canceling` status.
    ///
    /// Returns `true` if the run was updated, `false` if it was not found or
    /// has already reached a terminal status.
    ///
    /// Cancellation is requested by signaling the run and then recording the
    /// request, so a run that finishes in between — which is the common case
    /// when the work being canceled is a transfer rather than a task — would
    /// otherwise have its outcome overwritten by the request to cancel it, and
    /// would appear to be canceling forever.
    #[must_use = "the return value indicates whether a run was updated"]
    async fn mark_run_canceling(&self, id: Uuid) -> Result<bool>;
}
