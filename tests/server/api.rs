//! API integration tests.

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use serde_json::json;
use sprocket_server::api::models::SubmitWorkflowRequest;
use sprocket_server::api::models::WdlSourceRequest;
use sprocket_server::api::AppState;
use sprocket_server::config::Config;
use sprocket_server::db::Database;
use sprocket_server::manager::spawn_manager;
use tower::ServiceExt;

/// Simple WDL workflow that outputs values.
const SIMPLE_WORKFLOW: &str = r#"
version 1.2

workflow test {
    output {
        String message = "hello world"
        Int number = 42
    }
}
"#;

/// Create a test app router.
async fn create_test_app() -> axum::Router {
    let config = Config::default();
    let db = Database::new("sqlite::memory:", 20)
        .await
        .expect("failed to create database");
    let manager = spawn_manager(config.clone(), db);
    let state = AppState { manager };
    sprocket_server::server::create_router(state)
}

#[tokio::test]
async fn submit_workflow_with_content_source() {
    let app = create_test_app().await;

    let request = SubmitWorkflowRequest {
        source: WdlSourceRequest::Content {
            content: String::from(SIMPLE_WORKFLOW),
        },
        inputs: json!({}),
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workflows")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let response_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(response_json["id"].is_string());
    assert!(response_json["name"].is_string());
}

#[tokio::test]
async fn get_workflow_status() {
    let app = create_test_app().await;

    // Submit a workflow first.
    let submit_request = SubmitWorkflowRequest {
        source: WdlSourceRequest::Content {
            content: String::from(SIMPLE_WORKFLOW),
        },
        inputs: json!({}),
    };

    let submit_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workflows")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&submit_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = submit_response.into_body().collect().await.unwrap().to_bytes();
    let submit_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let workflow_id = submit_json["id"].as_str().unwrap();

    // Get the workflow status.
    let status_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/workflows/{}", workflow_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(status_response.status(), StatusCode::OK);

    let body = status_response.into_body().collect().await.unwrap().to_bytes();
    let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(status_json["id"], workflow_id);
    assert!(status_json["name"].is_string());
    assert!(status_json["status"].is_string());
}

#[tokio::test]
async fn list_workflows() {
    let app = create_test_app().await;

    // Submit a workflow first.
    let submit_request = SubmitWorkflowRequest {
        source: WdlSourceRequest::Content {
            content: String::from(SIMPLE_WORKFLOW),
        },
        inputs: json!({}),
    };

    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workflows")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&submit_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // List workflows.
    let list_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/workflows")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(list_response.status(), StatusCode::OK);

    let body = list_response.into_body().collect().await.unwrap().to_bytes();
    let list_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(list_json["workflows"].is_array());
    assert!(list_json["total"].is_number());
    assert!(list_json["total"].as_i64().unwrap() >= 1);
}

#[tokio::test]
async fn get_workflow_not_found() {
    let app = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/workflows/00000000-0000-0000-0000-000000000000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn cancel_workflow() {
    let app = create_test_app().await;

    // Submit a workflow first.
    let submit_request = SubmitWorkflowRequest {
        source: WdlSourceRequest::Content {
            content: String::from(SIMPLE_WORKFLOW),
        },
        inputs: json!({}),
    };

    let submit_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workflows")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&submit_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = submit_response.into_body().collect().await.unwrap().to_bytes();
    let submit_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let workflow_id = submit_json["id"].as_str().unwrap();

    // Cancel the workflow.
    let cancel_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workflows/{}/cancel", workflow_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(cancel_response.status(), StatusCode::OK);

    let body = cancel_response.into_body().collect().await.unwrap().to_bytes();
    let cancel_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(cancel_json["id"], workflow_id);
}

#[tokio::test]
async fn get_workflow_outputs() {
    let app = create_test_app().await;

    // Submit a workflow first.
    let submit_request = SubmitWorkflowRequest {
        source: WdlSourceRequest::Content {
            content: String::from(SIMPLE_WORKFLOW),
        },
        inputs: json!({}),
    };

    let submit_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workflows")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&submit_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = submit_response.into_body().collect().await.unwrap().to_bytes();
    let submit_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let workflow_id = submit_json["id"].as_str().unwrap().to_string();

    // Wait for workflow to complete (poll status).
    let mut completed = false;
    for _ in 0..50 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let status_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/workflows/{}", workflow_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = status_response.into_body().collect().await.unwrap().to_bytes();
        let status_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        if status_json["status"] == "completed" || status_json["status"] == "failed" {
            completed = true;
            assert_eq!(status_json["status"], "completed");
            break;
        }
    }

    assert!(completed, "workflow did not complete in time");

    // Get workflow outputs.
    let outputs_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/workflows/{}/outputs", workflow_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(outputs_response.status(), StatusCode::OK);

    let body = outputs_response.into_body().collect().await.unwrap().to_bytes();
    let outputs_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let expected_outputs = json!({
        "test.message": "hello world",
        "test.number": 42
    });

    assert_eq!(outputs_json["outputs"], expected_outputs);
}

#[tokio::test]
async fn get_workflow_logs() {
    let app = create_test_app().await;

    // Submit a workflow first.
    let submit_request = SubmitWorkflowRequest {
        source: WdlSourceRequest::Content {
            content: String::from(SIMPLE_WORKFLOW),
        },
        inputs: json!({}),
    };

    let submit_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workflows")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&submit_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = submit_response.into_body().collect().await.unwrap().to_bytes();
    let submit_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let workflow_id = submit_json["id"].as_str().unwrap();

    // Get workflow logs.
    let logs_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/workflows/{}/logs", workflow_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(logs_response.status(), StatusCode::OK);

    let body = logs_response.into_body().collect().await.unwrap().to_bytes();
    let logs_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(logs_json["logs"].is_array());
    assert!(logs_json["total"].is_number());
}
