//! Tests for the `sprocket localize` command.
//!
//! These tests serve "remote" outputs from a local HTTP server so that
//! downloading is exercised without needing access to cloud storage.

use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Output;
use std::process::Stdio;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use serde_json::Value;
use serde_json::json;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::process::Command;

/// Starts a server that serves two files and returns the address it listens on.
async fn start_server() -> SocketAddr {
    let app = Router::new()
        .route("/outputs/a/report.txt", get(|| async { "report from a" }))
        .route("/outputs/b/report.txt", get(|| async { "report from b" }))
        .route(
            "/outputs/missing.txt",
            get(|| async { StatusCode::NOT_FOUND }),
        );

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("should bind to a local port");
    let address = listener.local_addr().expect("should have an address");

    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("should serve");
    });

    address
}

/// Runs `sprocket localize` in the given directory.
async fn localize(dir: &Path, args: &[&str]) -> Output {
    let config_root = TempDir::new().expect("should create config root");

    Command::new(env!("CARGO_BIN_EXE_sprocket"))
        .arg("--skip-config-search")
        .arg("localize")
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("SPROCKET_CONFIG_ROOT", config_root.path())
        .env("RUST_LOG", "none")
        .env_remove("RUST_BACKTRACE")
        .output()
        .await
        .expect("should run sprocket")
}

/// Writes an outputs file to a new `run` directory.
fn write_run(dir: &Path, outputs: &Value) {
    let run = dir.join("run");
    fs::create_dir_all(&run).expect("should create run directory");
    fs::write(
        run.join("outputs.json"),
        serde_json::to_string_pretty(outputs).unwrap(),
    )
    .expect("should write outputs");
}

/// Gets a UTF-8 string from process output.
fn text(bytes: &[u8]) -> &str {
    str::from_utf8(bytes).expect("should be UTF-8")
}

#[tokio::test]
async fn downloads_remote_outputs() {
    let address = start_server().await;
    let dir = TempDir::new().unwrap();
    let outputs = json!({
        "wf.report": format!("http://{address}/outputs/a/report.txt"),
        "wf.reports": [
            format!("http://{address}/outputs/a/report.txt"),
            format!("http://{address}/outputs/b/report.txt"),
        ],
        "wf.local": "/data/runs/wf/work/local.txt",
        "wf.count": 3,
        "wf.label": "hello",
    });
    write_run(dir.path(), &outputs);

    let output = localize(dir.path(), &["run"]).await;
    assert!(
        output.status.success(),
        "command failed: {stderr}",
        stderr = text(&output.stderr)
    );

    // Remote outputs are downloaded next to the run, grouped by output
    let localized = dir.path().join("run/localized");
    let read = |path: &str| fs::read_to_string(localized.join(path)).unwrap();
    assert_eq!(read("wf.report/report.txt"), "report from a");
    assert_eq!(read("wf.reports/0/report.txt"), "report from a");
    assert_eq!(read("wf.reports/1/report.txt"), "report from b");

    // The localized outputs refer to the downloaded files, and only to those
    let stdout: Value = serde_json::from_str(text(&output.stdout)).unwrap();
    let localized = fs::canonicalize(&localized).unwrap();
    let local_path = |path: &str| localized.join(path).to_string_lossy().into_owned();
    assert_eq!(
        stdout,
        json!({
            "wf.report": local_path("wf.report/report.txt"),
            "wf.reports": [
                local_path("wf.reports/0/report.txt"),
                local_path("wf.reports/1/report.txt"),
            ],
            "wf.local": "/data/runs/wf/work/local.txt",
            "wf.count": 3,
            "wf.label": "hello",
        })
    );

    // The localized outputs are also written to the destination, and the
    // original outputs file is untouched
    let written: Value =
        serde_json::from_str(&fs::read_to_string(localized.join("outputs.json")).unwrap()).unwrap();
    assert_eq!(written, stdout);

    let original: Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join("run/outputs.json")).unwrap())
            .unwrap();
    assert_eq!(original, outputs);
}

#[tokio::test]
async fn accepts_the_outputs_file_and_a_destination() {
    let address = start_server().await;
    let dir = TempDir::new().unwrap();
    write_run(
        dir.path(),
        &json!({ "wf.report": format!("http://{address}/outputs/b/report.txt") }),
    );

    let output = localize(
        dir.path(),
        &["run/outputs.json", "--destination", "elsewhere"],
    )
    .await;
    assert!(
        output.status.success(),
        "command failed: {stderr}",
        stderr = text(&output.stderr)
    );

    assert_eq!(
        fs::read_to_string(dir.path().join("elsewhere/wf.report/report.txt")).unwrap(),
        "report from b"
    );
    assert!(dir.path().join("elsewhere/outputs.json").is_file());
    assert!(!dir.path().join("run/localized").exists());
}

#[tokio::test]
async fn requires_overwrite_to_replace_files() {
    let address = start_server().await;
    let dir = TempDir::new().unwrap();
    write_run(
        dir.path(),
        &json!({ "wf.report": format!("http://{address}/outputs/a/report.txt") }),
    );

    let first = localize(dir.path(), &["run"]).await;
    assert!(first.status.success(), "{}", text(&first.stderr));

    // A second run must not silently replace the existing download
    let second = localize(dir.path(), &["run"]).await;
    assert!(!second.status.success());
    assert!(
        text(&second.stderr).contains("--overwrite"),
        "unexpected stderr: {stderr}",
        stderr = text(&second.stderr)
    );

    let third = localize(dir.path(), &["run", "--overwrite"]).await;
    assert!(third.status.success(), "{}", text(&third.stderr));
}

#[tokio::test]
async fn reports_failed_downloads() {
    let address = start_server().await;
    let dir = TempDir::new().unwrap();
    write_run(
        dir.path(),
        &json!({ "wf.missing": format!("http://{address}/outputs/missing.txt") }),
    );

    let output = localize(dir.path(), &["run"]).await;
    assert!(!output.status.success());

    let stderr = text(&output.stderr);
    assert!(
        stderr.contains("failed to localize one or more outputs"),
        "unexpected stderr: {stderr}"
    );

    // No localized outputs are written for a failed localization
    assert!(!dir.path().join("run/localized/outputs.json").exists());
}
