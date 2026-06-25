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

/// Sends a `GET /api/v1/runs/{id}/tasks` request with the given query string
/// (appended after `?`) and returns the response status and parsed JSON body.
async fn list_run_tasks(
    app: &axum::Router,
    run_id: Uuid,
    query: &str,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/runs/{run_id}/tasks?{query}"))
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
async fn list_run_tasks_rejects_non_positive_limit(pool: sqlx::SqlitePool) {
    let (app, db, _temp) = create_test_server(pool).await;

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Server, "test-user")
        .await
        .unwrap();
    let run_id = seed_run(&db, session_id, "pagination-run").await;

    for limit in ["0", "-1", "-100"] {
        let (status, body) = list_run_tasks(&app, run_id, &format!("limit={limit}")).await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "limit={limit} should be rejected"
        );
        assert_eq!(body["kind"], "BadRequest");
        assert!(
            body["message"]
                .as_str()
                .unwrap_or_default()
                .contains("`limit` must be positive"),
            "unexpected message: {}",
            body["message"]
        );
    }
}

#[sqlx::test]
async fn list_run_tasks_rejects_negative_next_token(pool: sqlx::SqlitePool) {
    let (app, db, _temp) = create_test_server(pool).await;

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Server, "test-user")
        .await
        .unwrap();
    let run_id = seed_run(&db, session_id, "pagination-run").await;

    let (status, body) = list_run_tasks(&app, run_id, "next_token=-1").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["kind"], "BadRequest");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("`next_token` must be non-negative"),
        "unexpected message: {}",
        body["message"]
    );
}

#[sqlx::test]
async fn list_run_tasks_rejects_unparseable_next_token(pool: sqlx::SqlitePool) {
    let (app, db, _temp) = create_test_server(pool).await;

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Server, "test-user")
        .await
        .unwrap();
    let run_id = seed_run(&db, session_id, "pagination-run").await;

    let (status, body) = list_run_tasks(&app, run_id, "next_token=nope").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("invalid `next_token`"),
        "unexpected message: {}",
        body["message"]
    );
}

#[sqlx::test]
async fn list_run_tasks_accepts_valid_pagination(pool: sqlx::SqlitePool) {
    let (app, db, _temp) = create_test_server(pool).await;

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Server, "test-user")
        .await
        .unwrap();
    let run_id = seed_run(&db, session_id, "pagination-run").await;

    // Defaults (no `limit`, no `next_token`) are accepted.
    let (status, _) = list_run_tasks(&app, run_id, "").await;
    assert_eq!(status, StatusCode::OK);

    // Positive `limit` and zero `next_token` are accepted.
    let (status, _) = list_run_tasks(&app, run_id, "limit=10&next_token=0").await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test]
async fn list_run_tasks_filters_by_run(pool: sqlx::SqlitePool) {
    let (app, db, _temp) = create_test_server(pool).await;

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Server, "test-user")
        .await
        .unwrap();

    let run_a = seed_run(&db, session_id, "run-a").await;
    let run_b = seed_run(&db, session_id, "run-b").await;

    for name in ["a1", "a2", "a3"] {
        db.create_task(name, run_a).await.unwrap();
    }
    for name in ["b1", "b2"] {
        db.create_task(name, run_b).await.unwrap();
    }

    let (status, body) = list_run_tasks(&app, run_a, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 3);

    let tasks = body["tasks"].as_array().expect("tasks should be an array");
    assert_eq!(tasks.len(), 3);

    let returned_names: std::collections::HashSet<&str> = tasks
        .iter()
        .map(|t| t["name"].as_str().expect("name is a string"))
        .collect();
    let expected_names: std::collections::HashSet<&str> = ["a1", "a2", "a3"].into_iter().collect();
    assert_eq!(returned_names, expected_names);

    // Every returned task must be scoped to `run_a`.
    let run_a_str = run_a.to_string();
    for task in tasks {
        assert_eq!(
            task["run_uuid"].as_str().expect("run_uuid is a string"),
            run_a_str
        );
    }
}

