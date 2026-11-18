//! The API server for executing runs (WDL tasks and workflows).

use anyhow::Context;
use anyhow::bail;
use axum::Router;
use axum::http::HeaderValue;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tower_http::LatencyUnit;
use tower_http::cors::CorsLayer;
use tower_http::trace::DefaultMakeSpan;
use tower_http::trace::DefaultOnRequest;
use tower_http::trace::DefaultOnResponse;
use tower_http::trace::TraceLayer;
use tracing::Level;
use tracing::info;
use tracing::warn;
use utoipa::OpenApi as _;
use utoipa_swagger_ui::SwaggerUi;
use wdl::diagnostics::Mode;

use crate::analysis::Source;
use crate::config::Config;
use crate::lock::file::LockFile;
use crate::system::v1::exec::open_database;
use crate::system::v1::exec::svc::RunManagerCmd;
use crate::system::v1::exec::svc::RunManagerSvc;

mod api;
mod shutdown;

pub use api::AppState;
pub(crate) use api::v1::RunStatus;
pub(crate) use api::v1::TaskStatus;
pub(crate) use api::v1::error::ErrorResponse;
pub use api::v1::info::ServerFailureMode;
pub(crate) use api::v1::info::ServerInfoResponse;
pub use api::v1::info::ServerState;
pub(crate) use api::v1::metrics::CallMetrics;
pub(crate) use api::v1::metrics::RunMetrics;
pub(crate) use api::v1::metrics::RunMetricsResponse;
// Only referenced by test code today; kept alongside its sibling re-exports.
#[allow(unused_imports)]
pub(crate) use api::v1::metrics::RunMetricsTotals;
pub(crate) use api::v1::metrics::TaskAttemptMetrics;
pub(crate) use api::v1::metrics::TransferTotals;
pub(crate) use api::v1::metrics::build_run_metrics;
pub use api::v1::paths;
pub(crate) use api::v1::runs::CancelRunResponse;
pub(crate) use api::v1::runs::ListRunsResponse;
pub(crate) use api::v1::runs::Run;
pub(crate) use api::v1::runs::RunResponse;
pub(crate) use api::v1::runs::SubmitRunRequest;
pub(crate) use api::v1::tasks::ListTasksResponse;
pub(crate) use api::v1::tasks::RunTaskCountsResponse;
pub(crate) use api::v1::tasks::Task;
use shutdown::ShutdownController;
use shutdown::ShutdownStep;

use crate::commands::CommandOutput;

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
            SwaggerUi::new(paths::SWAGGER_UI).url(paths::OPENAPI_JSON, api::v1::ApiDoc::openapi()),
        )
        .nest("/api", api::create_router(state))
        .layer(cors_layer)
        .layer(trace_layer)
}

/// A built server and what shutting it down needs.
struct ServerApp {
    /// The router to serve.
    router: Router,
    /// Steps the server through shutdown.
    shutdown: ShutdownController,
    /// The run manager's task.
    run_manager: JoinHandle<()>,
    /// Sends commands to the run manager.
    run_manager_tx: mpsc::Sender<RunManagerCmd>,
}

/// Build the Sprocket server.
///
/// # Errors
///
/// Returns an error if we fail to initialize the database.
async fn create_server_app(
    config: Config,
    report_mode: Mode,
    output: CommandOutput,
) -> anyhow::Result<ServerApp> {
    let lock_file = LockFile::locate(&Source::default(), report_mode, output)?.unwrap_or_default();

    let db_path = config
        .server
        .database
        .resolve_url(&config.server.engine.output_dir);

    let db = open_database(&db_path).await?;
    let failure_mode = ServerFailureMode::from(config.server.engine.failure_mode);
    // Resolve the output directory to an absolute path so clients (e.g. `dev
    // server inspect`) can join it with a run-relative path to produce a
    // usable, copy-pasteable filesystem path, regardless of the server
    // process's working directory or whether the configured path was
    // relative (e.g. `./out`).
    let output_dir = std::path::absolute(&config.server.engine.output_dir)
        .unwrap_or_else(|_| config.server.engine.output_dir.clone())
        .display()
        .to_string();
    let shutdown = ShutdownController::new(config.server.engine.failure_mode);
    let (run_manager, run_manager_tx) = RunManagerSvc::spawn(
        DEFAULT_CHANNEL_BUFFER_SIZE,
        config.clone(),
        report_mode,
        output.colorize(),
        db.clone(),
        lock_file.as_container_overrides().unwrap_or_default(),
        shutdown.context().clone(),
    )
    .await?;

    let state = AppState::builder()
        .run_manager_tx(run_manager_tx.clone())
        .database(db)
        .failure_mode(failure_mode)
        .output_dir(output_dir)
        .shutdown(shutdown.context().clone())
        .build();

    let mut cors_layer = CorsLayer::new();
    for origin in config.server.allowed_origins {
        let header = origin
            .parse::<HeaderValue>()
            .with_context(|| format!("invalid CORS origin: `{}`", origin))?;

        cors_layer = cors_layer.allow_origin(header);
    }

    Ok(ServerApp {
        router: create_router().state(state).cors_layer(cors_layer).call(),
        shutdown,
        run_manager,
        run_manager_tx,
    })
}

