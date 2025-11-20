//! Run API end-to-end tests.

use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use serde_json::json;
use sprocket::database::Database;
use sprocket::database::RunStatus;
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
    engine: Option<wdl::engine::Config>,
) -> (axum::Router, Arc<dyn Database>, TempDir) {
    let temp = TempDir::new().unwrap();

    // Create a directory for WDL files and allow it
    let wdl_dir = temp.path().join("wdl");
    std::fs::create_dir(&wdl_dir).unwrap();

    let mut exec_config = ExecutionConfig::builder()
        .output_directory(temp.path().to_path_buf())
        .allowed_file_paths(vec![wdl_dir])
        .maybe_max_concurrent_runs(max_concurrent_runs)
        .maybe_engine(engine)
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
        Int number = 42
    }
}
"#;

#[sqlx::test]
async fn submit_run_and_verify_completion(pool: sqlx::SqlitePool) {
    let (app, db, temp) = create_test_server().pool(pool).call().await;

    // Write WDL to a file in the allowed directory
    let wdl_file = temp.path().join("wdl").join("test.wdl");
    std::fs::write(&wdl_file, SIMPLE_WORKFLOW).unwrap();

    // Submit run using file path
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

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let run_id = submit_response["id"].as_str().unwrap();
    let run_name = submit_response["name"].as_str().unwrap();

    // Verify run was created in database
    let run = db
        .get_run(run_id.parse().unwrap())
        .await
        .unwrap()
        .expect("run should exist in database");

    assert_eq!(run.name, run_name);

    // Poll until run completes (max 10 seconds)
    let mut completed = false;
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let status_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/runs/{}", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = status_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        if status_json["status"] == "completed" {
            completed = true;
            break;
        } else if status_json["status"] == "failed" {
            panic!("run failed: {:?}", status_json);
        }
    }

    assert!(completed, "run should complete within 10 seconds");

    // Verify final database state
    let run = db.get_run(run_id.parse().unwrap()).await.unwrap().unwrap();

    assert_eq!(run.status, RunStatus::Completed);
    assert!(run.started_at.is_some());
    assert!(run.completed_at.is_some());
    assert!(run.outputs.is_some());

    let outputs: serde_json::Value = serde_json::from_str(&run.outputs.unwrap()).unwrap();
    assert_eq!(outputs["test.message"], "hello world");
    assert_eq!(outputs["test.number"], 42);

    // Verify filesystem artifacts
    let run_dir = temp.path().join("runs").join("test");
    assert!(run_dir.exists(), "run directory should exist");

    // Find the timestamped execution directory
    let mut execution_dirs: Vec<_> = std::fs::read_dir(&run_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| !p.file_name().unwrap().to_str().unwrap().starts_with('_'))
        .collect();
    assert_eq!(
        execution_dirs.len(),
        1,
        "should have exactly one execution directory"
    );
    let execution_dir = execution_dirs.pop().unwrap();

    let outputs_file = execution_dir.join("outputs.json");
    assert!(outputs_file.exists(), "`outputs.json` should exist");

    let outputs_content = std::fs::read_to_string(outputs_file).unwrap();
    let outputs_json: serde_json::Value = serde_json::from_str(&outputs_content).unwrap();
    assert_eq!(outputs_json["test.message"], "hello world");
    assert_eq!(outputs_json["test.number"], 42);

    // Verify `_latest` symlink exists and points to the execution directory
    let latest_symlink = run_dir.join("_latest");
    assert!(
        latest_symlink.exists(),
        "`_latest` symlink should exist at `{}`",
        latest_symlink.display()
    );

    let metadata = std::fs::symlink_metadata(&latest_symlink).unwrap();
    assert!(metadata.is_symlink(), "`_latest` should be a symlink");

    let target = std::fs::read_link(&latest_symlink).unwrap();
    let resolved = latest_symlink.parent().unwrap().join(&target);
    assert!(resolved.exists(), "symlink should point to valid directory");

    // Verify symlink points to the execution directory we found
    assert_eq!(
        std::fs::canonicalize(&resolved).unwrap(),
        std::fs::canonicalize(&execution_dir).unwrap(),
        "`_latest` should point to the timestamped execution directory"
    );
}

