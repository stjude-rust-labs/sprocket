//! Manager command types and responses.

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use tokio::sync::oneshot;

use crate::server::db::WorkflowRow;
use crate::server::db::WorkflowStatus;

/// WDL source specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WdlSource {
    /// WDL content provided directly as a string.
    Content(String),
    /// WDL loaded from a file path.
    File(PathBuf),
}

/// Response for workflow submission.
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitResponse {
    /// The workflow ID.
    pub id: String,
    /// The generated workflow name.
    pub name: String,
}

/// Response for workflow status query.
#[derive(Debug, Serialize, Deserialize)]
pub struct StatusResponse {
    /// The workflow data.
    pub workflow: WorkflowRow,
}

/// Response for workflow list query.
#[derive(Debug, Serialize, Deserialize)]
pub struct ListResponse {
    /// The workflows.
    pub workflows: Vec<WorkflowRow>,
    /// Total count before pagination.
    pub total: i64,
}

/// Response for workflow cancellation.
#[derive(Debug, Serialize, Deserialize)]
pub struct CancelResponse {
    /// The workflow ID.
    pub id: String,
}

/// Response for workflow outputs query.
#[derive(Debug, Serialize, Deserialize)]
pub struct OutputsResponse {
    /// The workflow outputs as JSON.
    pub outputs: Option<Value>,
}

/// Response for workflow logs query.
#[derive(Debug, Serialize, Deserialize)]
pub struct LogsResponse {
    /// The log entries.
    pub logs: Vec<String>,
    /// Total count before pagination.
    pub total: i64,
}

/// Commands sent to the workflow manager.
#[derive(Debug)]
pub enum ManagerCommand {
    /// Submit a new workflow for execution.
    Submit {
        /// WDL source.
        source: WdlSource,
        /// Workflow inputs as JSON.
        inputs: Value,
        /// Channel to send the response back.
        rx: oneshot::Sender<anyhow::Result<SubmitResponse>>,
    },

    /// Get workflow status by ID.
    GetStatus {
        /// Workflow ID.
        id: String,
        /// Channel to send the response back.
        rx: oneshot::Sender<anyhow::Result<StatusResponse>>,
    },

    /// List workflows with optional filtering.
    List {
        /// Filter by status.
        status: Option<WorkflowStatus>,
        /// Number of results to return.
        limit: Option<i64>,
        /// Number of results to skip.
        offset: Option<i64>,
        /// Channel to send the response back.
        rx: oneshot::Sender<anyhow::Result<ListResponse>>,
    },

    /// Cancel a running workflow.
    Cancel {
        /// Workflow ID to cancel.
        id: String,
        /// Channel to send the response back.
        rx: oneshot::Sender<anyhow::Result<CancelResponse>>,
    },

    /// Get workflow outputs.
    GetOutputs {
        /// Workflow ID.
        id: String,
        /// Channel to send the response back.
        rx: oneshot::Sender<anyhow::Result<OutputsResponse>>,
    },

    /// Get workflow logs.
    GetLogs {
        /// Workflow ID.
        id: String,
        /// Number of log entries to return.
        limit: Option<i64>,
        /// Number of log entries to skip.
        offset: Option<i64>,
        /// Channel to send the response back.
        rx: oneshot::Sender<anyhow::Result<LogsResponse>>,
    },

    /// Shutdown the manager gracefully.
    Shutdown {
        /// Channel to send the response back.
        rx: oneshot::Sender<anyhow::Result<()>>,
    },
}
