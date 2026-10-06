//! Direct database read tests.

use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use sprocket::server::AppState;
use sprocket::server::ServerFailureMode;
use sprocket::server::create_router;
use sprocket::server::paths;
use sprocket::system::v1::db::Database;
use sprocket::system::v1::db::LogSource;
use sprocket::system::v1::db::SprocketCommand;
use sprocket::system::v1::db::SqliteDatabase;
use sprocket::system::v1::db::TaskStatus;
use sprocket::system::v1::exec::svc::RunManagerCmd;
use tokio::sync::mpsc;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

async fn assert_ok(app: &axum::Router, uri: impl AsRef<str>) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri.as_ref())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{}", uri.as_ref());
}

#[sqlx::test]
#[cfg_attr(docker_tests_disabled, ignore = "Docker tests are disabled")]
async fn direct_reads_bypass_manager(pool: sqlx::SqlitePool) {
    let database: Arc<dyn Database> = Arc::new(SqliteDatabase::from_pool(pool).await.unwrap());
    let session_id = Uuid::new_v4();
    database
        .create_session(session_id, SprocketCommand::Server, "tester")
        .await
        .unwrap();

    let run_id = Uuid::new_v4();
    database
        .create_run(
            run_id,
            session_id,
            "test-run",
            "workflow.wdl",
            Some("workflow"),
            "{}",
        )
        .await
        .unwrap();
    database
        .update_run_outputs(run_id, r#"{"answer":42}"#)
        .await
        .unwrap();
    database
        .create_task("test-task", run_id, TaskStatus::Running)
        .await
        .unwrap();
    database
        .insert_task_log("test-task", LogSource::Stdout, b"hello")
        .await
        .unwrap();

    let (run_manager_tx, run_manager_rx) = mpsc::channel::<RunManagerCmd>(1);
    drop(run_manager_rx);
    assert!(run_manager_tx.is_closed());

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

    for uri in [
        paths::LIST_RUNS.to_string(),
        paths::get_run(run_id),
        paths::get_run_outputs(run_id),
        paths::LIST_SESSIONS.to_string(),
        paths::get_session(session_id),
        paths::LIST_TASKS.to_string(),
        paths::list_run_tasks(run_id),
        paths::run_task_counts(run_id),
        paths::get_task("test-task"),
        paths::get_task_logs("test-task"),
    ] {
        assert_ok(&app, uri).await;
    }
}
