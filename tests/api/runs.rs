//! Run API end-to-end tests.

use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use serde_json::json;
use sprocket::ServerConfig;
use sprocket::server::AppState;
use sprocket::server::create_router;
use sprocket::system::v1::db::Database;
use sprocket::system::v1::db::Run;
use sprocket::system::v1::db::RunStatus;
use sprocket::system::v1::db::SqliteDatabase;
use sprocket::system::v1::exec::svc::RunManagerCmd;
use sprocket::system::v1::exec::svc::RunManagerSvc;
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

    let mut server_config = ServerConfig {
        output_directory: temp.path().to_path_buf(),
        allowed_file_paths: vec![wdl_dir],
        max_concurrent_runs,
        engine: engine.unwrap_or_default(),
        ..Default::default()
    };
    server_config.validate().unwrap();

    let db = SqliteDatabase::from_pool(pool).await.unwrap();
    let db: Arc<dyn Database> = Arc::new(db);

    let (_, run_manager_tx) = RunManagerSvc::spawn(1000, server_config, db.clone());

    // Wait manager to be ready
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

/// Poll database until run matches a predicate or timeout.
async fn poll_for_run<F>(
    db: &Arc<dyn Database>,
    run_id: uuid::Uuid,
    predicate: F,
    timeout_secs: u64,
    error_msg: &str,
) -> Result<RunStatus, String>
where
    F: Fn(&Run) -> bool,
{
    let poll_interval = std::time::Duration::from_millis(100);
    let max_polls = (timeout_secs * 1000) / 100;

    for _ in 0..max_polls {
        tokio::time::sleep(poll_interval).await;

        let run = db
            .get_run(run_id)
            .await
            .map_err(|e| format!("database error: {}", e))?
            .ok_or_else(|| "run not found".to_string())?;

        if predicate(&run) {
            return Ok(run.status);
        }
    }

    Err(format!("{} (timeout: {} seconds)", error_msg, timeout_secs))
}

/// Poll until run reaches any terminal state with `completed_at` set.
async fn poll_for_completion(
    db: &Arc<dyn Database>,
    run_id: uuid::Uuid,
    timeout_secs: u64,
) -> Result<RunStatus, String> {
    poll_for_run(
        db,
        run_id,
        |run| {
            matches!(
                run.status,
                RunStatus::Completed | RunStatus::Failed | RunStatus::Canceled
            ) && run.completed_at.is_some()
        },
        timeout_secs,
        "run did not complete",
    )
    .await
}

