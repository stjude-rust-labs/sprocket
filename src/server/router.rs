//! Server setup and routing.

use axum::routing::get;
use axum::routing::post;
use axum::Router;
use tower_http::cors::CorsLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use super::api::workflows::*;
use super::api::AppState;
use super::config::Config;
use super::db::Database;
use super::manager::spawn_manager;

/// OpenAPI documentation.
#[derive(OpenApi)]
#[openapi(
    paths(
        submit_workflow,
        get_workflow,
        list_workflows,
        cancel_workflow,
        get_workflow_outputs,
        get_workflow_logs,
    ),
    components(schemas(
        super::api::models::SubmitWorkflowRequest,
        super::api::models::SubmitWorkflowResponse,
        super::api::models::WdlSourceRequest,
        super::api::models::GetWorkflowResponse,
        super::api::models::ListWorkflowsQuery,
        super::api::models::ListWorkflowsResponse,
        super::api::models::CancelWorkflowResponse,
        super::api::models::GetWorkflowOutputsResponse,
        super::api::models::GetWorkflowLogsQuery,
        super::api::models::GetWorkflowLogsResponse,
        super::db::WorkflowRow,
        super::db::WorkflowStatus,
        super::db::WdlSourceType,
    )),
    tags(
        (name = "workflows", description = "Workflow management endpoints")
    )
)]
struct ApiDoc;

/// Create the application router.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .route("/workflows", post(submit_workflow).get(list_workflows))
        .route("/workflows/{id}", get(get_workflow))
        .route("/workflows/{id}/cancel", post(cancel_workflow))
        .route("/workflows/{id}/outputs", get(get_workflow_outputs))
        .route("/workflows/{id}/logs", get(get_workflow_logs))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Run the server.
///
/// # Errors
///
/// Returns an error if the server fails to start or bind to the address.
pub async fn run(config: Config) -> anyhow::Result<()> {
    let db = Database::new(
        config.database.url.as_str(),
        config.database.max_connections,
    )
    .await?;

    let manager = spawn_manager(config.clone(), db);

    let state = AppState { manager };

    let app = create_router(state);

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("server listening on `{}`", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
