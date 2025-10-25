//! Database models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// Workflow status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum WorkflowStatus {
    /// Workflow is queued for execution.
    Queued,
    /// Workflow is currently running.
    Running,
    /// Workflow completed successfully.
    Completed,
    /// Workflow failed.
    Failed,
    /// Workflow was cancelled.
    Cancelled,
}

/// WDL source type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum WdlSourceType {
    /// Direct WDL content.
    Content,
    /// Path to WDL file on server.
    File,
}

/// Workflow database row.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct WorkflowRow {
    /// Unique workflow ID.
    id: String,
    /// Human-readable workflow name.
    name: String,
    /// Current workflow status.
    status: WorkflowStatus,
    /// Type of WDL source.
    wdl_source_type: WdlSourceType,
    /// WDL source value (content or file path).
    wdl_source_value: String,
    /// Workflow inputs as JSON.
    inputs: String,
    /// Workflow outputs as JSON (if completed).
    outputs: Option<String>,
    /// Error message (if failed).
    error: Option<String>,
    /// Directory where workflow execution occurred.
    run_directory: Option<String>,
    /// When the workflow was created.
    created_at: DateTime<Utc>,
    /// When the workflow started running.
    started_at: Option<DateTime<Utc>>,
    /// When the workflow completed.
    completed_at: Option<DateTime<Utc>>,
}

impl WorkflowRow {
    /// Get the workflow ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the workflow name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the workflow status.
    pub fn status(&self) -> WorkflowStatus {
        self.status
    }

    /// Get the WDL source type.
    pub fn wdl_source_type(&self) -> WdlSourceType {
        self.wdl_source_type
    }

    /// Get the WDL source value.
    pub fn wdl_source_value(&self) -> &str {
        &self.wdl_source_value
    }

    /// Get the workflow inputs as JSON.
    pub fn inputs(&self) -> &str {
        &self.inputs
    }

    /// Get the workflow outputs as JSON.
    pub fn outputs(&self) -> Option<&str> {
        self.outputs.as_deref()
    }

    /// Get the error message.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Get the run directory.
    pub fn run_directory(&self) -> Option<&str> {
        self.run_directory.as_deref()
    }

    /// Get when the workflow was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Get when the workflow started running.
    pub fn started_at(&self) -> Option<DateTime<Utc>> {
        self.started_at
    }

    /// Get when the workflow completed.
    pub fn completed_at(&self) -> Option<DateTime<Utc>> {
        self.completed_at
    }
}

/// Log entry database row.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LogRow {
    /// Log entry ID.
    id: i64,
    /// Associated workflow ID.
    workflow_id: String,
    /// Log level.
    level: String,
    /// Log message.
    message: String,
    /// Log source (e.g., `task:hello`, `engine`).
    source: Option<String>,
    /// When the log was created.
    created_at: DateTime<Utc>,
}

impl LogRow {
    /// Get the log entry ID.
    pub fn id(&self) -> i64 {
        self.id
    }

    /// Get the associated workflow ID.
    pub fn workflow_id(&self) -> &str {
        &self.workflow_id
    }

    /// Get the log level.
    pub fn level(&self) -> &str {
        &self.level
    }

    /// Get the log message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Get the log source.
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// Get when the log was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}
