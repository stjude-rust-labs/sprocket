//! Models that back database entities.

use std::fmt;
use std::str::FromStr;

use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use sqlx::FromRow;
use sqlx::Type as SqlxType;
use utoipa::ToSchema;
use uuid::Uuid;

/// The status of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, SqlxType)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum RunStatus {
    /// The run is queued for execution.
    Queued,
    /// The run is currently running.
    Running,
    /// The run completed successfully.
    Completed,
    /// The run failed with an error.
    Failed,
    /// The run is being canceled.
    ///
    /// This state occurs when slow failing is enabled in Sprocket—the workflow
    /// waits until all currently running tasks and complete before finishing.
    Canceling,
    /// The run was canceled.
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
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "queued" => Ok(RunStatus::Queued),
            "running" => Ok(RunStatus::Running),
            "completed" => Ok(RunStatus::Completed),
            "failed" => Ok(RunStatus::Failed),
            "canceling" => Ok(RunStatus::Canceling),
            "canceled" => Ok(RunStatus::Canceled),
            _ => Err(()),
        }
    }
}

/// The Sprocket command used to submit runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, SqlxType)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum SprocketCommand {
    /// The run was submitted via the `run` command.
    Run,
    /// The run was submitted via an HTTP request to a server.
    Server,
}

impl fmt::Display for SprocketCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SprocketCommand::Run => write!(f, "run"),
            SprocketCommand::Server => write!(f, "server"),
        }
    }
}

impl FromStr for SprocketCommand {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "run" => Ok(SprocketCommand::Run),
            "server" => Ok(SprocketCommand::Server),
            _ => Err(format!("invalid session subcommand: {}", s)),
        }
    }
}

/// Task execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, SqlxType)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum TaskStatus {
    /// Task has been created.
    Pending,
    /// Task is executing.
    Running,
    /// Task completed successfully.
    Completed,
    /// Task execution failed.
    Failed,
    /// Task was canceled.
    Canceled,
    /// Task was preempted.
    Preempted,
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::Running => write!(f, "running"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Failed => write!(f, "failed"),
            TaskStatus::Canceled => write!(f, "canceled"),
            TaskStatus::Preempted => write!(f, "preempted"),
        }
    }
}

impl FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(TaskStatus::Pending),
            "running" => Ok(TaskStatus::Running),
            "completed" => Ok(TaskStatus::Completed),
            "failed" => Ok(TaskStatus::Failed),
            "canceled" => Ok(TaskStatus::Canceled),
            "preempted" => Ok(TaskStatus::Preempted),
            _ => Err(format!("invalid task status: {}", s)),
        }
    }
}

/// Task log source type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, SqlxType)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum LogSource {
    /// Standard output stream.
    Stdout,
    /// Standard error stream.
    Stderr,
}

impl fmt::Display for LogSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogSource::Stdout => write!(f, "stdout"),
            LogSource::Stderr => write!(f, "stderr"),
        }
    }
}

impl FromStr for LogSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "stdout" => Ok(LogSource::Stdout),
            "stderr" => Ok(LogSource::Stderr),
            _ => Err(format!("invalid log source: {}", s)),
        }
    }
}

/// Session record grouping related run submissions.
#[derive(Debug, Clone, FromRow)]
pub struct Session {
    /// Unique identifier.
    #[sqlx(try_from = "String")]
    pub uuid: Uuid,
    /// The Sprocket subcommand used to create this session.
    pub subcommand: SprocketCommand,
    /// User or system that created this session.
    pub created_by: String,
    /// Timestamp when the session was created.
    pub created_at: DateTime<Utc>,
}

/// Run record.
#[derive(Debug, Clone, FromRow)]
pub struct Run {
    /// Unique identifier.
    #[sqlx(try_from = "String")]
    pub uuid: Uuid,
    /// UUID of the session that submitted this run.
    #[sqlx(try_from = "String")]
    pub session_uuid: Uuid,
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
#[derive(Debug, Clone, FromRow)]
pub struct IndexLogEntry {
    /// Unique identifier.
    pub id: i64,
    /// UUID of the run that created this index entry.
    #[sqlx(try_from = "String")]
    pub run_uuid: Uuid,
    /// Path to the symlink in the index directory.
    pub link_path: String,
    /// Path to the actual run output file being symlinked.
    pub target_path: String,
    /// Timestamp when the symlink was created.
    pub created_at: DateTime<Utc>,
}

/// Task execution record.
#[derive(Debug, Clone, FromRow)]
pub struct Task {
    /// Task name from WDL.
    pub name: String,
    /// UUID of the run managing this task.
    #[sqlx(try_from = "String")]
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

/// Task log entry for stdout/stderr output.
#[derive(Debug, Clone, FromRow)]
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
