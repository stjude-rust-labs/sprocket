//! Task API end-to-end tests.

use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use serde_json::Value;
use sprocket::Config;
use sprocket::ServerConfig;
use sprocket::server::AppState;
use sprocket::server::create_router;
use sprocket::system::v1::db::Database;
use sprocket::system::v1::db::LogSource;
use sprocket::system::v1::db::RunStatus;
use sprocket::system::v1::db::SprocketCommand;
use sprocket::system::v1::db::SqliteDatabase;
use sprocket::system::v1::exec::svc::RunManagerCmd;
use sprocket::system::v1::exec::svc::RunManagerSvc;
use tempfile::TempDir;
use tokio::sync::oneshot;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

/// Create a test server with real database and filesystem.
async fn create_test_server(pool: sqlx::SqlitePool) -> (axum::Router, Arc<dyn Database>, TempDir) {
    let temp = TempDir::new().unwrap();
    let wdl_dir = temp.path().join("wdl");
    std::fs::create_dir(&wdl_dir).unwrap();

    let mut server_config = ServerConfig {
        output_dir: temp.path().to_path_buf(),
        allowed_file_paths: vec![wdl_dir],
        ..Default::default()
    };
    server_config.validate().unwrap();

    let db = SqliteDatabase::from_pool(pool).await.unwrap();
    let db: Arc<dyn Database> = Arc::new(db);

    let (_, run_manager_tx) = RunManagerSvc::spawn(
        1000,
        Config {
            server: server_config,
            ..Default::default()
        },
        db.clone(),
    );

    let (tx, rx) = oneshot::channel();
    run_manager_tx
        .send(RunManagerCmd::Ping { rx: tx })
        .await
        .unwrap();
    rx.await.unwrap().unwrap();

    let state = AppState::builder().run_manager_tx(run_manager_tx).build();
    let router = create_router()
        .state(state)
        .cors_layer(CorsLayer::new())
        .call();

    (router, db, temp)
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

async fn seed_completed_task(db: &Arc<dyn Database>) -> Uuid {
    let session_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    db.create_session(session_id, SprocketCommand::Server, "tester")
        .await
        .unwrap();
    db.create_run(
        run_id,
        session_id,
        "run-name",
        "workflow.wdl",
        Some("workflow"),
        "{}",
    )
    .await
    .unwrap();
    db.update_run_status(run_id, RunStatus::Running)
        .await
        .unwrap();
    db.create_task("task-one", run_id).await.unwrap();
    db.update_task_started("task-one", now).await.unwrap();
    db.update_task_completed("task-one", Some(0), now)
        .await
        .unwrap();
    db.insert_task_log("task-one", LogSource::Stdout, b"hello")
        .await
        .unwrap();
    db.insert_task_log("task-one", LogSource::Stderr, b"warning")
        .await
        .unwrap();

    run_id
}

#[sqlx::test]
async fn list_tasks_returns_empty_initially(pool: sqlx::SqlitePool) {
    let (app, ..) = create_test_server(pool).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tasks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["total"], 0);
    assert!(json["tasks"].as_array().unwrap().is_empty());
}

#[sqlx::test]
async fn task_endpoints_return_seeded_task_and_logs(pool: sqlx::SqlitePool) {
    let (app, db, ..) = create_test_server(pool).await;
    let run_id = seed_completed_task(&db).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tasks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["total"], 1);
    assert_eq!(json["tasks"][0]["name"], "task-one");
    assert_eq!(json["tasks"][0]["run_uuid"], run_id.to_string());
    assert_eq!(json["tasks"][0]["status"], "completed");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/v1/tasks?run_uuid={run_id}&status=completed&limit=1"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["total"], 1);
    assert_eq!(json["tasks"].as_array().unwrap().len(), 1);
    assert!(json["next_token"].is_null());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tasks/task-one")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["name"], "task-one");
    assert_eq!(json["status"], "completed");
    assert_eq!(json["exit_status"], 0);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tasks/task-one/logs?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["total"], 2);
    assert_eq!(json["logs"].as_array().unwrap().len(), 1);
    assert_eq!(json["next_token"], "1");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tasks/task-one/logs?source=stdout")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["total"], 1);
    assert_eq!(json["logs"][0]["source"], "stdout");
    assert_eq!(
        json["logs"][0]["chunk"],
        serde_json::json!([104, 101, 108, 108, 111])
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tasks/task-one/logs?next_token=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["total"], 2);
    assert_eq!(json["logs"].as_array().unwrap().len(), 1);
    assert!(json["next_token"].is_null());
}

#[sqlx::test]
async fn task_endpoints_return_expected_errors(pool: sqlx::SqlitePool) {
    let (app, db, ..) = create_test_server(pool).await;
    seed_completed_task(&db).await;

    let cases = [
        ("/api/v1/tasks?next_token=bad", StatusCode::BAD_REQUEST),
        ("/api/v1/tasks?status=bad", StatusCode::BAD_REQUEST),
        (
            "/api/v1/tasks/task-one/logs?next_token=bad",
            StatusCode::BAD_REQUEST,
        ),
        (
            "/api/v1/tasks/task-one/logs?source=bad",
            StatusCode::BAD_REQUEST,
        ),
        ("/api/v1/tasks/missing-task", StatusCode::NOT_FOUND),
        ("/api/v1/tasks/missing-task/logs", StatusCode::NOT_FOUND),
    ];

    for (uri, status) in cases {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status, "unexpected status for `{uri}`");
    }
}
