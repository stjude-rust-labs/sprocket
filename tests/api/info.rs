//! Server info API end-to-end tests.

use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use sprocket::Config;
use sprocket::ServerConfig;
use sprocket::server::AppState;
use sprocket::server::ServerFailureMode;
use sprocket::server::create_router;
use sprocket::server::paths;
use sprocket::system::v1::db::Database;
use sprocket::system::v1::db::SqliteDatabase;
use sprocket::system::v1::exec::svc::RunManagerCmd;
use sprocket::system::v1::exec::svc::RunManagerSvc;
use tempfile::TempDir;
use tokio::sync::oneshot;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;
use wdl::diagnostics::Mode;

/// Create a test server whose `AppState` reports the given failure mode.
///
/// The returned router is exercised by `oneshot` requests; the temp dir is
/// retained for the duration of the test. The returned `output_dir` string
/// matches what the `AppState` was configured with.
async fn create_test_server(
    pool: sqlx::SqlitePool,
    failure_mode: ServerFailureMode,
) -> (axum::Router, TempDir, String) {
    let temp = TempDir::new().unwrap();

    let mut server_config = ServerConfig {
        output_dir: temp.path().to_path_buf(),
        ..Default::default()
    };
    server_config.validate().unwrap();
    let output_dir = server_config.output_dir.display().to_string();

    let db = SqliteDatabase::from_pool(pool).await.unwrap();
    let db: Arc<dyn Database> = Arc::new(db);

    let (_, run_manager_tx) = RunManagerSvc::spawn(
        1000,
        Config {
            server: server_config,
            ..Default::default()
        },
        Mode::default(),
        true,
        db,
    );

    // Wait for the manager to be ready.
    let (tx, rx) = oneshot::channel();
    run_manager_tx
        .send(RunManagerCmd::Ping { rx: tx })
        .await
        .unwrap();
    rx.await.unwrap().unwrap();

    let state = AppState::builder()
        .run_manager_tx(run_manager_tx)
        .failure_mode(failure_mode)
        .output_dir(output_dir.clone())
        .build();
    let router = create_router()
        .state(state)
        .cors_layer(CorsLayer::new())
        .call();

    (router, temp, output_dir)
}

/// Sends a `GET /api/v1/info` request and returns the response status and
/// parsed JSON body.
async fn get_info(app: &axum::Router) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(paths::SERVER_INFO)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, json)
}

#[sqlx::test]
async fn info_returns_configured_slow_mode(pool: sqlx::SqlitePool) {
    let (app, _temp, output_dir) = create_test_server(pool, ServerFailureMode::Slow).await;

    let (status, body) = get_info(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["failure_mode"], "slow");
    assert_eq!(body["output_dir"], output_dir);
}

#[sqlx::test]
async fn info_returns_configured_fast_mode(pool: sqlx::SqlitePool) {
    let (app, _temp, output_dir) = create_test_server(pool, ServerFailureMode::Fast).await;

    let (status, body) = get_info(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["failure_mode"], "fast");
    assert_eq!(body["output_dir"], output_dir);
}