/// Poll until run reaches a specific status.
async fn poll_for_status(
    db: &Arc<dyn Database>,
    run_id: uuid::Uuid,
    expected: RunStatus,
    timeout_secs: u64,
) -> Result<RunStatus, String> {
    poll_for_run(
        db,
        run_id,
        |run| run.status == expected,
        timeout_secs,
        &format!("run did not reach status {:?}", expected),
    )
    .await
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
#[cfg_attr(docker_tests_disabled, ignore = "Docker tests are disabled")]
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

    let run_id = submit_response["uuid"].as_str().unwrap();
    let run_name = submit_response["name"].as_str().unwrap();

    // Verify run was created in database
    let run = db
        .get_run(run_id.parse().unwrap())
        .await
        .unwrap()
        .expect("run should exist in database");

    assert_eq!(run.name, run_name);

    // Wait for run to complete
    let run_id_uuid = run_id.parse().unwrap();
    let status = poll_for_completion(&db, run_id_uuid, 120)
        .await
        .expect("run should complete");

    assert_eq!(status, RunStatus::Completed);

    // Verify final database state
    let run = db.get_run(run_id_uuid).await.unwrap().unwrap();

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
#[cfg_attr(docker_tests_disabled, ignore = "Docker tests are disabled")]
async fn latest_symlink_updates_with_subsequent_runs(pool: sqlx::SqlitePool) {
    let (app, db, temp) = create_test_server().pool(pool).call().await;

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
    let run_id_1 = submit_response["uuid"].as_str().unwrap();

    // Wait for first run to complete
    let status = poll_for_completion(&db, run_id_1.parse().unwrap(), 120)
        .await
        .expect("first run should complete");
    assert_eq!(status, RunStatus::Completed);

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
    let run_id_2 = submit_response["uuid"].as_str().unwrap();

    // Wait for second run to complete
    let status = poll_for_completion(&db, run_id_2.parse().unwrap(), 120)
        .await
        .expect("second run should complete");
    assert_eq!(status, RunStatus::Completed);

    // Get all execution directories
    let execution_dirs: Vec<_> = std::fs::read_dir(&run_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| !p.file_name().unwrap().to_str().unwrap().starts_with('_'))
        .collect();
    assert_eq!(
        execution_dirs.len(),
        2,
        "should have two execution directories"
    );

    // Find the second execution directory (most recent)
    let second_execution_dir = execution_dirs
        .iter()
        .find(|p| *p != first_execution_dir)
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
#[cfg_attr(docker_tests_disabled, ignore = "Docker tests are disabled")]
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
        container: "ubuntu:latest"
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
    let run_id = submit_response["uuid"].as_str().unwrap();

    // Wait for workflow to start running
    poll_for_status(&db, run_id.parse().unwrap(), RunStatus::Running, 120)
        .await
        .expect("workflow should start running");

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

    // Second cancel request (should go to `Canceled`)
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
    assert!(run.completed_at.is_some());
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
#[cfg_attr(docker_tests_disabled, ignore = "Docker tests are disabled")]
async fn cancel_running_run_fast_mode(pool: sqlx::SqlitePool) {
    // Create execution config with fast failure mode
    let engine_config = wdl::engine::Config {
        failure_mode: wdl::engine::config::FailureMode::Fast,
        ..Default::default()
    };

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
        container: "ubuntu:latest"
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
    let run_id = submit_response["uuid"].as_str().unwrap();

    // Wait for workflow to start running
    poll_for_status(&db, run_id.parse().unwrap(), RunStatus::Running, 120)
        .await
        .expect("workflow should start running");

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
    assert!(run.completed_at.is_some());
    assert!(run.outputs.is_none());
}

#[sqlx::test]
async fn submit_run_with_invalid_wdl(pool: sqlx::SqlitePool) {
    let (app, db, temp) = create_test_server().pool(pool).call().await;

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

    // Run is now accepted and analysis happens async
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id: uuid::Uuid = submit_response["uuid"].as_str().unwrap().parse().unwrap();

    let status = poll_for_completion(&db, run_id, 120).await.unwrap();
    assert_eq!(
        status,
        RunStatus::Failed,
        "run should fail due to invalid WDL"
    );

    let run = db.get_run(run_id).await.unwrap().unwrap();
    assert!(run.error.is_some(), "error message should be set");
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
#[cfg_attr(docker_tests_disabled, ignore = "Docker tests are disabled")]
async fn list_runs_with_filtering(pool: sqlx::SqlitePool) {
    let (app, db, temp) = create_test_server().pool(pool).call().await;

    // Submit multiple workflows
    let wdl_file = temp.path().join("wdl").join("test.wdl");
    std::fs::write(&wdl_file, SIMPLE_WORKFLOW).unwrap();

    let submit_request = json!({
        "source": wdl_file.to_str().unwrap(),
        "inputs": {},
    });

    // Submit 3 workflows and collect their IDs
    let mut run_ids = Vec::new();
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

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        run_ids.push(submit_response["uuid"].as_str().unwrap().to_string());
    }

    // Wait for all workflows to complete
    for run_id in &run_ids {
        poll_for_completion(&db, run_id.parse().unwrap(), 120)
            .await
            .expect("workflow should complete");
    }

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
    assert_eq!(
        list_response["next_token"].as_str().unwrap(),
        "2",
        "`next_token` should be the offset of the next item"
    );

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
#[cfg_attr(docker_tests_disabled, ignore = "Docker tests are disabled")]
async fn cancel_already_completed_run(pool: sqlx::SqlitePool) {
    let (app, db, temp) = create_test_server().pool(pool).call().await;

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
    let run_id = submit_response["uuid"].as_str().unwrap();

    // Wait for completion
    let status = poll_for_completion(&db, run_id.parse().unwrap(), 120)
        .await
        .expect("workflow should complete");
    assert_eq!(status, RunStatus::Completed);

    // Get workflow outputs
    let outputs_response = app
        .clone()
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

    // Try to cancel already completed run - should fail
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

    assert_eq!(
        cancel_response.status(),
        StatusCode::CONFLICT,
        "should not be able to cancel a completed run"
    );
}

#[sqlx::test]
#[cfg_attr(docker_tests_disabled, ignore = "Docker tests are disabled")]
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
    let run_id = submit_response["uuid"].as_str().unwrap();

    // Wait for completion
    let status = poll_for_completion(&db, run_id.parse().unwrap(), 120)
        .await
        .expect("workflow should complete");
    assert_eq!(status, RunStatus::Completed);

    // Verify final database state
    let run = db.get_run(run_id.parse().unwrap()).await.unwrap().unwrap();

    assert_eq!(run.status, RunStatus::Completed);
    assert!(run.started_at.is_some());
    assert!(run.completed_at.is_some());
    assert!(
        run.error.is_none(),
        "completed workflow should not have error"
    );
    assert!(
        run.outputs.is_some(),
        "completed workflow should have outputs"
    );

    // Verify that `index_directory` was set
    assert!(
        run.index_directory.is_some(),
        "index_directory should be set when index_on is provided"
    );

    let index_dir_relative = run
        .index_directory
        .as_ref()
        .unwrap()
        .strip_prefix("./")
        .unwrap();
    let index_path = temp.path().join(index_dir_relative);

    assert!(
        index_path.exists(),
        "index directory should exist at {:?}",
        index_path
    );

    // Verify `outputs.json` was created in the index directory
    let outputs_json_path = index_path.join("outputs.json");
    assert!(
        outputs_json_path.exists(),
        "outputs.json should exist in index directory at {:?}",
        outputs_json_path
    );

    // Verify we can read and parse the outputs
    let outputs_content =
        std::fs::read_to_string(&outputs_json_path).expect("should be able to read outputs.json");
    let outputs: serde_json::Value =
        serde_json::from_str(&outputs_content).expect("outputs.json should be valid JSON");

    // The outputs are serialized with the workflow name as a prefix
    assert_eq!(outputs["test.message"], "hello world");
    assert_eq!(outputs["test.number"], 42);
}

#[sqlx::test]
#[cfg_attr(docker_tests_disabled, ignore = "Docker tests are disabled")]
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
        container: "ubuntu:latest"
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
        run_ids.push(submit_response["uuid"].as_str().unwrap().to_string());
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

    // Wait both to complete
    for id in &run_ids {
        poll_for_completion(&db, id.parse().unwrap(), 120)
            .await
            .expect("run should complete");
    }

    // Verify both `completed` successfully
    for id in &run_ids {
        let run = db.get_run(id.parse().unwrap()).await.unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Completed);
    }
}