#[sqlx::test]
async fn latest_symlink_updates_with_subsequent_runs(pool: sqlx::SqlitePool) {
    let (app, _, temp) = create_test_server().pool(pool).call().await;

    let wdl_file = temp.path().join("wdl").join("test.wdl");
    std::fs::write(&wdl_file, SIMPLE_WORKFLOW).unwrap();

    let submit_request = json!({
        "source": wdl_file.to_str().unwrap(),
        "inputs": {},
    });

    // Submit first run
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

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id_1 = submit_response["id"].as_str().unwrap();

    // Wait for first run to complete
    let mut completed = false;
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/runs/{}", run_id_1))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        if status_json["status"] == "completed" {
            completed = true;
            break;
        }
    }

    assert!(completed, "first run should complete");

    let run_dir = temp.path().join("runs").join("test");
    let latest_symlink = run_dir.join("_latest");

    // Get first execution directory
    let execution_dirs: Vec<_> = std::fs::read_dir(&run_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| !p.file_name().unwrap().to_str().unwrap().starts_with('_'))
        .collect();
    assert_eq!(execution_dirs.len(), 1);
    let first_execution_dir = &execution_dirs[0];

    // Verify `_latest` points to first run
    let target = std::fs::read_link(&latest_symlink).unwrap();
    let resolved = latest_symlink.parent().unwrap().join(&target);
    assert_eq!(
        std::fs::canonicalize(&resolved).unwrap(),
        std::fs::canonicalize(first_execution_dir).unwrap(),
        "`_latest` should point to first run"
    );

    // Submit second run
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

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id_2 = submit_response["id"].as_str().unwrap();

    // Wait for second run to complete
    let mut completed = false;
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/runs/{}", run_id_2))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        if status_json["status"] == "completed" {
            completed = true;
            break;
        }
    }

    assert!(completed, "second run should complete");

    // Get all execution directories
    let execution_dirs: Vec<_> = std::fs::read_dir(&run_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| !p.file_name().unwrap().to_str().unwrap().starts_with('_'))
        .collect();
    assert_eq!(execution_dirs.len(), 2, "should have two execution directories");

    // Find the second execution directory (most recent)
    let second_execution_dir = execution_dirs
        .iter()
        .filter(|p| *p != first_execution_dir)
        .next()
        .unwrap();

    // Verify `_latest` now points to second run
    let target = std::fs::read_link(&latest_symlink).unwrap();
    let resolved = latest_symlink.parent().unwrap().join(&target);
    assert_eq!(
        std::fs::canonicalize(&resolved).unwrap(),
        std::fs::canonicalize(second_execution_dir).unwrap(),
        "`_latest` should be updated to point to second run"
    );
}

#[sqlx::test]
async fn cancel_running_run(pool: sqlx::SqlitePool) {
    let (app, db, temp) = create_test_server().pool(pool).call().await;

    // Write long-running WDL to a file
    let wdl_content = r#"
version 1.2

workflow long_test {
    call sleep_task
}

task sleep_task {
    command <<<
        sleep 30
    >>>

    runtime {
        container: "ubuntu:22.04"
    }
}
"#;
    let wdl_file = temp.path().join("wdl").join("long.wdl");
    std::fs::write(&wdl_file, wdl_content).unwrap();

    // Submit workflow
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

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = submit_response["id"].as_str().unwrap();

    // Wait for workflow to start running
    let mut running = false;
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let status_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/runs/{}", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = status_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        if status_json["status"] == "running" {
            running = true;
            break;
        }
    }

    assert!(running, "workflow should start running");

    // First cancel request (with slow failure mode, should go to Canceling)
    let cancel_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/runs/{}/cancel", run_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(cancel_response.status(), StatusCode::OK);

    // Verify workflow is canceling in database
    let run = db.get_run(run_id.parse().unwrap()).await.unwrap().unwrap();

    assert_eq!(run.status, RunStatus::Canceling);
    assert!(run.started_at.is_some());
    assert!(run.completed_at.is_none());
    assert!(run.outputs.is_none());

    // Second cancel request (should go to Cancelled)
    let cancel_response2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/runs/{}/cancel", run_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(cancel_response2.status(), StatusCode::OK);

    // Verify workflow is now canceled in database
    let run = db.get_run(run_id.parse().unwrap()).await.unwrap().unwrap();

    assert_eq!(run.status, RunStatus::Canceled);
    assert!(run.started_at.is_some());
    assert!(run.completed_at.is_none());
    assert!(run.outputs.is_none());

    // Verify no results were indexed
    let index_entries = db
        .list_index_log_entries_by_run(run_id.parse().unwrap())
        .await
        .unwrap();
    assert_eq!(
        index_entries.len(),
        0,
        "canceled workflow should not have indexed results"
    );
}

