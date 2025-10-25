//! Workflow API handlers.

use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::Json;
use tokio::sync::oneshot;

use super::error::Error;
use super::models::*;
use super::AppState;
use crate::server::manager::ManagerCommand;

/// Submit a new workflow for execution.
///
/// # Errors
///
/// Returns an error if the workflow submission fails.
#[utoipa::path(
    post,
    path = "/workflows",
    request_body = SubmitWorkflowRequest,
    responses(
        (status = 200, description = "Workflow submitted successfully", body = SubmitWorkflowResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "File sources not allowed"),
    )
)]
pub async fn submit_workflow(
    State(state): State<AppState>,
    Json(request): Json<SubmitWorkflowRequest>,
) -> Result<Json<SubmitWorkflowResponse>, Error> {
    let (tx, rx) = oneshot::channel();

    state
        .manager
        .send(ManagerCommand::Submit {
            source: request.source.into(),
            inputs: request.inputs,
            rx: tx,
        })
        .await
        .map_err(|_| Error::Internal(anyhow::anyhow!("manager channel closed")))?;

    let response = rx
        .await
        .map_err(|_| Error::Internal(anyhow::anyhow!("manager response channel closed")))?
        .map_err(Error::Internal)?;

    Ok(Json(SubmitWorkflowResponse {
        id: response.id,
        name: response.name,
    }))
}

/// Get workflow status by ID.
///
/// # Errors
///
/// Returns an error if the workflow is not found.
#[utoipa::path(
    get,
    path = "/workflows/{id}",
    params(
        ("id" = String, Path, description = "Workflow ID")
    ),
    responses(
        (status = 200, description = "Workflow found", body = GetWorkflowResponse),
        (status = 404, description = "Workflow not found"),
    )
)]
pub async fn get_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<GetWorkflowResponse>, Error> {
    let (tx, rx) = oneshot::channel();

    state
        .manager
        .send(ManagerCommand::GetStatus { id, rx: tx })
        .await
        .map_err(|_| Error::Internal(anyhow::anyhow!("manager channel closed")))?;

    let response = rx
        .await
        .map_err(|_| Error::Internal(anyhow::anyhow!("manager response channel closed")))?
        .map_err(Error::Internal)?;

    Ok(Json(GetWorkflowResponse {
        workflow: response.workflow,
    }))
}

/// List workflows with optional filtering.
///
/// # Errors
///
/// Returns an error if the query fails.
#[utoipa::path(
    get,
    path = "/workflows",
    params(ListWorkflowsQuery),
    responses(
        (status = 200, description = "Workflows retrieved", body = ListWorkflowsResponse),
    )
)]
pub async fn list_workflows(
    State(state): State<AppState>,
    Query(query): Query<ListWorkflowsQuery>,
) -> Result<Json<ListWorkflowsResponse>, Error> {
    let (tx, rx) = oneshot::channel();

    state
        .manager
        .send(ManagerCommand::List {
            status: query.status,
            limit: query.limit,
            offset: query.offset,
            rx: tx,
        })
        .await
        .map_err(|_| Error::Internal(anyhow::anyhow!("manager channel closed")))?;

    let response = rx
        .await
        .map_err(|_| Error::Internal(anyhow::anyhow!("manager response channel closed")))?
        .map_err(Error::Internal)?;

    Ok(Json(ListWorkflowsResponse {
        workflows: response.workflows,
        total: response.total,
    }))
}

/// Cancel a running workflow.
///
/// # Errors
///
/// Returns an error if the workflow cannot be cancelled.
#[utoipa::path(
    post,
    path = "/workflows/{id}/cancel",
    params(
        ("id" = String, Path, description = "Workflow ID")
    ),
    responses(
        (status = 200, description = "Workflow cancelled", body = CancelWorkflowResponse),
        (status = 404, description = "Workflow not found"),
        (status = 409, description = "Workflow cannot be cancelled"),
    )
)]
pub async fn cancel_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CancelWorkflowResponse>, Error> {
    let (tx, rx) = oneshot::channel();

    state
        .manager
        .send(ManagerCommand::Cancel { id, rx: tx })
        .await
        .map_err(|_| Error::Internal(anyhow::anyhow!("manager channel closed")))?;

    let response = rx
        .await
        .map_err(|_| Error::Internal(anyhow::anyhow!("manager response channel closed")))?
        .map_err(Error::Internal)?;

    Ok(Json(CancelWorkflowResponse { id: response.id }))
}

/// Get workflow outputs.
///
/// # Errors
///
/// Returns an error if the workflow is not found.
#[utoipa::path(
    get,
    path = "/workflows/{id}/outputs",
    params(
        ("id" = String, Path, description = "Workflow ID")
    ),
    responses(
        (status = 200, description = "Workflow outputs retrieved", body = GetWorkflowOutputsResponse),
        (status = 404, description = "Workflow not found"),
    )
)]
pub async fn get_workflow_outputs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<GetWorkflowOutputsResponse>, Error> {
    let (tx, rx) = oneshot::channel();

    state
        .manager
        .send(ManagerCommand::GetOutputs { id, rx: tx })
        .await
        .map_err(|_| Error::Internal(anyhow::anyhow!("manager channel closed")))?;

    let response = rx
        .await
        .map_err(|_| Error::Internal(anyhow::anyhow!("manager response channel closed")))?
        .map_err(Error::Internal)?;

    Ok(Json(GetWorkflowOutputsResponse {
        outputs: response.outputs,
    }))
}

/// Get workflow logs.
///
/// # Errors
///
/// Returns an error if the workflow is not found.
#[utoipa::path(
    get,
    path = "/workflows/{id}/logs",
    params(
        ("id" = String, Path, description = "Workflow ID"),
        GetWorkflowLogsQuery,
    ),
    responses(
        (status = 200, description = "Workflow logs retrieved", body = GetWorkflowLogsResponse),
        (status = 404, description = "Workflow not found"),
    )
)]
pub async fn get_workflow_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<GetWorkflowLogsQuery>,
) -> Result<Json<GetWorkflowLogsResponse>, Error> {
    let (tx, rx) = oneshot::channel();

    state
        .manager
        .send(ManagerCommand::GetLogs {
            id,
            limit: query.limit,
            offset: query.offset,
            rx: tx,
        })
        .await
        .map_err(|_| Error::Internal(anyhow::anyhow!("manager channel closed")))?;

    let response = rx
        .await
        .map_err(|_| Error::Internal(anyhow::anyhow!("manager response channel closed")))?
        .map_err(Error::Internal)?;

    Ok(Json(GetWorkflowLogsResponse {
        logs: response.logs,
        total: response.total,
    }))
}