/// Run the server with a pre-defined listener.
///
/// # Errors
///
/// Returns an error if the server fails to start.
pub async fn run_with_listener(
    config: Config,
    report_mode: Mode,
    output: CommandOutput,
    tcp_listener: TcpListener,
) -> anyhow::Result<()> {
    let signals = forward_signals().context("failed to install shutdown signal handlers")?;
    // Boxed: the server future is large and would overflow the main thread's
    // stack on Windows.
    Box::pin(serve(config, report_mode, output, tcp_listener, signals)).await
}

/// Sends a message on the returned channel for each SIGINT or SIGTERM.
#[cfg(unix)]
fn forward_signals() -> std::io::Result<mpsc::Receiver<()>> {
    use tokio::signal::unix::SignalKind;
    use tokio::signal::unix::signal;

    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let (tx, rx) = mpsc::channel(1);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = interrupt.recv() => {}
                _ = terminate.recv() => {}
            }
            if tx.send(()).await.is_err() {
                break;
            }
        }
    });
    Ok(rx)
}

/// Sends a message on the returned channel for each Ctrl+C.
#[cfg(not(unix))]
fn forward_signals() -> std::io::Result<mpsc::Receiver<()>> {
    let (tx, rx) = mpsc::channel(1);
    tokio::spawn(async move {
        while tokio::signal::ctrl_c().await.is_ok() {
            if tx.send(()).await.is_err() {
                break;
            }
        }
    });
    Ok(rx)
}

/// Serves until shutdown finishes, with each message on `signals` acting as a
/// shutdown signal.
///
/// The first signal stops new runs. What each signal does depends on the
/// failure mode; see [`ShutdownController::signal`]. Returns an error if a
/// signal forces an exit before runs finish.
async fn serve(
    config: Config,
    report_mode: Mode,
    output: CommandOutput,
    listener: TcpListener,
    mut signals: mpsc::Receiver<()>,
) -> anyhow::Result<()> {
    let timeout = config.server.shutdown_timeout();
    let ServerApp {
        router,
        shutdown,
        run_manager,
        run_manager_tx,
    } = create_server_app(config, report_mode, output).await?;

    let drained = {
        let started = shutdown.context().first().clone();
        let tx = run_manager_tx.clone();
        async move {
            started.cancelled().await;
            wait_for_runs(&tx).await;
        }
    };
    let server = axum::serve(listener, router)
        .with_graceful_shutdown(drained)
        .into_future();
    tokio::pin!(server);

    loop {
        tokio::select! {
            result = &mut server => {
                result?;
                break;
            }
            Some(()) = signals.recv() => match shutdown.signal() {
                ShutdownStep::Drain => {
                    warn!(
                        "shutting down: waiting for running tasks to finish; send the signal \
                         again to cancel them"
                    );
                    if let Some(timeout) = timeout {
                        shutdown.spawn_deadline(timeout);
                    }
                }
                ShutdownStep::Cancel => {
                    warn!("shutting down: canceling running tasks; send the signal again to exit now");
                }
                ShutdownStep::Exit => {
                    bail!("shutdown forced: runs still in progress will be marked `orphaned`");
                }
            },
        }
    }

    let (tx, rx) = oneshot::channel();
    if run_manager_tx
        .send(RunManagerCmd::Shutdown { rx: tx })
        .await
        .is_ok()
    {
        let _ = rx.await;
    }
    let _ = run_manager.await;
    info!("server shut down");
    Ok(())
}

/// Waits for every submitted run to finish.
async fn wait_for_runs(tx: &mpsc::Sender<RunManagerCmd>) {
    let (rx_tx, rx) = oneshot::channel();
    if tx.send(RunManagerCmd::Drain { rx: rx_tx }).await.is_ok()
        && let Ok(tracker) = rx.await
    {
        tracker.wait().await;
    }
}