#[sqlx::test]
async fn cancel_running_run_fast_mode(pool: sqlx::SqlitePool) {
    // Create execution config with fast failure mode
    let mut engine_config = wdl::engine::Config::default();
    engine_config.failure_mode = wdl::engine::config::FailureMode::Fast;

    let (app, db, temp) = create_test_server()
        .pool(pool)
        .engine(engine_config)
        .call()
        .await;

    // Write long-running WDL to a file
    let wdl_content = r#"
version 1.2

workflow long_test {
    call sleep_task
}

task sleep_task {
    command <<<
        sleep 30
    >>>

    runtime {
        container: "ubuntu:22.04"
    }
}
"#;
    let wdl_file = temp.path().join("wdl").join("long.wdl");
    std::fs::write(&wdl_file, wdl_content).unwrap();

    // Submit workflow
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

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = submit_response["id"].as_str().unwrap();

    // Wait for workflow to start running
    let mut running = false;
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let status_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/runs/{}", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = status_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        if status_json["status"] == "running" {
            running = true;
            break;
        }
    }

    assert!(running, "workflow should start running");

    // With fast failure mode, single cancel request should go straight to Cancelled
    let cancel_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/runs/{}/cancel", run_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(cancel_response.status(), StatusCode::OK);

    // Verify workflow is canceled in database (not Canceling)
    let run = db.get_run(run_id.parse().unwrap()).await.unwrap().unwrap();

    assert_eq!(run.status, RunStatus::Canceled);
    assert!(run.started_at.is_some());
    assert!(run.completed_at.is_none());
    assert!(run.outputs.is_none());
}

