//! Database models.

use std::fmt;
use std::str::FromStr;

use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

/// Run status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum RunStatus {
    /// Run is queued for execution.
    Queued,
    /// Run is currently running.
    Running,
    /// Run completed successfully.
    Completed,
    /// Run failed with an error.
    Failed,
    /// Run is being canceled.
    Canceling,
    /// Run was canceled.
    Canceled,
}

impl fmt::Display for RunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunStatus::Queued => write!(f, "queued"),
            RunStatus::Running => write!(f, "running"),
            RunStatus::Completed => write!(f, "completed"),
            RunStatus::Failed => write!(f, "failed"),
            RunStatus::Canceling => write!(f, "canceling"),
            RunStatus::Canceled => write!(f, "canceled"),
        }
    }
}

impl FromStr for RunStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "queued" => Ok(RunStatus::Queued),
            "running" => Ok(RunStatus::Running),
            "completed" => Ok(RunStatus::Completed),
            "failed" => Ok(RunStatus::Failed),
            "canceling" => Ok(RunStatus::Canceling),
            "canceled" => Ok(RunStatus::Canceled),
            _ => Err(format!("invalid run status: {}", s)),
        }
    }
}

impl TryFrom<String> for RunStatus {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_str(&s)
    }
}

/// Invocation method indicating how runs were submitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum InvocationMethod {
    /// The run was submitted via the `run` command.
    Run,
    /// The run was submitted via an HTTP request to a server.
    Server,
}

impl fmt::Display for InvocationMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvocationMethod::Run => write!(f, "run"),
            InvocationMethod::Server => write!(f, "server"),
        }
    }
}

impl FromStr for InvocationMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "run" => Ok(InvocationMethod::Run),
            "server" => Ok(InvocationMethod::Server),
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

/// Invocation record grouping related run submissions.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Invocation {
    /// Unique identifier.
    #[schema(value_type = String)]
    pub id: Uuid,
    /// How the runs were submitted.
    pub method: InvocationMethod,
    /// User or system that created this invocation.
    pub created_by: String,
    /// Timestamp when the invocation was created.
    pub created_at: DateTime<Utc>,
}

/// Run record.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Run {
    /// Unique identifier.
    #[schema(value_type = String)]
    pub id: Uuid,
    /// Foreign key to the invocation that submitted this run.
    #[schema(value_type = String)]
    pub invocation_id: Uuid,
    /// Name of the run.
    pub name: String,
    /// Source WDL file path or URL.
    pub source: String,
    /// Current status.
    pub status: RunStatus,
    /// JSON-encoded inputs.
    pub inputs: String,
    /// JSON-encoded outputs.
    pub outputs: Option<String>,
    /// Error message if run failed.
    pub error: Option<String>,
    /// Path to the run directory.
    pub directory: String,
    /// Path to the indexed output directory (`null` if not indexed).
    pub index_directory: Option<String>,
    /// Timestamp when the run started.
    pub started_at: Option<DateTime<Utc>>,
    /// Timestamp when the run finished.
    pub completed_at: Option<DateTime<Utc>>,
    /// Timestamp when the run was created.
    pub created_at: DateTime<Utc>,
}

/// Index log entry tracking symlink creation for run outputs.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IndexLogEntry {
    /// Unique identifier.
    pub id: i64,
    /// Foreign key to the run that created this index entry.
    pub run_id: Uuid,
    /// Path to the symlink in the index directory.
    pub index_path: String,
    /// Path to the actual run output file being symlinked.
    pub target_path: String,
    /// Timestamp when the symlink was created.
    pub created_at: DateTime<Utc>,
}
