//! API request and response models.

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use utoipa::IntoParams;
use utoipa::ToSchema;

use crate::server::db::WorkflowRow;
use crate::server::db::WorkflowStatus;
use crate::server::manager::commands::WdlSource;

/// Request to submit a new workflow.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubmitWorkflowRequest {
    /// WDL source.
    pub source: WdlSourceRequest,
    /// Workflow inputs as JSON.
    #[serde(default)]
    pub inputs: Value,
}

/// WDL source in API request.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WdlSourceRequest {
    /// WDL content provided directly as a string.
    Content {
        /// The WDL content.
        content: String,
    },
    /// WDL loaded from a file path.
    File {
        /// Path to the WDL file.
        path: String,
    },
}

impl From<WdlSourceRequest> for WdlSource {
    fn from(source: WdlSourceRequest) -> Self {
        match source {
            WdlSourceRequest::Content { content } => WdlSource::Content(content),
            WdlSourceRequest::File { path } => WdlSource::File(PathBuf::from(path)),
        }
    }
}

/// Response for workflow submission.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubmitWorkflowResponse {
    /// The workflow ID.
    pub id: String,
    /// The generated workflow name.
    pub name: String,
}

/// Response for workflow status query.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GetWorkflowResponse {
    /// The workflow data.
    #[serde(flatten)]
    pub workflow: WorkflowRow,
}

/// Query parameters for listing workflows.
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct ListWorkflowsQuery {
    /// Filter by status.
    #[serde(default)]
    pub status: Option<WorkflowStatus>,
    /// Number of results to return (default: `100`).
    #[serde(default)]
    pub limit: Option<i64>,
    /// Number of results to skip (default: `0`).
    #[serde(default)]
    pub offset: Option<i64>,
}

/// Response for workflow list query.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListWorkflowsResponse {
    /// The workflows.
    pub workflows: Vec<WorkflowRow>,
    /// Total count before pagination.
    pub total: i64,
}

/// Response for workflow cancellation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CancelWorkflowResponse {
    /// The workflow ID.
    pub id: String,
}

/// Response for workflow outputs query.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GetWorkflowOutputsResponse {
    /// The workflow outputs as JSON.
    pub outputs: Option<Value>,
}

/// Query parameters for getting workflow logs.
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct GetWorkflowLogsQuery {
    /// Number of log entries to return (default: `100`).
    #[serde(default)]
    pub limit: Option<i64>,
    /// Number of log entries to skip (default: `0`).
    #[serde(default)]
    pub offset: Option<i64>,
}

/// Response for workflow logs query.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GetWorkflowLogsResponse {
    /// The log entries.
    pub logs: Vec<String>,
    /// Total count before pagination.
    pub total: i64,
}
