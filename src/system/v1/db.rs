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

    /// A migration error.
    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),

    /// An I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),

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

    /// Create a new run.
    async fn create_run(
        &self,
        id: Uuid,
        session_id: Uuid,
        name: &str,
        source: &str,
        inputs: &str,
        directory: &str,
    ) -> Result<Run>;

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

    /// Update run index directory.
    ///
    /// Returns `true` if a run was updated, `false` if the run was
    /// not found.
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

    /// Create a new task record.
    async fn create_task(&self, name: &str, run_id: Uuid) -> Result<Task>;

    /// Update task with started timestamp.
    ///
    /// Returns `true` if a task was updated, `false` if not found.
    async fn update_task_started(&self, name: &str, started_at: DateTime<Utc>) -> Result<bool>;

    /// Update task with completion data.
    ///
    /// Returns `true` if a task was updated, `false` if not found.
    async fn update_task_completed(
        &self,
        name: &str,
        exit_status: Option<i32>,
        completed_at: DateTime<Utc>,
    ) -> Result<bool>;

    /// Update task with failure data.
    ///
    /// Returns `true` if a task was updated, `false` if not found.
    async fn update_task_failed(
        &self,
        name: &str,
        error: &str,
        completed_at: DateTime<Utc>,
    ) -> Result<bool>;

    /// Update task as canceled.
    ///
    /// Returns `true` if a task was updated, `false` if not found.
    async fn update_task_canceled(&self, name: &str, completed_at: DateTime<Utc>) -> Result<bool>;

    /// Update task as preempted.
    ///
    /// Returns `true` if a task was updated, `false` if not found.
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
}