/// Run the server.
///
/// # Errors
///
/// Returns an error if the server fails to start or bind to the address.
pub async fn run(config: Config, report_mode: Mode, output: CommandOutput) -> anyhow::Result<()> {
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("server listening on {}", addr);
    run_with_listener(config, report_mode, output, listener).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::body::Body;
    use axum::http::Request;
    use axum::http::StatusCode;
    use serde_json::Value;
    use tower::ServiceExt;
    use wdl::engine::config::BackendConfig;
    use wdl::engine::config::FailureMode;

    use super::*;
    use crate::system::v1::db::RunStatus;
    use crate::system::v1::db::SqliteDatabase;

    #[tokio::test]
    async fn router_serves_openapi_and_nested_api_routes() -> anyhow::Result<()> {
        let (run_manager_tx, _run_manager_rx) = mpsc::channel::<RunManagerCmd>(1);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        let database = std::sync::Arc::new(SqliteDatabase::from_pool(pool).await?);
        let state = AppState::builder()
            .run_manager_tx(run_manager_tx)
            .database(database)
            .failure_mode(ServerFailureMode::Slow)
            .output_dir(String::new())
            .build();
        let app = create_router()
            .state(state)
            .cors_layer(CorsLayer::new())
            .call();

        let request = Request::builder()
            .uri("/api/v1/openapi.json")
            .body(Body::empty())?;
        let response = app.clone().oneshot(request).await?;
        assert_eq!(response.status(), StatusCode::OK);

        let request = Request::builder().uri("/api/v1/nope").body(Body::empty())?;
        let response = app.oneshot(request).await?;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        Ok(())
    }

    /// Sends a request and parses the JSON response body.
    async fn json(request: reqwest::RequestBuilder) -> anyhow::Result<Value> {
        let bytes = request.send().await?.error_for_status()?.bytes().await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    #[tokio::test]
    async fn signals_drain_then_cancel_runs() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let dir = temp.path().canonicalize()?;
        let source = dir.join("wait.wdl");
        std::fs::write(
            &source,
            "version 1.2\n\ntask wait {\n    command <<< sleep 600 >>>\n}\n",
        )?;

        let mut config = Config::default();
        config.server.engine.output_dir = dir.join("runs");
        config.server.engine.failure_mode = FailureMode::Slow;
        config.server.allowed_file_paths = vec![dir.clone()];
        config.server.engine.backend = "local".to_string();
        config.server.engine.backends.insert(
            "local".to_string(),
            BackendConfig::Local {
                config: Default::default(),
            },
        );
        let db_url = config
            .server
            .database
            .resolve_url(&config.server.engine.output_dir);

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let base = format!("http://{}", listener.local_addr()?);
        let (signal, signals) = mpsc::channel(1);
        let server = tokio::spawn(serve(
            config,
            Mode::default(),
            CommandOutput::new(false),
            listener,
            signals,
        ));

        let client = reqwest::Client::new();
        let submit = || {
            client
                .post(format!("{base}{}", paths::SUBMIT_RUN))
                .header("content-type", "application/json")
                .body(
                    serde_json::json!({ "source": source, "inputs": {}, "target": "wait" })
                        .to_string(),
                )
        };
        let run = json(submit()).await?;
        let id: uuid::Uuid = run["uuid"].as_str().unwrap().parse()?;
        let tasks_url = format!("{base}{}?status=running", paths::list_run_tasks(id));
        let info_url = format!("{base}{}", paths::SERVER_INFO);

        // Polls `url` until `field` equals `want`.
        let wait_for = |url: String, field: &'static str, want: &'static str| {
            let client = client.clone();
            async move {
                tokio::time::timeout(Duration::from_secs(30), async {
                    while json(client.get(&url)).await?[field] != want {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    anyhow::Ok(())
                })
                .await?
            }
        };

        // Wait for the task's command to start; a signal before then cancels
        // the run at once.
        tokio::time::timeout(Duration::from_secs(30), async {
            while json(client.get(&tasks_url)).await?["total"] == 0 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            anyhow::Ok(())
        })
        .await??;

        signal.send(()).await?;
        wait_for(info_url, "state", "draining").await?;
        let response = submit().send().await?;
        assert_eq!(response.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);

        signal.send(()).await?;
        tokio::time::timeout(Duration::from_secs(60), server).await???;

        let db = crate::system::v1::exec::open_database(&db_url).await?;
        let run = db.get_run(id).await?.expect("run not found");
        assert_eq!(run.status, RunStatus::Canceled, "{:?}", run.error);
        Ok(())
    }
}
