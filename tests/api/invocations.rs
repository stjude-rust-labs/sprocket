//! Invocation API end-to-end tests.

use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use serde_json::json;
use sprocket::database::Database;
use sprocket::database::SqliteDatabase;
use sprocket::execution::ExecutionConfig;
use sprocket::execution::ManagerCommand;
use sprocket::execution::spawn_manager;
use sprocket::server::AppState;
use sprocket::server::create_router;
use tempfile::TempDir;
use tokio::sync::oneshot;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

/// Create a test server with real database and filesystem.
#[bon::builder]
async fn create_test_server(
    pool: sqlx::SqlitePool,
    max_concurrent_runs: Option<usize>,
) -> (axum::Router, Arc<dyn Database>, TempDir) {
    let temp = TempDir::new().unwrap();

    // Create a directory for WDL files and allow it
    let wdl_dir = temp.path().join("wdl");
    std::fs::create_dir(&wdl_dir).unwrap();

    let mut exec_config = ExecutionConfig::builder()
        .output_directory(temp.path().to_path_buf())
        .allowed_file_paths(vec![wdl_dir])
        .maybe_max_concurrent_runs(max_concurrent_runs)
        .build();
    exec_config.validate().unwrap();

    let db = SqliteDatabase::from_pool(pool).await.unwrap();
    let db: Arc<dyn Database> = Arc::new(db);

    let events = wdl::engine::Events::all(100);
    let manager = spawn_manager(exec_config, db.clone(), events);

    // Wait for manager to be ready
    let (tx, rx) = oneshot::channel();
    manager.send(ManagerCommand::Ping { rx: tx }).await.unwrap();
    rx.await.unwrap().unwrap();

    let state = AppState { manager };

    let router = create_router().state(state).cors(CorsLayer::new()).call();

    (router, db, temp)
}

/// Simple WDL workflow for testing.
const SIMPLE_WORKFLOW: &str = r#"
version 1.2

workflow test {
    output {
        String message = "hello world"
    }
}
"#;

#[sqlx::test]
async fn list_invocations_returns_empty_initially(pool: sqlx::SqlitePool) {
    let (app, ..) = create_test_server().pool(pool).call().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/invocations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["invocations"].is_array());
    assert_eq!(
        json["invocations"].as_array().unwrap().len(),
        0,
        "should have no invocations initially"
    );
}

#[sqlx::test]
async fn get_invocation_after_workflow_submission(pool: sqlx::SqlitePool) {
    let (app, _, temp) = create_test_server().pool(pool).call().await;

    // Write WDL to a file
    let wdl_file = temp.path().join("wdl").join("test.wdl");
    std::fs::write(&wdl_file, SIMPLE_WORKFLOW).unwrap();

    // Submit a workflow to create an invocation
    let submit_request = json!({
        "source": wdl_file.to_str().unwrap(),
        "inputs": {},
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&submit_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Now list invocations
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/invocations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let list_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        list_json["invocations"].as_array().unwrap().len(),
        1,
        "should have one invocation after workflow submission"
    );

    let invocation_id = list_json["invocations"][0]["id"].as_str().unwrap();

    // Get the invocation by ID
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/invocations/{}", invocation_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let invocation_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(invocation_json["id"].as_str().unwrap(), invocation_id);
    assert_eq!(invocation_json["method"].as_str().unwrap(), "server");
    assert!(invocation_json["created_by"].is_string());
    assert!(invocation_json["created_at"].is_string());
}

#[sqlx::test]
async fn get_nonexistent_invocation_returns_404(pool: sqlx::SqlitePool) {
    let (app, ..) = create_test_server().pool(pool).call().await;

    let fake_id = "00000000-0000-0000-0000-000000000000";

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/invocations/{}", fake_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn list_invocations_with_pagination(pool: sqlx::SqlitePool) {
    let (app, _, temp) = create_test_server().pool(pool).call().await;

    // Write WDL to a file
    let wdl_file = temp.path().join("wdl").join("test.wdl");
    std::fs::write(&wdl_file, SIMPLE_WORKFLOW).unwrap();

    // Submit a workflow to create an invocation
    let submit_request = json!({
        "source": wdl_file.to_str().unwrap(),
        "inputs": {},
    });

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&submit_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // List with limit=1
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/invocations?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["invocations"].as_array().unwrap().len(), 1);

    // List with offset=1 should return empty
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/invocations?offset=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        json["invocations"].as_array().unwrap().len(),
        0,
        "offset beyond available items should return empty"
    );
}
