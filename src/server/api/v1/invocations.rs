//! Invocation API handlers.

use axum::Json;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use uuid::Uuid;

use super::AppState;
use super::InvocationResponse;
use super::ListInvocationsResponse;
use super::common::send_command;
use super::error::Error;
use super::models::ListInvocationsQuery;
use crate::execution::ManagerCommand;

/// List invocations.
#[utoipa::path(
    get,
    path = "/api/v1/invocations",
    params(ListInvocationsQuery),
    responses(
        (status = 200, description = "Invocations retrieved", body = ListInvocationsResponse),
    ),
    tag = "invocations"
)]
pub async fn list_invocations(
    State(state): State<AppState>,
    Query(query): Query<ListInvocationsQuery>,
) -> Result<Json<ListInvocationsResponse>, Error> {
    let response = send_command(&state.manager, |rx| ManagerCommand::ListInvocations {
        limit: query.limit,
        offset: query.offset,
        rx,
    })
    .await?;

    Ok(Json(response))
}

/// Get invocation by ID.
#[utoipa::path(
    get,
    path = "/api/v1/invocations/{id}",
    params(
        ("id" = String, Path, description = "Invocation ID")
    ),
    responses(
        (status = 200, description = "Invocation found", body = InvocationResponse),
        (status = 404, description = "Invocation not found"),
    ),
    tag = "invocations"
)]
pub async fn get_invocation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<InvocationResponse>, Error> {
    let response = send_command(&state.manager, |rx| ManagerCommand::GetInvocation {
        id,
        rx,
    })
    .await?;
    Ok(Json(response))
}
