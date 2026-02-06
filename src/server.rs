//! The API server for executing runs (WDL tasks and workflows).

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

use crate::config::ServerConfig;
use crate::system::v1::exec::open_database;
use crate::system::v1::exec::svc::RunManagerSvc;

mod api;

pub use api::AppState;

/// The default channel buffer size.
///
/// A reasonably large, arbitrary number for buffering commands to the run
/// manager.
const DEFAULT_CHANNEL_BUFFER_SIZE: usize = 2048;

/// Create the default application router for Axum.
#[bon::builder]
pub fn create_router(state: AppState, cors_layer: CorsLayer) -> Router {
    let trace_layer = TraceLayer::new_for_http()
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
        .nest("/api", api::create_router(state))
        .layer(cors_layer)
        .layer(trace_layer)
}

/// Run the server.
///
/// # Errors
///
/// Returns an error if the server fails to start or bind to the address.
pub async fn run(config: ServerConfig) -> anyhow::Result<()> {
    let db_path = config.database.url.clone().unwrap_or_else(|| {
        config
            .output_directory
            .join(crate::config::DEFAULT_DATABASE_FILENAME)
            .display()
            .to_string()
    });

    let db = open_database(&db_path).await?;
    let (_, run_manager_tx) = RunManagerSvc::spawn(DEFAULT_CHANNEL_BUFFER_SIZE, config.clone(), db);

    let state = AppState::builder().run_manager_tx(run_manager_tx).build();

    let mut cors_layer = CorsLayer::new();
    for origin in config.allowed_origins {
        let header = origin
            .parse::<HeaderValue>()
            .with_context(|| format!("invalid CORS origin: `{}`", origin))?;

        cors_layer = cors_layer.allow_origin(header);
    }

    let app = create_router().state(state).cors_layer(cors_layer).call();

    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("server listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
