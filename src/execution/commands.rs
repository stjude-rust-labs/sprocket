//! Manager command types and responses.

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::oneshot;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::database::Invocation;
use crate::database::Run;
use crate::database::RunStatus;
use crate::execution::ManagerResult;

/// Response for run submission.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SubmitResponse {
    /// The run ID.
    #[schema(value_type = String)]
    pub id: Uuid,
    /// The generated run name.
    pub name: String,
}

/// Response for run status query.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StatusResponse {
    /// The run data.
    #[serde(flatten)]
    pub run: Run,
}

/// Response for run list query.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListResponse {
    /// The runs.
    pub runs: Vec<Run>,
    /// Total count before pagination.
    pub total: i64,
}

/// Response for run cancellation.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CancelResponse {
    /// The run ID.
    #[schema(value_type = String)]
    pub id: Uuid,
}

/// Response for run outputs query.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OutputsResponse {
    /// The run outputs as JSON.
    pub outputs: Option<Value>,
}

/// Response for invocation query.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InvocationResponse {
    /// The invocation data.
    #[serde(flatten)]
    pub invocation: Invocation,
}

/// Response for invocation list query.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListInvocationsResponse {
    /// The invocations.
    pub invocations: Vec<Invocation>,
}

/// Commands sent to the run manager.
#[derive(Debug)]
pub enum ManagerCommand {
    /// Ping the manager to check if it's ready.
    Ping {
        /// Channel to send the response back.
        rx: oneshot::Sender<ManagerResult<()>>,
    },

    /// Submit a new run for execution.
    Submit {
        /// WDL source path (local file path or HTTP/HTTPS URL).
        source: String,
        /// Run inputs as JSON.
        inputs: Value,
        /// Optional target workflow or task name to execute.
        target: Option<String>,
        /// Optional output directory to index on.
        index_on: Option<String>,
        /// Channel to send the response back.
        rx: oneshot::Sender<ManagerResult<SubmitResponse>>,
    },

    /// Get run status by ID.
    GetStatus {
        /// Run ID.
        id: Uuid,
        /// Channel to send the response back.
        rx: oneshot::Sender<ManagerResult<StatusResponse>>,
    },

    /// List runs with optional filtering.
    List {
        /// Filter by status.
        status: Option<RunStatus>,
        /// Number of results to return.
        limit: Option<i64>,
        /// Number of results to skip.
        offset: Option<i64>,
        /// Channel to send the response back.
        rx: oneshot::Sender<ManagerResult<ListResponse>>,
    },

    /// Cancel a running run.
    Cancel {
        /// Run ID to cancel.
        id: Uuid,
        /// Channel to send the response back.
        rx: oneshot::Sender<ManagerResult<CancelResponse>>,
    },

    /// Get run outputs.
    GetOutputs {
        /// Run ID.
        id: Uuid,
        /// Channel to send the response back.
        rx: oneshot::Sender<ManagerResult<OutputsResponse>>,
    },

    /// Get invocation by ID.
    GetInvocation {
        /// Invocation ID.
        id: Uuid,
        /// Channel to send the response back.
        rx: oneshot::Sender<ManagerResult<InvocationResponse>>,
    },

    /// List invocations.
    ListInvocations {
        /// Number of results to return.
        limit: Option<i64>,
        /// Number of results to skip.
        offset: Option<i64>,
        /// Channel to send the response back.
        rx: oneshot::Sender<ManagerResult<ListInvocationsResponse>>,
    },

    /// Shutdown the manager gracefully.
    Shutdown {
        /// Channel to send the response back.
        rx: oneshot::Sender<ManagerResult<()>>,
    },
}
