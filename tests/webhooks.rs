//! End-to-end webhook notification tests.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use serde_json::Value;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::process::Command;

#[derive(Clone, Debug)]
struct MockWebhook {
    addr: std::net::SocketAddr,
    status: StatusCode,
    received: Arc<Mutex<Vec<Value>>>,
}

impl MockWebhook {
    async fn start(status: StatusCode) -> Self {
        async fn handler(
            State(mock): State<MockWebhook>,
            Json(body): Json<Value>,
        ) -> impl IntoResponse {
            mock.received
                .lock()
                .expect("mock webhook state poisoned")
                .push(body);
            mock.status
        }

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mock = Self {
            addr,
            status,
            received: Arc::new(Mutex::new(Vec::new())),
        };
        let app = Router::new()
            .route("/", post(handler))
            .with_state(mock.clone());
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        mock
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn received(&self) -> Vec<Value> {
        self.received
            .lock()
            .expect("mock webhook state poisoned")
            .clone()
    }

    /// Gets the event names of the received Slack payloads, in order.
    fn events(&self) -> Vec<String> {
        self.received()
            .iter()
            .map(|payload| {
                payload["blocks"][1]["fields"][0]["text"]
                    .as_str()
                    .and_then(|text| text.strip_prefix("*Event:*\n"))
                    .expect("payload should have an event field")
                    .to_string()
            })
            .collect()
    }
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap()
}

fn write_failing_wdl(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("fail.wdl");
    std::fs::write(
        &path,
        r#"
version 1.3

task fail {
    requirements {
        max_retries: 1
    }

    command <<<
        exit 1
    >>>
}
"#,
    )
    .unwrap();
    path
}

fn write_config(
    dir: &std::path::Path,
    output_dir: &std::path::Path,
    webhook: Option<&str>,
) -> std::path::PathBuf {
    let config = dir.join("sprocket.toml");
    let webhook = webhook
        .map(|url| {
            format!(
                r#"
[[notifications.webhooks]]
kind = "slack"
url = {}
"#,
                toml_string(url)
            )
        })
        .unwrap_or_default();
    std::fs::write(
        &config,
        format!(
            r#"
[run]
output_dir = {}

[run.backends.default]
type = "local"
{webhook}
"#,
            toml_string(&output_dir.display().to_string()),
        ),
    )
    .unwrap();
    config
}

async fn run_sprocket(
    cwd: &std::path::Path,
    config: &std::path::Path,
    source: &std::path::Path,
    config_root: &std::path::Path,
    timeout: Duration,
) -> std::process::Output {
    tokio::time::timeout(timeout, async {
        Command::new(env!("CARGO_BIN_EXE_sprocket"))
            .current_dir(cwd)
            .env("SPROCKET_CONFIG_ROOT", config_root)
            .arg("--skip-config-search")
            .arg("--config")
            .arg(config)
            .arg("--color")
            .arg("never")
            .arg("run")
            .arg(source)
            .output()
            .await
            .unwrap()
    })
    .await
    .expect("sprocket run timed out")
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_sends_slack_notifications_for_failed_retrying_task() {
    let temp = TempDir::new().unwrap();
    let source = write_failing_wdl(temp.path());

    let baseline_config = write_config(temp.path(), &temp.path().join("baseline-out"), None);
    let baseline = run_sprocket(
        temp.path(),
        &baseline_config,
        &source,
        &temp.path().join("baseline-config-root"),
        Duration::from_secs(30),
    )
    .await;

    let mock = MockWebhook::start(StatusCode::OK).await;
    let config = write_config(
        temp.path(),
        &temp.path().join("webhook-out"),
        Some(&mock.url()),
    );
    let output = run_sprocket(
        temp.path(),
        &config,
        &source,
        &temp.path().join("webhook-config-root"),
        Duration::from_secs(30),
    )
    .await;

    assert_eq!(output.status.code(), baseline.status.code());
    assert!(!output.status.success());

    assert_eq!(
        mock.events(),
        ["run.started", "task.retried", "task.failed", "run.failed"]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_webhook_500_does_not_change_exit_code_or_hang() {
    let temp = TempDir::new().unwrap();
    let source = write_failing_wdl(temp.path());

    let baseline_config = write_config(temp.path(), &temp.path().join("baseline-out"), None);
    let baseline = run_sprocket(
        temp.path(),
        &baseline_config,
        &source,
        &temp.path().join("baseline-config-root"),
        Duration::from_secs(30),
    )
    .await;

    let mock = MockWebhook::start(StatusCode::INTERNAL_SERVER_ERROR).await;
    let config = write_config(
        temp.path(),
        &temp.path().join("webhook-out"),
        Some(&mock.url()),
    );

    let start = Instant::now();
    let output = run_sprocket(
        temp.path(),
        &config,
        &source,
        &temp.path().join("webhook-config-root"),
        Duration::from_secs(25),
    )
    .await;

    assert_eq!(output.status.code(), baseline.status.code());
    assert!(!output.status.success());
    assert!(
        start.elapsed() < Duration::from_secs(20),
        "webhook retries should be bounded by CLI shutdown timeout"
    );
    assert!(!mock.received().is_empty());
}
