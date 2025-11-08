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
pub use models::Workflow;
pub use models::WorkflowStatus;
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
}

/// Result type for database operations.
pub type Result<T> = std::result::Result<T, DatabaseError>;

/// A database trait containing needed provenance operations.
#[async_trait]
pub trait Database: Send + Sync {
    /// Create a new invocation.
    async fn create_invocation(
        &self,
        id: Uuid,
        method: InvocationMethod,
        created_by: Option<String>,
    ) -> Result<Invocation>;

    /// Get an invocation by ID.
    async fn get_invocation(&self, id: Uuid) -> Result<Option<Invocation>>;

    /// Create a new workflow.
    async fn create_workflow(
        &self,
        id: Uuid,
        invocation_id: Uuid,
        name: String,
        source: String,
        inputs: String,
        execution_dir: String,
    ) -> Result<Workflow>;

    /// Update workflow status.
    async fn update_workflow_status(
        &self,
        id: Uuid,
        status: WorkflowStatus,
        started_at: Option<DateTime<Utc>>,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<()>;

    /// Update workflow outputs.
    async fn update_workflow_outputs(&self, id: Uuid, outputs: String) -> Result<()>;

    /// Update workflow error.
    async fn update_workflow_error(&self, id: Uuid, error: String) -> Result<()>;

    /// Get a workflow by ID.
    async fn get_workflow(&self, id: Uuid) -> Result<Option<Workflow>>;

    /// List workflows by invocation ID.
    async fn list_workflows_by_invocation(&self, invocation_id: Uuid) -> Result<Vec<Workflow>>;

    /// Create an index log entry.
    async fn create_index_log_entry(
        &self,
        id: Uuid,
        workflow_id: Uuid,
        index_path: String,
        target_path: String,
    ) -> Result<IndexLogEntry>;

    /// List index log entries by workflow ID.
    async fn list_index_log_entries_by_workflow(
        &self,
        workflow_id: Uuid,
    ) -> Result<Vec<IndexLogEntry>>;
}