#[sqlx::test]
async fn list_run_tasks_filters_by_status(pool: sqlx::SqlitePool) {
    let (app, db, _temp) = create_test_server(pool).await;

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Server, "test-user")
        .await
        .unwrap();
    let run_id = seed_run(&db, session_id, "status-filter-run").await;

    // Two pending (untouched), one running, one completed, one failed, one
    // canceled, no preempted.
    for name in ["s1", "s2", "s3", "s4", "s5", "s6"] {
        db.create_task(name, run_id).await.unwrap();
    }
    assert!(db.update_task_started("s3", Utc::now()).await.unwrap());
    assert!(
        db.update_task_completed("s4", Some(0), Utc::now())
            .await
            .unwrap()
    );
    assert!(
        db.update_task_failed("s5", "boom", Utc::now())
            .await
            .unwrap()
    );
    assert!(db.update_task_canceled("s6", Utc::now()).await.unwrap());

    // `pending` returns the two untouched tasks.
    let (status, body) = list_run_tasks(&app, run_id, "status=pending").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 2);
    let tasks = body["tasks"].as_array().expect("tasks should be an array");
    assert_eq!(tasks.len(), 2);
    for task in tasks {
        assert_eq!(task["status"].as_str().unwrap(), "pending");
    }

    // Single-row statuses.
    for (status_param, expected_name) in
        [("running", "s3"), ("completed", "s4"), ("failed", "s5"), ("canceled", "s6")]
    {
        let (resp_status, body) =
            list_run_tasks(&app, run_id, &format!("status={status_param}")).await;
        assert_eq!(resp_status, StatusCode::OK);
        assert_eq!(body["total"], 1, "status={status_param} total mismatch");
        let tasks = body["tasks"].as_array().expect("tasks should be an array");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["name"].as_str().unwrap(), expected_name);
        assert_eq!(tasks[0]["status"].as_str().unwrap(), status_param);
    }

    // Empty filter result is a 200 with an empty list, not an error.
    let (status, body) = list_run_tasks(&app, run_id, "status=preempted").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 0);
    assert!(body["tasks"].as_array().expect("tasks should be an array").is_empty());
}

#[sqlx::test]
async fn list_run_tasks_paginates(pool: sqlx::SqlitePool) {
    let (app, db, _temp) = create_test_server(pool).await;

    let session_id = Uuid::new_v4();
    db.create_session(session_id, SprocketCommand::Server, "test-user")
        .await
        .unwrap();
    let run_id = seed_run(&db, session_id, "paginate-run").await;

    for name in ["t1", "t2", "t3", "t4", "t5"] {
        db.create_task(name, run_id).await.unwrap();
    }

    // Helper: collect a page's task names.
    fn names_of(body: &serde_json::Value) -> Vec<String> {
        body["tasks"]
            .as_array()
            .expect("tasks should be an array")
            .iter()
            .map(|t| t["name"].as_str().expect("name is a string").to_string())
            .collect()
    }

    // Page 1: limit=2, no token. Expect 2 items and `next_token == "2"`.
    let (status, page1) = list_run_tasks(&app, run_id, "limit=2").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page1["total"], 5);
    assert_eq!(page1["tasks"].as_array().unwrap().len(), 2);
    assert_eq!(page1["next_token"].as_str(), Some("2"));

    // Page 2: limit=2, next_token=2. Expect 2 items and `next_token == "4"`.
    let (status, page2) = list_run_tasks(&app, run_id, "limit=2&next_token=2").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page2["total"], 5);
    assert_eq!(page2["tasks"].as_array().unwrap().len(), 2);
    assert_eq!(page2["next_token"].as_str(), Some("4"));

    // Page 3: limit=2, next_token=4. Expect 1 item and no `next_token`.
    let (status, page3) = list_run_tasks(&app, run_id, "limit=2&next_token=4").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page3["total"], 5);
    assert_eq!(page3["tasks"].as_array().unwrap().len(), 1);
    assert!(
        page3["next_token"].is_null(),
        "expected absent `next_token` on last page, got {}",
        page3["next_token"]
    );

    // Set-based union: every seeded task appears exactly once across the pages
    // (Option A: avoids depending on the SQLite tie-break order when multiple
    // rows share a `created_at` timestamp).
    let mut all_names: Vec<String> = Vec::new();
    all_names.extend(names_of(&page1));
    all_names.extend(names_of(&page2));
    all_names.extend(names_of(&page3));
    let unique: std::collections::HashSet<&String> = all_names.iter().collect();
    assert_eq!(unique.len(), all_names.len(), "duplicates across pages: {all_names:?}");
    let expected: std::collections::HashSet<String> =
        ["t1", "t2", "t3", "t4", "t5"].into_iter().map(String::from).collect();
    let returned: std::collections::HashSet<String> = all_names.into_iter().collect();
    assert_eq!(returned, expected);
}

#[sqlx::test]
async fn list_run_tasks_unknown_run_returns_empty(pool: sqlx::SqlitePool) {
    let (app, _db, _temp) = create_test_server(pool).await;

    let unknown = Uuid::new_v4();
    let (status, body) = list_run_tasks(&app, unknown, "").await;

    // Mirrors `GET /runs/{id}/tasks/counts`: unknown runs are not an error.
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 0);
    assert!(body["tasks"].as_array().expect("tasks should be an array").is_empty());
    assert!(
        body["next_token"].is_null(),
        "expected absent `next_token`, got {}",
        body["next_token"]
    );
}
