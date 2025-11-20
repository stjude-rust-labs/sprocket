//! V1 API routes and documentation.

use axum::Router;
use axum::routing::get;
use axum::routing::post;
use utoipa::OpenApi;

pub mod common;
pub mod error;
pub mod invocations;
pub mod models;
pub mod runs;

// Re-export command response types for API use.
use self::invocations::*;
use self::runs::*;
use super::AppState;
pub use crate::execution::commands::CancelResponse;
pub use crate::execution::commands::InvocationResponse;
pub use crate::execution::commands::ListInvocationsResponse;
pub use crate::execution::commands::ListResponse;
pub use crate::execution::commands::OutputsResponse;
pub use crate::execution::commands::StatusResponse;
pub use crate::execution::commands::SubmitResponse;

/// OpenAPI documentation for V1 API.
#[derive(OpenApi)]
#[openapi(
    paths(
        submit_run,
        get_run,
        list_runs,
        cancel_run,
        get_run_outputs,
        list_invocations,
        get_invocation,
    ),
    components(schemas(
        models::SubmitRunRequest,
        models::ListRunsQuery,
        models::ListInvocationsQuery,
        SubmitResponse,
        StatusResponse,
        ListResponse,
        CancelResponse,
        OutputsResponse,
        InvocationResponse,
        ListInvocationsResponse,
        crate::database::Run,
        crate::database::RunStatus,
        crate::database::Invocation,
        crate::database::InvocationMethod,
    )),
    tags(
        (name = "runs", description = "Run management endpoints"),
        (name = "invocations", description = "Invocation management endpoints")
    )
)]
pub struct ApiDoc;

/// Create the V1 API router.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/runs", post(submit_run).get(list_runs))
        .route("/runs/{id}", get(get_run))
        .route("/runs/{id}/cancel", post(cancel_run))
        .route("/runs/{id}/outputs", get(get_run_outputs))
        .route("/invocations", get(list_invocations))
        .route("/invocations/{id}", get(get_invocation))
        .with_state(state)
}