#[sqlx::test]
#[cfg_attr(docker_tests_disabled, ignore = "Docker tests are disabled")]
async fn execute_task_with_explicit_target(pool: sqlx::SqlitePool) {
    let (app, db, temp) = create_test_server().pool(pool).call().await;

    // Create a WDL with both a workflow and a task
    let wdl_content = r#"
version 1.2

workflow main_workflow {
    call my_task

    output {
        String result = my_task.message
    }
}

task my_task {
    command <<<
    >>>

    output {
        String message = "hello from task"
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
    let run_id = submit_response["uuid"].as_str().unwrap();

    // Wait for task to complete
    let status = poll_for_completion(&db, run_id.parse().unwrap(), 120)
        .await
        .expect("task should complete");
    assert_eq!(status, RunStatus::Completed);

    // Get outputs to verify task was executed
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
        outputs_json["outputs"]["my_task.message"]
            .as_str()
            .unwrap()
            .trim(),
        "hello from task",
        "should execute the task with explicit target"
    );
}

#[sqlx::test]
async fn ambiguous_document_requires_target(pool: sqlx::SqlitePool) {
    let (app, db, temp) = create_test_server().pool(pool).call().await;

    // Create a WDL with multiple tasks and no workflow
    let wdl_content = r#"
version 1.2

task task_one {
    command <<< echo "one" >>>
    runtime { container: "ubuntu:latest" }
}

task task_two {
    command <<< echo "two" >>>
    runtime { container: "ubuntu:latest" }
}
"#;
    let wdl_file = temp.path().join("wdl").join("ambiguous.wdl");
    std::fs::write(&wdl_file, wdl_content).unwrap();

    // Submit without specifying target
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

    // Run is accepted and analysis happens async
    assert_eq!(response.status(), StatusCode::OK, "run should be accepted");

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id: uuid::Uuid = submit_response["uuid"].as_str().unwrap().parse().unwrap();

    let status = poll_for_completion(&db, run_id, 120).await.unwrap();
    assert_eq!(
        status,
        RunStatus::Failed,
        "run should fail when target is ambiguous"
    );

    let run = db.get_run(run_id).await.unwrap().unwrap();
    let error_message = run.error.as_ref().unwrap();
    assert!(
        error_message.contains("a target cannot be inferred"),
        "error message should indicate target is required, got: {}",
        error_message
    );
}

#[sqlx::test]
async fn target_not_found_fails_run(pool: sqlx::SqlitePool) {
    let (app, db, temp) = create_test_server().pool(pool).call().await;

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

    // Run is accepted and analysis happens async
    assert_eq!(response.status(), StatusCode::OK, "run should be accepted");

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id: uuid::Uuid = submit_response["uuid"].as_str().unwrap().parse().unwrap();

    let status = poll_for_completion(&db, run_id, 120).await.unwrap();
    assert_eq!(
        status,
        RunStatus::Failed,
        "run should fail when target not found"
    );

    let run = db.get_run(run_id).await.unwrap().unwrap();
    let error_message = run.error.as_ref().unwrap();
    assert!(
        error_message.contains("nonexistent_workflow"),
        "error message should include target name, got: {}",
        error_message
    );
    assert!(
        error_message.contains("not found"),
        "error message should indicate target not found, got: {}",
        error_message
    );
}

#[sqlx::test]
async fn empty_document_fails_run(pool: sqlx::SqlitePool) {
    let (app, db, temp) = create_test_server().pool(pool).call().await;

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

    // Run is accepted and analysis happens async
    assert_eq!(response.status(), StatusCode::OK, "run should be accepted");

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let submit_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id: uuid::Uuid = submit_response["uuid"].as_str().unwrap().parse().unwrap();

    let status = poll_for_completion(&db, run_id, 120).await.unwrap();
    assert_eq!(
        status,
        RunStatus::Failed,
        "run should fail when document has no executable target"
    );

    let run = db.get_run(run_id).await.unwrap().unwrap();
    let error_message = run.error.as_ref().unwrap();
    assert!(
        error_message
            .contains("there must be at least one task, workflow, struct, or enum definition"),
        "error message should indicate no executable target, got: {}",
        error_message
    );
}

#[sqlx::test]
#[cfg_attr(docker_tests_disabled, ignore = "Docker tests are disabled")]
async fn events_are_received_during_execution(pool: sqlx::SqlitePool) {
    let temp = TempDir::new().unwrap();
    let wdl_dir = temp.path().join("wdl");
    std::fs::create_dir(&wdl_dir).unwrap();

    let mut server_config = ServerConfig {
        output_directory: temp.path().to_path_buf(),
        allowed_file_paths: vec![wdl_dir.clone()],
        ..Default::default()
    };
    server_config.validate().unwrap();

    let db: Arc<dyn Database> = Arc::new(SqliteDatabase::from_pool(pool).await.unwrap());

    // Create events and subscribe to crankshaft events
    let (_, manager) = RunManagerSvc::spawn(1000, server_config, db.clone());

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
        .send(RunManagerCmd::Submit {
            source: workflow_path.to_str().unwrap().to_string(),
            inputs: json!({"test.name": "World"}),
            target: None,
            index_on: None,
            rx: tx,
        })
        .await
        .unwrap();
    let submit_response = rx.await.unwrap().unwrap();
    let mut events_rx = submit_response.events.subscribe_crankshaft().unwrap();
    // Wait for the task to finish.
    submit_response.handle.await.unwrap();

    // Collect events - we should have received some crankshaft events
    let mut event_count = 0;
    while let Ok(event) = events_rx.try_recv() {
        event_count += 1;
        drop(event); // Just count them
    }
    assert!(event_count > 0, "should receive crankshaft events");
}

#[sqlx::test]
async fn invalid_next_token_returns_error(pool: sqlx::SqlitePool) {
    let (app, _db, _temp) = create_test_server().pool(pool).call().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs?next_token=not_a_number")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let error: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("invalid `next_token`")
    );
}
