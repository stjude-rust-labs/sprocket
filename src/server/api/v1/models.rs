//! API request and response models.

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use utoipa::IntoParams;
use utoipa::ToSchema;

use crate::database::RunStatus;

/// Request to submit a new run.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "source": "workflow.wdl",
    "inputs": {
        "key": "value"
    },
    "target": "my_workflow",
    "index_on": "output_dir"
}))]
pub struct SubmitRunRequest {
    /// WDL source path (local file path or HTTP/HTTPS URL).
    pub source: String,
    /// Run inputs as JSON object.
    #[serde(default)]
    pub inputs: Value,
    /// Optional target workflow or task name to execute.
    ///
    /// If not provided, will attempt to automatically select:
    ///
    /// 1. The workflow in the document (if one exists)
    /// 2. The single task in the document (if no workflow but exactly one task)
    /// 3. Error if ambiguous (no workflow and multiple tasks)
    #[serde(default)]
    pub target: Option<String>,
    /// Optional output name to index on.
    ///
    /// If provided, the run outputs will be indexed.
    #[serde(default)]
    pub index_on: Option<String>,
}

/// Query parameters for listing runs.
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct ListRunsQuery {
    /// Filter by status.
    #[serde(default)]
    pub status: Option<RunStatus>,
    /// Number of results to return (default: `100`).
    #[serde(default)]
    pub limit: Option<i64>,
    /// Number of results to skip (default: `0`).
    #[serde(default)]
    pub offset: Option<i64>,
}

/// Query parameters for listing invocations.
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct ListInvocationsQuery {
    /// Number of results to return (default: `100`).
    #[serde(default)]
    pub limit: Option<i64>,
    /// Number of results to skip (default: `0`).
    #[serde(default)]
    pub offset: Option<i64>,
}
