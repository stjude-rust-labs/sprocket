//! Database models.

use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use sqlx::FromRow;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use uuid::Uuid;

/// Workflow execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowStatus {
    /// Workflow is pending execution.
    Pending,
    /// Workflow is currently running.
    Running,
    /// Workflow completed successfully.
    Completed,
    /// Workflow failed with an error.
    Failed,
    /// Workflow was cancelled.
    Cancelled,
}

impl fmt::Display for WorkflowStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkflowStatus::Pending => write!(f, "pending"),
            WorkflowStatus::Running => write!(f, "running"),
            WorkflowStatus::Completed => write!(f, "completed"),
            WorkflowStatus::Failed => write!(f, "failed"),
            WorkflowStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl FromStr for WorkflowStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(WorkflowStatus::Pending),
            "running" => Ok(WorkflowStatus::Running),
            "completed" => Ok(WorkflowStatus::Completed),
            "failed" => Ok(WorkflowStatus::Failed),
            "cancelled" => Ok(WorkflowStatus::Cancelled),
            _ => Err(format!("invalid workflow status: {}", s)),
        }
    }
}

impl TryFrom<String> for WorkflowStatus {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_str(&s)
    }
}

/// Invocation method indicating how workflows were submitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InvocationMethod {
    /// Submitted via CLI.
    Cli,
    /// Submitted via HTTP API.
    Http,
}

impl fmt::Display for InvocationMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvocationMethod::Cli => write!(f, "cli"),
            InvocationMethod::Http => write!(f, "http"),
        }
    }
}

impl FromStr for InvocationMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cli" => Ok(InvocationMethod::Cli),
            "http" => Ok(InvocationMethod::Http),
            _ => Err(format!("invalid invocation method: {}", s)),
        }
    }
}

impl TryFrom<String> for InvocationMethod {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_str(&s)
    }
}

/// Invocation record grouping related workflow submissions.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Invocation {
    /// Unique identifier.
    #[sqlx(try_from = "String")]
    pub id: Uuid,
    /// How the workflows were submitted.
    #[sqlx(try_from = "String")]
    pub method: InvocationMethod,
    /// Optional user or system that created this invocation.
    pub created_by: Option<String>,
    /// Timestamp when the invocation was created.
    pub created_at: DateTime<Utc>,
}

/// Workflow execution record.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Workflow {
    /// Unique identifier.
    #[sqlx(try_from = "String")]
    pub id: Uuid,
    /// Foreign key to the invocation that submitted this workflow.
    #[sqlx(try_from = "String")]
    pub invocation_id: Uuid,
    /// Name of the workflow.
    pub name: String,
    /// Source WDL file path or URL.
    pub source: String,
    /// Current execution status.
    #[sqlx(try_from = "String")]
    pub status: WorkflowStatus,
    /// JSON-encoded workflow inputs.
    pub inputs: String,
    /// JSON-encoded workflow outputs.
    pub outputs: Option<String>,
    /// Error message if workflow failed.
    pub error: Option<String>,
    /// Path to the workflow execution directory.
    pub execution_dir: String,
    /// Timestamp when the workflow was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when the workflow started executing.
    pub started_at: Option<DateTime<Utc>>,
    /// Timestamp when the workflow finished executing.
    pub completed_at: Option<DateTime<Utc>>,
}

/// Index log entry tracking symlink creation for workflow outputs.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IndexLogEntry {
    /// Unique identifier.
    #[sqlx(try_from = "String")]
    pub id: Uuid,
    /// Foreign key to the workflow that created this index entry.
    #[sqlx(try_from = "String")]
    pub workflow_id: Uuid,
    /// Path to the symlink in the index directory.
    #[sqlx(try_from = "String")]
    pub index_path: PathBuf,
    /// Path to the actual workflow output file being symlinked.
    #[sqlx(try_from = "String")]
    pub target_path: PathBuf,
    /// Timestamp when the symlink was created.
    pub created_at: DateTime<Utc>,
}
