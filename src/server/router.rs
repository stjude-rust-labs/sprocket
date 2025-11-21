//! Server setup and routing.

use std::sync::Arc;

use anyhow::Context;
use axum::Router;
use axum::http::HeaderValue;
use tower_http::LatencyUnit;
use tower_http::cors::CorsLayer;
use tower_http::trace::DefaultMakeSpan;
use tower_http::trace::DefaultOnRequest;
use tower_http::trace::DefaultOnResponse;
use tower_http::trace::TraceLayer;
use tracing::Level;
use utoipa::OpenApi as _;
use utoipa_swagger_ui::SwaggerUi;

use super::api::AppState;
use crate::config::ServerConfig;
use crate::database::SqliteDatabase;
use crate::execution::ExecutionConfig;
use crate::execution::spawn_manager;
use crate::server::api;

/// Channel capacity for events.
///
/// A reasonable buffer size to handle burst event production without blocking
/// event emitters while consumers process events.
const EVENTS_CHANNEL_CAPACITY: usize = 100;

/// Create the application router.
#[bon::builder]
pub fn create_router(state: AppState, cors: CorsLayer) -> Router {
    let trace = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_request(DefaultOnRequest::new().level(Level::INFO))
        .on_response(
            DefaultOnResponse::new()
                .level(Level::INFO)
                .latency_unit(LatencyUnit::Millis),
        );

    Router::new()
        .merge(
            SwaggerUi::new("/api/v1/swagger-ui")
                .url("/api/v1/openapi.json", api::v1::ApiDoc::openapi()),
        )
        .nest("/api", super::api::create_router(state))
        .layer(cors)
        .layer(trace)
}

/// Run the server.
///
/// # Errors
///
/// Returns an error if the server fails to start or bind to the address.
pub async fn run(
    server_config: ServerConfig,
    execution_config: ExecutionConfig,
) -> anyhow::Result<()> {
    let db_path = server_config.database.url.clone().unwrap_or_else(|| {
        execution_config
            .output_directory
            .join(crate::config::DEFAULT_DATABASE_FILENAME)
            .display()
            .to_string()
    });

    let db = Arc::new(SqliteDatabase::new(&db_path).await?);
    let events = wdl::engine::Events::all(EVENTS_CHANNEL_CAPACITY);
    let manager = spawn_manager(execution_config, db, events);

    let state = AppState { manager };

    let mut cors = CorsLayer::new();
    for origin in server_config.allowed_origins {
        let header = origin
            .parse::<HeaderValue>()
            .with_context(|| format!("invalid CORS origin: `{}`", origin))?;

        cors = cors.allow_origin(header);
    }

    let app = create_router().state(state).cors(cors).call();

    let addr = format!("{}:{}", server_config.host, server_config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("server listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