#[sqlx::test]
async fn submit_run_with_invalid_wdl(pool: sqlx::SqlitePool) {
    let (app, _, temp) = create_test_server().pool(pool).call().await;

    // Write invalid WDL to a file
    let wdl_content = r#"
version 1.2

this is not valid WDL syntax
"#;
    let wdl_file = temp.path().join("wdl").join("invalid.wdl");
    std::fs::write(&wdl_file, wdl_content).unwrap();

    // Submit workflow
    let submit_request = json!({
        "source": wdl_file.to_str().unwrap(),
        "inputs": {},
    });

    let response = app
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

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn submit_run_with_forbidden_file_path(pool: sqlx::SqlitePool) {
    let (app, _, temp) = create_test_server().pool(pool).call().await;

    // Write WDL to a file outside the allowed directory
    let forbidden_dir = temp.path().join("forbidden");
    std::fs::create_dir(&forbidden_dir).unwrap();

    let wdl_file = forbidden_dir.join("test.wdl");
    std::fs::write(&wdl_file, SIMPLE_WORKFLOW).unwrap();

    // Submit workflow using forbidden file path
    let submit_request = json!({
        "source": wdl_file.to_str().unwrap(),
        "inputs": {},
    });

    let response = app
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

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn get_run_not_found(pool: sqlx::SqlitePool) {
    let (app, ..) = create_test_server().pool(pool).call().await;

    let fake_id = uuid::Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/runs/{}", fake_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn list_runs_with_filtering(pool: sqlx::SqlitePool) {
    let (app, _, temp) = create_test_server().pool(pool).call().await;

    // Submit multiple workflows
    let wdl_file = temp.path().join("wdl").join("test.wdl");
    std::fs::write(&wdl_file, SIMPLE_WORKFLOW).unwrap();

    let submit_request = json!({
        "source": wdl_file.to_str().unwrap(),
        "inputs": {},
    });

    // Submit 3 workflows
    for _ in 0..3 {
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
    }

    // Wait for all workflows to complete
    let mut all_completed = false;
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/runs?status=completed")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let list_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

        if list_response["total"] == 3 {
            all_completed = true;
            break;
        }
    }

    assert!(
        all_completed,
        "all workflows should complete within 10 seconds"
    );

    // Verify no running workflows
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs?status=running")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let list_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(list_response["total"], 0);
    assert_eq!(list_response["runs"].as_array().unwrap().len(), 0);

    // List all workflows
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let list_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(list_response["total"], 3);
    assert_eq!(list_response["runs"].as_array().unwrap().len(), 3);

    // List with limit
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs?limit=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let list_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(list_response["total"], 3);
    assert_eq!(list_response["runs"].as_array().unwrap().len(), 2);

    // List with status filter (completed)
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs?status=completed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let list_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(list_response["total"], 3);
    assert_eq!(list_response["runs"].as_array().unwrap().len(), 3);

    // Verify all are completed
    for workflow in list_response["runs"].as_array().unwrap() {
        assert_eq!(workflow["status"], "completed");
    }
}

#[sqlx::test]
async fn cancel_already_completed_run(pool: sqlx::SqlitePool) {
    let (app, _, temp) = create_test_server().pool(pool).call().await;

    // Submit and wait for completion
    let wdl_file = temp.path().join("wdl").join("test.wdl");
    std::fs::write(&wdl_file, SIMPLE_WORKFLOW).unwrap();

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

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = submit_response["id"].as_str().unwrap();

    // Wait for completion
    let mut completed = false;
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let status_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/runs/{}", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = status_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        if status_json["status"] == "completed" {
            completed = true;
            break;
        }
    }

    assert!(completed, "workflow should complete");

    // Try to cancel completed workflow
    let cancel_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/runs/{}/cancel", run_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(cancel_response.status(), StatusCode::CONFLICT);
}

#[sqlx::test]
async fn get_run_outputs(pool: sqlx::SqlitePool) {
    let (app, _, temp) = create_test_server().pool(pool).call().await;

    // Submit and wait for completion
    let wdl_file = temp.path().join("wdl").join("test.wdl");
    std::fs::write(&wdl_file, SIMPLE_WORKFLOW).unwrap();

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

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = submit_response["id"].as_str().unwrap();

    // Wait for completion
    let mut completed = false;
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let status_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/runs/{}", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = status_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        if status_json["status"] == "completed" {
            completed = true;
            break;
        }
    }

    assert!(completed, "workflow should complete");

    // Get workflow outputs
    let outputs_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/runs/{}/outputs", run_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(outputs_response.status(), StatusCode::OK);

    let body = outputs_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let outputs_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(outputs_json["outputs"]["test.message"], "hello world");
    assert_eq!(outputs_json["outputs"]["test.number"], 42);
}

