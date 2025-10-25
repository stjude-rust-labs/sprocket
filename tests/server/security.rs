//! Tests for file source security.

use sprocket_server::api::AppState;
use sprocket_server::api::models::SubmitWorkflowRequest;
use sprocket_server::api::models::WdlSourceRequest;
use sprocket_server::config::Config;
use sprocket_server::config::ServerConfig;
use sprocket_server::db::Database;
use sprocket_server::manager::spawn_manager;
use tempfile::TempDir;

/// Create a test app state with given configuration.
async fn create_test_state(mut config: Config) -> AppState {
    // Canonicalize allowed file paths for tests.
    config.server.allowed_file_paths = config
        .server
        .allowed_file_paths
        .into_iter()
        .filter_map(|p| p.canonicalize().ok())
        .collect();

    let db = Database::new("sqlite::memory:", 20)
        .await
        .expect("failed to create database");
    let manager = spawn_manager(config, db);
    AppState { manager }
}

#[tokio::test]
async fn file_sources_disabled_by_default() {
    let config = Config::default();
    assert!(!config.server.allow_file_sources);
}

#[tokio::test]
async fn file_source_rejected_when_disabled() {
    let temp_dir = TempDir::new().unwrap();
    let wdl_file = temp_dir.path().join("workflow.wdl");
    std::fs::write(&wdl_file, "version 1.2\nworkflow test {}").unwrap();

    let config = Config {
        server: ServerConfig {
            allow_file_sources: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let state = create_test_state(config).await;

    let request = SubmitWorkflowRequest {
        source: WdlSourceRequest::File {
            path: wdl_file.display().to_string(),
        },
        inputs: serde_json::json!({}),
    };

    let (tx, rx) = tokio::sync::oneshot::channel();
    state
        .manager
        .send(sprocket_server::manager::ManagerCommand::Submit {
            source: request.source.into(),
            inputs: request.inputs,
            rx: tx,
        })
        .await
        .unwrap();

    let result = rx.await.unwrap();
    assert_eq!(
        result.unwrap_err().to_string(),
        "file sources are not allowed"
    );
}

#[tokio::test]
async fn file_source_path_not_in_allowed_list() {
    let temp_dir = TempDir::new().unwrap();
    let wdl_file = temp_dir.path().join("workflow.wdl");
    std::fs::write(&wdl_file, "version 1.2\nworkflow test {}").unwrap();

    let other_dir = TempDir::new().unwrap();

    let config = Config {
        server: ServerConfig {
            allow_file_sources: true,
            allowed_file_paths: vec![other_dir.path().to_path_buf()],
            ..Default::default()
        },
        ..Default::default()
    };

    let state = create_test_state(config).await;

    let request = SubmitWorkflowRequest {
        source: WdlSourceRequest::File {
            path: wdl_file.display().to_string(),
        },
        inputs: serde_json::json!({}),
    };

    let (tx, rx) = tokio::sync::oneshot::channel();
    state
        .manager
        .send(sprocket_server::manager::ManagerCommand::Submit {
            source: request.source.into(),
            inputs: request.inputs,
            rx: tx,
        })
        .await
        .unwrap();

    let result = rx.await.unwrap();
    assert_eq!(
        result.unwrap_err().to_string(),
        "file path is not in allowed paths"
    );
}

#[tokio::test]
async fn file_source_allowed_when_in_allowed_list() {
    let temp_dir = TempDir::new().unwrap();
    let wdl_file = temp_dir.path().join("workflow.wdl");
    std::fs::write(&wdl_file, "version 1.2\nworkflow test {}").unwrap();

    let config = Config {
        server: ServerConfig {
            allow_file_sources: true,
            allowed_file_paths: vec![temp_dir.path().to_path_buf()],
            ..Default::default()
        },
        ..Default::default()
    };

    let state = create_test_state(config).await;

    let request = SubmitWorkflowRequest {
        source: WdlSourceRequest::File {
            path: wdl_file.display().to_string(),
        },
        inputs: serde_json::json!({}),
    };

    let (tx, rx) = tokio::sync::oneshot::channel();
    state
        .manager
        .send(sprocket_server::manager::ManagerCommand::Submit {
            source: request.source.into(),
            inputs: request.inputs,
            rx: tx,
        })
        .await
        .unwrap();

    let result = rx.await.unwrap();
    assert!(result.is_ok());
}

#[tokio::test]
async fn file_source_nonexistent_file() {
    let temp_dir = TempDir::new().unwrap();
    let wdl_file = temp_dir.path().join("nonexistent.wdl");

    let config = Config {
        server: ServerConfig {
            allow_file_sources: true,
            allowed_file_paths: vec![temp_dir.path().to_path_buf()],
            ..Default::default()
        },
        ..Default::default()
    };

    let state = create_test_state(config).await;

    let request = SubmitWorkflowRequest {
        source: WdlSourceRequest::File {
            path: wdl_file.display().to_string(),
        },
        inputs: serde_json::json!({}),
    };

    let (tx, rx) = tokio::sync::oneshot::channel();
    state
        .manager
        .send(sprocket_server::manager::ManagerCommand::Submit {
            source: request.source.into(),
            inputs: request.inputs,
            rx: tx,
        })
        .await
        .unwrap();

    let result = rx.await.unwrap();
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.starts_with("file does not exist: "));
    assert!(err_msg.contains("nonexistent.wdl"));
}

#[tokio::test]
async fn file_source_path_traversal_attempt() {
    let temp_dir = TempDir::new().unwrap();
    let allowed_dir = temp_dir.path().join("allowed");
    std::fs::create_dir(&allowed_dir).unwrap();

    let outside_file = temp_dir.path().join("outside.wdl");
    std::fs::write(&outside_file, "version 1.2\nworkflow test {}").unwrap();

    let config = Config {
        server: ServerConfig {
            allow_file_sources: true,
            allowed_file_paths: vec![allowed_dir.clone()],
            ..Default::default()
        },
        ..Default::default()
    };

    let state = create_test_state(config).await;

    let traversal_path = allowed_dir.join("../outside.wdl");

    let request = SubmitWorkflowRequest {
        source: WdlSourceRequest::File {
            path: traversal_path.display().to_string(),
        },
        inputs: serde_json::json!({}),
    };

    let (tx, rx) = tokio::sync::oneshot::channel();
    state
        .manager
        .send(sprocket_server::manager::ManagerCommand::Submit {
            source: request.source.into(),
            inputs: request.inputs,
            rx: tx,
        })
        .await
        .unwrap();

    let result = rx.await.unwrap();
    assert_eq!(
        result.unwrap_err().to_string(),
        "file path is not in allowed paths"
    );
}

#[tokio::test]
async fn content_source_always_allowed() {
    let config = Config::default();
    let state = create_test_state(config).await;

    let request = SubmitWorkflowRequest {
        source: WdlSourceRequest::Content {
            content: String::from("version 1.2\nworkflow test {}"),
        },
        inputs: serde_json::json!({}),
    };

    let (tx, rx) = tokio::sync::oneshot::channel();
    state
        .manager
        .send(sprocket_server::manager::ManagerCommand::Submit {
            source: request.source.into(),
            inputs: request.inputs,
            rx: tx,
        })
        .await
        .unwrap();

    let result = rx.await.unwrap();
    assert!(result.is_ok());
}
