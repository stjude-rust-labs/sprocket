//! Run API handlers.

use axum::Json;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::extract::rejection::QueryRejection;
use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use utoipa::IntoParams;
use utoipa::ToSchema;
use uuid::Uuid;

use super::AppState;
use super::RunStatus;
use super::error::Error;
use super::send_command;
use crate::system::v1::exec::svc::RunManagerCmd;
use crate::system::v1::exec::svc::run_manager::commands;

/// Request to submit a new run.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
    #[schema(example = "target")]
    pub target: Option<String>,
    /// Optional output name to index on.
    ///
    /// If provided, the run outputs will be indexed.
    #[serde(default)]
    #[schema(example = "an/index/path")]
    pub index_on: Option<String>,
}

/// Query parameters for listing runs.
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct ListRunsQueryParams {
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

/// The response for a "submit run" request.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SubmitResponse {
    /// The run ID.
    pub id: Uuid,
    /// The generated run name.
    pub name: String,
}

impl From<commands::SubmitResponse> for SubmitResponse {
    fn from(response: commands::SubmitResponse) -> Self {
        Self {
            id: response.id,
            name: response.name,
        }
    }
}

/// The run data for API responses.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Run {
    /// Unique identifier.
    pub id: Uuid,
    /// Foreign key to the session that submitted this run.
    pub session_id: Uuid,
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

impl From<crate::system::v1::db::Run> for Run {
    fn from(run: crate::system::v1::db::Run) -> Self {
        Self {
            id: run.id,
            session_id: run.session_id,
            name: run.name,
            source: run.source,
            status: run.status,
            inputs: run.inputs,
            outputs: run.outputs,
            error: run.error,
            directory: run.directory,
            index_directory: run.index_directory,
            started_at: run.started_at,
            completed_at: run.completed_at,
            created_at: run.created_at,
        }
    }
}

/// The response for a "get run" query.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RunResponse {
    /// The run data.
    #[serde(flatten)]
    pub run: Run,
}

impl From<commands::RunResponse> for RunResponse {
    fn from(response: commands::RunResponse) -> Self {
        Self {
            run: response.run.into(),
        }
    }
}

/// The response for "list runs" query.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListRunsResponse {
    /// The runs.
    pub runs: Vec<Run>,
    /// Total count before pagination.
    pub total: i64,
}

impl From<commands::ListRunsResponse> for ListRunsResponse {
    fn from(response: commands::ListRunsResponse) -> Self {
        Self {
            runs: response.runs.into_iter().map(Into::into).collect(),
            total: response.total,
        }
    }
}

/// The response for a "cancel run" request.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CancelRunResponse {
    /// The run ID.
    pub id: Uuid,
}

impl From<commands::CancelRunResponse> for CancelRunResponse {
    fn from(response: commands::CancelRunResponse) -> Self {
        Self { id: response.id }
    }
}

/// The response for a "get run outputs" query.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RunOutputsResponse {
    /// The run outputs as JSON.
    pub outputs: Option<Value>,
}

impl From<commands::RunOutputsResponse> for RunOutputsResponse {
    fn from(response: commands::RunOutputsResponse) -> Self {
        Self {
            outputs: response.outputs,
        }
    }
}

/// Submit a new run for execution.
#[utoipa::path(
    post,
    path = "/api/v1/runs",
    request_body = SubmitRunRequest,
    responses(
        (status = 200, description = "Run submitted successfully", body = SubmitResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "File sources not allowed"),
    ),
    tag = "runs"
)]
pub async fn submit_run(
    State(state): State<AppState>,
    Json(request): Json<SubmitRunRequest>,
) -> Result<Json<SubmitResponse>, Error> {
    let response = send_command(&state.run_manager_tx, |rx| RunManagerCmd::Submit {
        source: request.source,
        inputs: request.inputs,
        target: request.target,
        index_on: request.index_on,
        rx,
    })
    .await?;

    Ok(Json(response.into()))
}

/// Get run status by ID.
#[utoipa::path(
    get,
    path = "/api/v1/runs/{id}",
    params(
        ("id" = String, Path, description = "Run ID")
    ),
    responses(
        (status = 200, description = "Run found", body = RunResponse),
        (status = 404, description = "Run not found"),
    ),
    tag = "runs"
)]
pub async fn get_run(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<RunResponse>, Error> {
    let response = send_command(&state.run_manager_tx, |rx| RunManagerCmd::GetStatus {
        id,
        rx,
    })
    .await?;
    Ok(Json(response.into()))
}

/// List runs with optional filtering.
#[utoipa::path(
    get,
    path = "/api/v1/runs",
    params(ListRunsQueryParams),
    responses(
        (status = 200, description = "Runs retrieved", body = ListRunsResponse),
    ),
    tag = "runs"
)]
pub async fn list_runs(
    State(state): State<AppState>,
    query: Result<Query<ListRunsQueryParams>, QueryRejection>,
) -> Result<Json<ListRunsResponse>, Error> {
    let Query(query) = query.map_err(|rejection| match rejection {
        QueryRejection::FailedToDeserializeQueryString(err) => {
            Error::BadRequest(format!("invalid query parameters: {}", err))
        }
        _ => Error::BadRequest("invalid query parameters".to_string()),
    })?;

    let response = send_command(&state.run_manager_tx, |rx| RunManagerCmd::List {
        status: query.status,
        limit: query.limit,
        offset: query.offset,
        rx,
    })
    .await?;

    Ok(Json(response.into()))
}

/// Cancel a running run.
///
/// The cancellation behavior depends on the configured failure mode provided to
/// the server (typically via `Sprocket.toml`):
///
/// ## Slow Failure Mode (default)
///
/// The first call transitions the run to `Canceling` status, allowing currently
/// executing tasks to complete before stopping the workflow. A second call to
/// this endpoint will force immediate cancellation, transitioning the run to
/// `Canceled` status and halting all executing tasks.
///
/// ## Fast Failure Mode
///
/// A single call immediately transitions the run to `Canceled` status and halts
/// all executing tasks without waiting for them to complete.
#[utoipa::path(
    post,
    path = "/api/v1/runs/{id}/cancel",
    params(
        ("id" = String, Path, description = "Run ID")
    ),
    responses(
        (status = 200, description = "Run cancelled", body = CancelRunResponse),
        (status = 404, description = "Run not found"),
        (status = 409, description = "Run cannot be cancelled"),
    ),
    tag = "runs"
)]
pub async fn cancel_run(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CancelRunResponse>, Error> {
    let response =
        send_command(&state.run_manager_tx, |rx| RunManagerCmd::Cancel { id, rx }).await?;
    Ok(Json(response.into()))
}

/// Get run outputs.
#[utoipa::path(
    get,
    path = "/api/v1/runs/{id}/outputs",
    params(
        ("id" = String, Path, description = "Run ID")
    ),
    responses(
        (status = 200, description = "Run outputs retrieved", body = RunOutputsResponse),
        (status = 404, description = "Run not found"),
    ),
    tag = "runs"
)]
pub async fn get_run_outputs(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<RunOutputsResponse>, Error> {
    let response = send_command(&state.run_manager_tx, |rx| RunManagerCmd::GetOutputs {
        id,
        rx,
    })
    .await?;
    Ok(Json(response.into()))
}