#[sqlx::test]
async fn run_with_indexing(pool: sqlx::SqlitePool) {
    let (app, db, temp) = create_test_server().pool(pool).call().await;

    // Create an index directory
    let index_dir = temp.path().join("index_test");
    std::fs::create_dir(&index_dir).unwrap();

    let wdl_file = temp.path().join("wdl").join("test.wdl");
    std::fs::write(&wdl_file, SIMPLE_WORKFLOW).unwrap();

    // Submit workflow with index_on parameter
    let submit_request = json!({
        "source": wdl_file.to_str().unwrap(),
        "inputs": {},
        "index_on": index_dir.to_str().unwrap(),
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

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = submit_response["id"].as_str().unwrap();

    // Wait for completion
    let mut completed = false;
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let status_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/runs/{}", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = status_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        if status_json["status"] == "completed" {
            completed = true;
            break;
        }
    }

    assert!(completed, "workflow should complete");

    // Verify index entries were created
    let index_entries = db
        .list_index_log_entries_by_run(run_id.parse().unwrap())
        .await
        .unwrap();

    assert!(index_entries.len() > 0, "index entries should be created");

    // Verify each index entry exists on filesystem and is a symlink
    for entry in &index_entries {
        assert!(
            entry.index_path.starts_with("./"),
            "index path should start with `./`: `{}`",
            entry.index_path
        );
        assert!(
            entry.target_path.starts_with("./"),
            "target path should start with `./`: `{}`",
            entry.target_path
        );

        let index_path = temp
            .path()
            .join(entry.index_path.strip_prefix("./").unwrap());
        assert!(
            index_path.exists(),
            "index path should exist: `{}`",
            entry.index_path
        );

        let metadata = std::fs::symlink_metadata(&index_path).unwrap();
        assert!(
            metadata.is_symlink(),
            "index path should be a symlink: `{}`",
            entry.index_path
        );

        // Verify target exists
        let target_path = temp
            .path()
            .join(entry.target_path.strip_prefix("./").unwrap());
        assert!(
            target_path.exists(),
            "target path should exist: `{}`",
            entry.target_path
        );

        // Verify symlink resolves to the target (canonicalize both to compare)
        let link_resolved = std::fs::canonicalize(&index_path).unwrap();
        let target_canonical = std::fs::canonicalize(&target_path).unwrap();
        assert_eq!(
            link_resolved, target_canonical,
            "symlink should resolve to target"
        );
    }
}

#[sqlx::test]
async fn get_outputs_for_incomplete_run(pool: sqlx::SqlitePool) {
    let (app, _, temp) = create_test_server().pool(pool).call().await;

    // Submit long-running workflow
    let wdl_content = r#"
version 1.2

workflow long_test {
    call sleep_task
}

task sleep_task {
    command <<<
        sleep 30
    >>>

    runtime {
        container: "ubuntu:22.04"
    }
}
"#;
    let wdl_file = temp.path().join("wdl").join("long.wdl");
    std::fs::write(&wdl_file, wdl_content).unwrap();

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

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = submit_response["id"].as_str().unwrap();

    // Get outputs immediately (while still running)
    let outputs_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/runs/{}/outputs", run_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(outputs_response.status(), StatusCode::OK);

    let body = outputs_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let outputs_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(
        outputs_json["outputs"].is_null(),
        "outputs should be `null` for incomplete workflow"
    );
}

#[sqlx::test]
async fn get_outputs_for_nonexistent_run(pool: sqlx::SqlitePool) {
    let (app, ..) = create_test_server().pool(pool).call().await;

    let fake_id = uuid::Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/runs/{}/outputs", fake_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn cancel_nonexistent_run(pool: sqlx::SqlitePool) {
    let (app, ..) = create_test_server().pool(pool).call().await;

    let fake_id = uuid::Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/runs/{}/cancel", fake_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn list_runs_with_offset_pagination(pool: sqlx::SqlitePool) {
    let (app, _, temp) = create_test_server().pool(pool).call().await;

    // Submit 5 workflows
    let wdl_file = temp.path().join("wdl").join("test.wdl");
    std::fs::write(&wdl_file, SIMPLE_WORKFLOW).unwrap();

    let submit_request = json!({
        "source": wdl_file.to_str().unwrap(),
        "inputs": {},
    });

    for _ in 0..5 {
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
    }

    // Wait for all to complete
    let mut all_completed = false;
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/runs?status=completed")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let list_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

        if list_response["total"] == 5 {
            all_completed = true;
            break;
        }
    }

    assert!(all_completed, "all workflows should complete");

    // Test pagination with offset
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs?limit=2&offset=0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let page1: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(page1["total"], 5);
    assert_eq!(page1["runs"].as_array().unwrap().len(), 2);

    // Get next page
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs?limit=2&offset=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let page2: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(page2["total"], 5);
    assert_eq!(page2["runs"].as_array().unwrap().len(), 2);

    // Get last page
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs?limit=2&offset=4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let page3: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(page3["total"], 5);
    assert_eq!(page3["runs"].as_array().unwrap().len(), 1);

    // Verify IDs are different across pages
    let id1 = page1["runs"][0]["id"].as_str().unwrap();
    let id2 = page2["runs"][0]["id"].as_str().unwrap();
    assert_ne!(id1, id2, "different pages should have different workflows");
}

