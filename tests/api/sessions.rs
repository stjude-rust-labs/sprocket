//! Session API end-to-end tests.

use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use serde_json::json;
use sprocket::system::v1::db::Database;
use sprocket::system::v1::db::SqliteDatabase;
use sprocket::system::v1::exec::ExecutionConfig;
use sprocket::system::v1::exec::svc::RunManagerCmd;
use sprocket::system::v1::exec::svc::RunManagerSvc;
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

    let (_, run_manager_tx) = RunManagerSvc::spawn(1000, exec_config, db.clone());

    // Wait for manager to be ready
    let (tx, rx) = oneshot::channel();
    run_manager_tx.send(RunManagerCmd::Ping { rx: tx }).await.unwrap();
    rx.await.unwrap().unwrap();

    let state = AppState::builder().run_manager_tx(run_manager_tx).build();
    let router = create_router().state(state).cors_layer(CorsLayer::new()).call();

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
async fn list_sessions_returns_empty_initially(pool: sqlx::SqlitePool) {
    let (app, ..) = create_test_server().pool(pool).call().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/sessions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["sessions"].is_array());
    assert_eq!(
        json["sessions"].as_array().unwrap().len(),
        0,
        "should have no sessions initially"
    );
}

#[sqlx::test]
async fn get_session_after_workflow_submission(pool: sqlx::SqlitePool) {
    let (app, _, temp) = create_test_server().pool(pool).call().await;

    // Write WDL to a file
    let wdl_file = temp.path().join("wdl").join("test.wdl");
    std::fs::write(&wdl_file, SIMPLE_WORKFLOW).unwrap();

    // Submit a workflow to create an session
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

    // Now list sessions
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/sessions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let list_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        list_json["sessions"].as_array().unwrap().len(),
        1,
        "should have one session after workflow submission"
    );

    let session_id = list_json["sessions"][0]["id"].as_str().unwrap();

    // Get the session by ID
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/sessions/{}", session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let session_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(session_json["id"].as_str().unwrap(), session_id);
    assert_eq!(session_json["subcommand"].as_str().unwrap(), "server");
    assert!(session_json["created_by"].is_string());
    assert!(session_json["created_at"].is_string());
}

#[sqlx::test]
async fn get_nonexistent_session_returns_404(pool: sqlx::SqlitePool) {
    let (app, ..) = create_test_server().pool(pool).call().await;

    let fake_id = "00000000-0000-0000-0000-000000000000";

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/sessions/{}", fake_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn list_sessions_with_pagination(pool: sqlx::SqlitePool) {
    let (app, _, temp) = create_test_server().pool(pool).call().await;

    // Write WDL to a file
    let wdl_file = temp.path().join("wdl").join("test.wdl");
    std::fs::write(&wdl_file, SIMPLE_WORKFLOW).unwrap();

    // Submit a workflow to create an session
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
                .uri("/api/v1/sessions?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["sessions"].as_array().unwrap().len(), 1);

    // List with offset=1 should return empty
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/sessions?offset=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        json["sessions"].as_array().unwrap().len(),
        0,
        "offset beyond available items should return empty"
    );
}
