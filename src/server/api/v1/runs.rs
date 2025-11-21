//! Run API handlers.

use axum::Json;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::extract::rejection::QueryRejection;
use uuid::Uuid;

use super::AppState;
use super::CancelResponse;
use super::ListResponse;
use super::OutputsResponse;
use super::StatusResponse;
use super::SubmitResponse;
use super::common::send_command;
use super::error::Error;
use super::models::ListRunsQuery;
use super::models::SubmitRunRequest;
use crate::execution::ManagerCommand;

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
    let response = send_command(&state.manager, |rx| ManagerCommand::Submit {
        source: request.source,
        inputs: request.inputs,
        target: request.target,
        index_on: request.index_on,
        rx,
    })
    .await?;

    Ok(Json(response))
}

/// Get run status by ID.
#[utoipa::path(
    get,
    path = "/api/v1/runs/{id}",
    params(
        ("id" = String, Path, description = "Run ID")
    ),
    responses(
        (status = 200, description = "Run found", body = StatusResponse),
        (status = 404, description = "Run not found"),
    ),
    tag = "runs"
)]
pub async fn get_run(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<StatusResponse>, Error> {
    let response = send_command(&state.manager, |rx| ManagerCommand::GetStatus { id, rx }).await?;
    Ok(Json(response))
}

/// List runs with optional filtering.
#[utoipa::path(
    get,
    path = "/api/v1/runs",
    params(ListRunsQuery),
    responses(
        (status = 200, description = "Runs retrieved", body = ListResponse),
    ),
    tag = "runs"
)]
pub async fn list_runs(
    State(state): State<AppState>,
    query: Result<Query<ListRunsQuery>, QueryRejection>,
) -> Result<Json<ListResponse>, Error> {
    let Query(query) = query.map_err(|rejection| match rejection {
        QueryRejection::FailedToDeserializeQueryString(err) => {
            Error::BadRequest(format!("invalid query parameters: {}", err))
        }
        _ => Error::BadRequest("invalid query parameters".to_string()),
    })?;

    let response = send_command(&state.manager, |rx| ManagerCommand::List {
        status: query.status,
        limit: query.limit,
        offset: query.offset,
        rx,
    })
    .await?;

    Ok(Json(response))
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
        (status = 200, description = "Run cancelled", body = CancelResponse),
        (status = 404, description = "Run not found"),
        (status = 409, description = "Run cannot be cancelled"),
    ),
    tag = "runs"
)]
pub async fn cancel_run(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CancelResponse>, Error> {
    let response = send_command(&state.manager, |rx| ManagerCommand::Cancel { id, rx }).await?;
    Ok(Json(response))
}

/// Get run outputs.
#[utoipa::path(
    get,
    path = "/api/v1/runs/{id}/outputs",
    params(
        ("id" = String, Path, description = "Run ID")
    ),
    responses(
        (status = 200, description = "Run outputs retrieved", body = OutputsResponse),
        (status = 404, description = "Run not found"),
    ),
    tag = "runs"
)]
pub async fn get_run_outputs(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<OutputsResponse>, Error> {
    let response = send_command(&state.manager, |rx| ManagerCommand::GetOutputs { id, rx }).await?;
    Ok(Json(response))
}