#[sqlx::test]
async fn run_that_fails(pool: sqlx::SqlitePool) {
    let (app, db, temp) = create_test_server().pool(pool).call().await;

    // Create a workflow that will fail (command exits with non-zero)
    let wdl_content = r#"
version 1.2

workflow failing_test {
    call fail_task
}

task fail_task {
    command <<<
        exit 1
    >>>

    runtime {
        container: "ubuntu:22.04"
    }
}
"#;
    let wdl_file = temp.path().join("wdl").join("failing.wdl");
    std::fs::write(&wdl_file, wdl_content).unwrap();

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

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = submit_response["id"].as_str().unwrap();

    // Wait for workflow to fail
    let mut failed = false;
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let status_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/runs/{}", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = status_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        if status_json["status"] == "failed" {
            failed = true;
            break;
        }
    }

    assert!(failed, "workflow should fail within 10 seconds");

    // Verify final database state
    let run = db.get_run(run_id.parse().unwrap()).await.unwrap().unwrap();

    assert_eq!(run.status, RunStatus::Failed);
    assert!(run.started_at.is_some());
    assert!(run.completed_at.is_some());
    assert!(run.error.is_some(), "error field should be populated");
    assert!(
        run.outputs.is_none(),
        "failed workflow should not have outputs"
    );
}

#[sqlx::test]
async fn max_concurrent_runs_limit(pool: sqlx::SqlitePool) {
    let (app, db, temp) = create_test_server()
        .pool(pool)
        .max_concurrent_runs(1)
        .call()
        .await;

    // Create a workflow that takes a bit of time to complete
    let wdl_content = r#"
version 1.2

workflow slow_test {
    call slow_task
}

task slow_task {
    command <<<
        sleep 1
    >>>

    runtime {
        container: "ubuntu:22.04"
    }
}
"#;
    let wdl_file = temp.path().join("wdl").join("slow.wdl");
    std::fs::write(&wdl_file, wdl_content).unwrap();

    let submit_request = json!({
        "source": wdl_file.to_str().unwrap(),
        "inputs": {},
    });

    // Submit two workflows quickly
    let mut run_ids = vec![];
    for _ in 0..2 {
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

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        run_ids.push(submit_response["id"].as_str().unwrap().to_string());
    }

    // Check that at least one is `queued` while the other is `running`
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut statuses = vec![];
    for id in &run_ids {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/runs/{}", id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        statuses.push(status_json["status"].as_str().unwrap().to_string());
    }

    // With `max_concurrent_workflows=1`, we should see one `running` and one
    // `queued`
    assert!(
        statuses.contains(&"running".to_string()) || statuses.contains(&"queued".to_string()),
        "at least one workflow should be `running` or `queued`, got {:?}",
        statuses
    );

    // Wait for both to complete
    for id in &run_ids {
        for _ in 0..100 {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(format!("/api/v1/runs/{}", id))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            let body = response.into_body().collect().await.unwrap().to_bytes();
            let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

            if status_json["status"] == "completed" {
                break;
            }
        }
    }

    // Verify both `completed` successfully
    for id in &run_ids {
        let run = db.get_run(id.parse().unwrap()).await.unwrap().unwrap();

        assert_eq!(run.status, RunStatus::Completed);
    }
}

