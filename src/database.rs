//! Database abstraction layer.

use async_trait::async_trait;
use chrono::DateTime;
use chrono::Utc;
use thiserror::Error;
use uuid::Uuid;

pub mod models;
pub mod sqlite;

pub use models::IndexLogEntry;
pub use models::Invocation;
pub use models::InvocationMethod;
pub use models::Run;
pub use models::RunStatus;
pub use sqlite::SqliteDatabase;

/// Database errors.
#[derive(Debug, Error)]
pub enum DatabaseError {
    /// A database error.
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    /// A migration error.
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    /// An I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    /// A validation error.
    #[error("validation error: {0}")]
    Validation(String),
}

/// Result type for database operations.
pub type Result<T> = std::result::Result<T, DatabaseError>;

/// A database trait containing needed provenance operations.
#[async_trait]
pub trait Database: Send + Sync + std::fmt::Debug {
    /// Create a new invocation.
    async fn create_invocation(
        &self,
        id: Uuid,
        method: InvocationMethod,
        created_by: &str,
    ) -> Result<Invocation>;

    /// Get an invocation by ID.
    async fn get_invocation(&self, id: Uuid) -> Result<Option<Invocation>>;

    /// List invocations.
    async fn list_invocations(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Invocation>>;

    /// Create a new run.
    async fn create_run(
        &self,
        id: Uuid,
        invocation_id: Uuid,
        name: &str,
        source: &str,
        inputs: &str,
        directory: &str,
    ) -> Result<Run>;

    /// Update run status.
    async fn update_run_status(
        &self,
        id: Uuid,
        status: RunStatus,
        started_at: Option<DateTime<Utc>>,
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

    /// List runs by invocation ID.
    async fn list_runs_by_invocation(&self, invocation_id: Uuid) -> Result<Vec<Run>>;

    /// Create an index log entry.
    async fn create_index_log_entry(
        &self,
        run_id: Uuid,
        index_path: &str,
        target_path: &str,
    ) -> Result<IndexLogEntry>;

    /// List index log entries by run ID.
    async fn list_index_log_entries_by_run(&self, run_id: Uuid) -> Result<Vec<IndexLogEntry>>;

    /// List the latest index log entry for each unique index path.
    async fn list_latest_index_entries(&self) -> Result<Vec<IndexLogEntry>>;
}
