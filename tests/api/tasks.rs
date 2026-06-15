//! Task API end-to-end tests.

use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use chrono::Utc;
use http_body_util::BodyExt;
use sprocket::Config;
use sprocket::ServerConfig;
use sprocket::server::AppState;
use sprocket::server::create_router;
use sprocket::system::v1::db::Database;
use sprocket::system::v1::db::SprocketCommand;
use sprocket::system::v1::db::SqliteDatabase;
use sprocket::system::v1::exec::svc::RunManagerCmd;
use sprocket::system::v1::exec::svc::RunManagerSvc;
use tempfile::TempDir;
use tokio::sync::oneshot;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

/// Create a test server with a real database and filesystem.
async fn create_test_server(pool: sqlx::SqlitePool) -> (axum::Router, Arc<dyn Database>, TempDir) {
    let temp = TempDir::new().unwrap();

    let mut server_config = ServerConfig {
        output_dir: temp.path().to_path_buf(),
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

    // Wait for the manager to be ready.
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

/// Seed a run with the given name and return its UUID.
async fn seed_run(db: &Arc<dyn Database>, session_id: Uuid, name: &str) -> Uuid {
    let run_id = Uuid::new_v4();
    db.create_run(run_id, session_id, name, "test.wdl", Some("wf"), "{}")
        .await
        .unwrap();
    run_id
}

#[sqlx::test]
async fn run_task_counts_groups_by_status(pool: sqlx::SqlitePool) {
    let (app, db, _temp) = create_test_server(pool).await;

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Server, "test-user")
        .await
        .unwrap();

    let run_id = seed_run(&db, session_id, "counts-run").await;

    // Two pending (left as created), one running, one completed, one failed,
    // one canceled. No preempted tasks.
    for name in ["t1", "t2", "t3", "t4", "t5", "t6"] {
        db.create_task(name, run_id).await.unwrap();
    }
    assert!(db.update_task_started("t3", Utc::now()).await.unwrap());
    assert!(
        db.update_task_completed("t4", Some(0), Utc::now())
            .await
            .unwrap()
    );
    assert!(
        db.update_task_failed("t5", "boom", Utc::now())
            .await
            .unwrap()
    );
    assert!(db.update_task_canceled("t6", Utc::now()).await.unwrap());

    // A task on a different run must not be counted.
    let other_run_id = seed_run(&db, session_id, "other-run").await;
    db.create_task("other", other_run_id).await.unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/runs/{}/tasks/counts", run_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let counts: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(counts["pending"], 2);
    assert_eq!(counts["running"], 1);
    assert_eq!(counts["completed"], 1);
    assert_eq!(counts["failed"], 1);
    assert_eq!(counts["canceled"], 1);
    assert_eq!(counts["preempted"], 0);
    assert_eq!(counts["total"], 6);
}

#[sqlx::test]
async fn run_task_counts_unknown_run_is_all_zero(pool: sqlx::SqlitePool) {
    let (app, _db, _temp) = create_test_server(pool).await;

    let unknown = Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/runs/{}/tasks/counts", unknown))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Mirrors `GET /runs/{id}/tasks`: unknown runs are not an error.
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let counts: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(counts["pending"], 0);
    assert_eq!(counts["running"], 0);
    assert_eq!(counts["completed"], 0);
    assert_eq!(counts["failed"], 0);
    assert_eq!(counts["canceled"], 0);
    assert_eq!(counts["preempted"], 0);
    assert_eq!(counts["total"], 0);
}