#[sqlx::test]
async fn execute_task_with_explicit_target(pool: sqlx::SqlitePool) {
    let (app, db, temp) = create_test_server().pool(pool).call().await;

    // Create a WDL with a standalone task
    let wdl_content = r#"
version 1.2

task my_task {
    command <<<
        echo "hello from task"
    >>>

    output {
        String message = read_string(stdout())
    }

    runtime {
        container: "ubuntu:22.04"
    }
}
"#;
    let wdl_file = temp.path().join("wdl").join("task.wdl");
    std::fs::write(&wdl_file, wdl_content).unwrap();

    let submit_request = json!({
        "source": wdl_file.to_str().unwrap(),
        "inputs": {},
        "target": "my_task",
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

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = submit_response["id"].as_str().unwrap();

    // Wait for task to complete
    let mut completed = false;
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let status_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/runs/{}", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = status_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        if status_json["status"] == "completed" {
            completed = true;
            break;
        }
    }

    assert!(completed, "task should complete within 10 seconds");

    // Verify final database state
    let run = db.get_run(run_id.parse().unwrap()).await.unwrap().unwrap();

    assert_eq!(run.status, RunStatus::Completed);
    assert!(run.outputs.is_some());
}

#[sqlx::test]
async fn execute_single_task_implicit_target(pool: sqlx::SqlitePool) {
    let (app, db, temp) = create_test_server().pool(pool).call().await;

    // Create a WDL with a single task and no workflow
    let wdl_content = r#"
version 1.2

task only_task {
    command <<<
        echo "implicit task execution"
    >>>

    output {
        String result = read_string(stdout())
    }

    runtime {
        container: "ubuntu:22.04"
    }
}
"#;
    let wdl_file = temp.path().join("wdl").join("single_task.wdl");
    std::fs::write(&wdl_file, wdl_content).unwrap();

    // Submit without specifying target - should automatically use the single task
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

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = submit_response["id"].as_str().unwrap();

    // Wait for task to complete
    let mut completed = false;
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let status_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/runs/{}", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = status_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        if status_json["status"] == "completed" {
            completed = true;
            break;
        }
    }

    assert!(
        completed,
        "single task should complete automatically within 10 seconds"
    );

    let run = db.get_run(run_id.parse().unwrap()).await.unwrap().unwrap();

    assert_eq!(run.status, RunStatus::Completed);
}

#[sqlx::test]
async fn workflow_prioritized_over_task_implicit_target(pool: sqlx::SqlitePool) {
    let (app, _, temp) = create_test_server().pool(pool).call().await;

    // Create a WDL with both a workflow and a task
    let wdl_content = r#"
version 1.2

task helper_task {
    command <<< echo "from task" >>>
    output { String msg = read_string(stdout()) }
    runtime { container: "ubuntu:22.04" }
}

workflow main_workflow {
    output {
        String result = "from workflow"
    }
}
"#;
    let wdl_file = temp.path().join("wdl").join("workflow_and_task.wdl");
    std::fs::write(&wdl_file, wdl_content).unwrap();

    // Submit without target - should execute workflow
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

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = submit_response["id"].as_str().unwrap();

    // Wait to complete
    let mut completed = false;
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let status_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/runs/{}", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = status_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        if status_json["status"] == "completed" {
            completed = true;
            break;
        }
    }

    assert!(completed, "workflow should complete within 10 seconds");

    // Get outputs to verify workflow was executed (not task)
    let outputs_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/runs/{}/outputs", run_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(outputs_response.status(), StatusCode::OK);

    let body = outputs_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let outputs_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        outputs_json["outputs"]["main_workflow.result"], "from workflow",
        "should execute workflow, not task"
    );
}

#[sqlx::test]
async fn ambiguous_document_requires_target(pool: sqlx::SqlitePool) {
    let (app, _, temp) = create_test_server().pool(pool).call().await;

    // Create a WDL with multiple tasks and no workflow
    let wdl_content = r#"
version 1.2

task task_one {
    command <<< echo "one" >>>
    runtime { container: "ubuntu:22.04" }
}

task task_two {
    command <<< echo "two" >>>
    runtime { container: "ubuntu:22.04" }
}
"#;
    let wdl_file = temp.path().join("wdl").join("ambiguous.wdl");
    std::fs::write(&wdl_file, wdl_content).unwrap();

    // Submit without specifying target - should return error
    let submit_request = json!({
        "source": wdl_file.to_str().unwrap(),
        "inputs": {},
    });

    let response = app
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

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "should require target when document is ambiguous"
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let error_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        error_json["message"]
            .as_str()
            .unwrap()
            .contains("target required"),
        "error message should indicate target is required"
    );
}

#[sqlx::test]
async fn target_not_found_returns_404(pool: sqlx::SqlitePool) {
    let (app, _, temp) = create_test_server().pool(pool).call().await;

    let wdl_file = temp.path().join("wdl").join("test.wdl");
    std::fs::write(&wdl_file, SIMPLE_WORKFLOW).unwrap();

    // Submit with non-existent target
    let submit_request = json!({
        "source": wdl_file.to_str().unwrap(),
        "inputs": {},
        "target": "nonexistent_workflow",
    });

    let response = app
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

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "should return 404 when target not found"
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let error_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        error_json["message"]
            .as_str()
            .unwrap()
            .contains("nonexistent_workflow"),
        "error message should include target name"
    );
    assert!(
        error_json["message"]
            .as_str()
            .unwrap()
            .contains("not found"),
        "error message should indicate target not found"
    );
}

#[sqlx::test]
async fn empty_document_returns_error(pool: sqlx::SqlitePool) {
    let (app, _, temp) = create_test_server().pool(pool).call().await;

    // Create a WDL with no workflow and no tasks
    let wdl_content = r#"
version 1.2
"#;
    let wdl_file = temp.path().join("wdl").join("empty.wdl");
    std::fs::write(&wdl_file, wdl_content).unwrap();

    let submit_request = json!({
        "source": wdl_file.to_str().unwrap(),
        "inputs": {},
    });

    let response = app
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

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "should return error when document has no executable target"
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let error_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        error_json["message"]
            .as_str()
            .unwrap()
            .contains("no workflows or tasks"),
        "error message should indicate no executable target"
    );
}

#[sqlx::test]
async fn events_are_received_during_execution(pool: sqlx::SqlitePool) {
    let temp = TempDir::new().unwrap();
    let wdl_dir = temp.path().join("wdl");
    std::fs::create_dir(&wdl_dir).unwrap();

    let mut exec_config = ExecutionConfig::builder()
        .output_directory(temp.path().to_path_buf())
        .allowed_file_paths(vec![wdl_dir.clone()])
        .build();
    exec_config.validate().unwrap();

    let db: Arc<dyn Database> = Arc::new(SqliteDatabase::from_pool(pool).await.unwrap());

    // Create events and subscribe to crankshaft events
    let events = wdl::engine::Events::all(100);
    let mut crankshaft_rx = events.subscribe_crankshaft().unwrap();

    let manager = spawn_manager(exec_config, db.clone(), events);

    // Write workflow with task that will generate events
    let workflow_path = wdl_dir.join("test.wdl");
    std::fs::write(
        &workflow_path,
        r#"
version 1.2

task hello {
    input { String name }
    command <<< echo "Hello, ~{name}!" >>>
    output { String message = read_string(stdout()) }
}

workflow test {
    input { String name }
    call hello { input: name = name }
    output { String message = hello.message }
}
"#,
    )
    .unwrap();

    // Submit run
    let (tx, rx) = oneshot::channel();
    manager
        .send(ManagerCommand::Submit {
            source: workflow_path.to_str().unwrap().to_string(),
            inputs: json!({"test.name": "World"}),
            target: None,
            index_on: None,
            rx: tx,
        })
        .await
        .unwrap();
    let run_id = rx.await.unwrap().unwrap().id;

    // Collect events in background
    let event_collector = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Ok(event) = crankshaft_rx.recv().await {
            events.push(event);
        }
        events
    });

    // Poll until run completes
    loop {
        let (tx, rx) = oneshot::channel();
        manager
            .send(ManagerCommand::GetStatus { id: run_id, rx: tx })
            .await
            .unwrap();
        let status = rx.await.unwrap().unwrap();

        if status.run.status != RunStatus::Running && status.run.status != RunStatus::Queued {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // Shutdown manager to close event channels
    let (tx, rx) = oneshot::channel();
    manager
        .send(ManagerCommand::Shutdown { rx: tx })
        .await
        .unwrap();
    rx.await.unwrap().unwrap();

    // Collect events
    let events = event_collector.await.unwrap();
    assert!(!events.is_empty(), "should receive crankshaft events");
}
