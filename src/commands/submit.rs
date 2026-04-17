//! Implementation of the `submit` subcommand.

use anyhow::Context;
use clap::Args as ClapArgs;
use clap::Parser;
use wdl::ast::AstNode;
use wdl::ast::Severity;
use wdl::diagnostics::Mode;
use wdl::diagnostics::emit_diagnostics;

use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::commands::run::inputs_to_json;
use crate::commands::validate::analyze_source;
use crate::commands::validate::validate_inputs;
use crate::config::Config;
use crate::server::ErrorResponse;
use crate::server::SubmitRunRequest;

/// CLI arguments for connecting to a Sprocket server instance.
#[derive(ClapArgs, Debug)]
pub struct SprocketClientConnectionArgs {
    /// The hostname of the running Sprocket server to talk to.
    ///
    /// If not provided, falls back to the value in the Sprocket config.
    #[arg(long)]
    host: Option<String>,

    /// The port of the running Sprocket server to talk to.
    ///
    /// If not provided, falls back to the value in the Sprocket config.
    #[arg(short, long)]
    port: Option<u16>,
}

impl SprocketClientConnectionArgs {
    fn base_url(&self, config: &Config) -> String {
        let host = self.host.as_deref().unwrap_or(&config.server.host);
        let port = self.port.unwrap_or(config.server.port);
        format!("http://{host}:{port}")
    }
}

/// CLI arguments for specifying the body of the [`SubmitRunRequest`].
#[derive(ClapArgs, Debug)]
pub struct SubmitRunRequestArgs {
    /// The WDL source file to submit.
    ///
    /// The source file may be specified by either a local file path or a URL.
    #[clap(value_name = "SOURCE")]
    source: Source,

    /// The inputs for the task or workflow.
    ///
    /// An input can be a key-value pair (e.g., `task.name=value`), an input
    /// file prefixed with `@` (e.g., `@inputs.json`), or a bare value that
    /// is appended to the preceding key's array.
    inputs: Vec<String>,

    /// The name of the task or workflow to submit.
    ///
    /// This argument is required if submitting a task or workflow without
    /// any inputs.
    ///
    /// If `target` is not specified, all inputs (from both files and
    /// key-value pairs) are expected to be prefixed with the name of the
    /// workflow or task being submitted.
    ///
    /// If `target` is specified, it will be appended with a `.` delimiter
    /// and then prepended to all key-value pair inputs on the command line.
    /// Keys specified within files are unchanged by this argument.
    #[clap(short, long, value_name = "NAME")]
    target: Option<String>,

    /// The output name to index on.
    ///
    /// If provided, the server will index the run outputs using the specified
    /// output name as the key.
    #[clap(long, value_name = "OUTPUT_NAME")]
    index_on: Option<String>,

    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    report_mode: Option<Mode>,
}

/// Arguments for the `submit` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    #[command(flatten)]
    client_args: SprocketClientConnectionArgs,

    #[command(flatten)]
    run_request_args: SubmitRunRequestArgs,
}

/// Handles the `submit` subcommand.
///
/// Submits a workflow to a Sprocket server based on the Args / Config.
pub async fn submit(args: Args, config: Config, colorize: bool) -> CommandResult<()> {
    let document = analyze_source(
        &args.run_request_args.source,
        config.common.wdl.fallback_version.inner().cloned(),
    )
    .await?;

    let mut diagnostics = document
        .diagnostics()
        .filter(|t| t.severity() == Severity::Error)
        .peekable();

    if diagnostics.peek().is_some() {
        let path = document.path().to_string();
        let source = document.root().text().to_string();

        emit_diagnostics(
            &path,
            source,
            diagnostics,
            args.run_request_args.report_mode.unwrap_or_default(),
            colorize,
        )
        .context("failed to emit diagnostics")?;

        return Err(anyhow::anyhow!(
            "failed to submit WDL document to server due to analysis errors"
        )
        .into());
    }

    let (target, inputs) = validate_inputs(
        &document,
        &args.run_request_args.inputs,
        args.run_request_args.target.clone(),
    )
    .await?;

    let target_json_inputs = serde_json::from_str(&inputs_to_json(&target, &inputs)?)
        .context("deserializing previously serialized inputs shouldn't fail")?;

    let url = format!(
        "{base}/api/v1/runs",
        base = args.client_args.base_url(&config)
    );

    let source_str = match &args.run_request_args.source {
        Source::File(url) => url
            .to_file_path()
            .ok()
            .and_then(|p| p.to_str().map(String::from))
            .unwrap_or_else(|| url.to_string()),
        other => other.to_string(),
    };

    let request = SubmitRunRequest {
        source: source_str,
        inputs: target_json_inputs,
        target: args.run_request_args.target,
        index_on: args.run_request_args.index_on,
    };

    let resp = reqwest::Client::new()
        .post(url)
        .json(&request)
        .send()
        .await
        .context("sending request")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<ErrorResponse>(&body)
            .map(|e| format!("{}: {}", e.kind, e.message))
            .unwrap_or_else(|_| format!("HTTP {status}: {body}"));
        return Err(CommandError::Single(anyhow::anyhow!(msg)));
    }

    let submit_response: serde_json::Value = resp
        .json()
        .await
        .context("expected a response body for successful `SubmitRunRequest`")?;

    println!(
        "{}",
        serde_json::to_string_pretty(&submit_response)
            .context("failed to pretty-print response")?
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;
    use tokio::net::TcpListener;

    use crate::Config;
    use crate::analysis::Source;
    use crate::commands::CommandError;
    use crate::commands::submit::Args;
    use crate::commands::submit::SprocketClientConnectionArgs;
    use crate::commands::submit::SubmitRunRequestArgs;
    use crate::commands::submit::submit;
    use crate::server::run_with_listener;

    struct ServerTestFixture {
        server_task: tokio::task::JoinHandle<anyhow::Result<()>>,
        wdl_file: NamedTempFile,
        base_url: String,
        port: u16,
    }

    async fn start_server(mut config: Config) -> anyhow::Result<ServerTestFixture> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let wdl_file = NamedTempFile::new()?;

        let prefix = wdl_file
            .path()
            .parent()
            .unwrap()
            .canonicalize()
            .expect("temp dir should be canonicalizable");

        config.server.allowed_file_paths.push(prefix);

        let db_path = tempfile::NamedTempFile::new()?;

        config.server.database.url = db_path
            .path()
            .to_str()
            .expect("tempfile should have valid path")
            .to_string();

        let port = listener.local_addr()?.port();
        let server_task = tokio::task::spawn(async {
            run_with_listener(config, listener).await?;
            anyhow::Result::<()>::Ok(())
        });

        let base_url = format!("http://127.0.0.1:{port}");

        Ok(ServerTestFixture {
            base_url,
            port,
            wdl_file,
            server_task,
        })
    }

    const EXAMPLE_WDL_FILE: &str = r#"
version 1.3

task my_task {

input {
    String name
}

command <<<>>>
}"#;

    const INVALID_FILE: &str = r#"this is not valid wdl"#;

    #[tokio::test]
    pub async fn can_submit_and_complete() -> anyhow::Result<()> {
        let ServerTestFixture {
            server_task,
            mut wdl_file,
            base_url,
            port,
        } = start_server(Config::default()).await?;

        wdl_file.write_all(EXAMPLE_WDL_FILE.as_bytes())?;

        let client = reqwest::Client::new();

        let config = Config::default();
        submit(
            Args {
                client_args: SprocketClientConnectionArgs {
                    host: Some("127.0.0.1".to_string()),
                    port: Some(port),
                },
                run_request_args: SubmitRunRequestArgs {
                    source: Source::File(
                        url::Url::from_file_path(wdl_file.path())
                            .expect("tempfile path should work"),
                    ),
                    inputs: Vec::from(["name=Brendon".to_string()]),
                    index_on: None,
                    target: Some("my_task".to_string()),
                    report_mode: None,
                },
            },
            config,
            false,
        )
        .await
        .expect("should be able to submit file");

        if !cfg!(docker_tests_disabled) {
            let runs: serde_json::Value = client
                .get(format!("{base_url}/api/v1/runs"))
                .send()
                .await?
                .json()
                .await?;

            let uuid = runs["runs"][0]["uuid"]
                .as_str()
                .expect("should have at least one run");

            let poll_url = format!("{base_url}/api/v1/runs/{uuid}");
            let mut status = String::new();

            for _ in 0..50 {
                let run: serde_json::Value = client.get(&poll_url).send().await?.json().await?;
                status = run["status"]
                    .as_str()
                    .expect("run should have a status")
                    .to_string();

                if status != "queued" && status != "running" {
                    break;
                }

                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            assert_eq!(status, "completed");
        }

        assert!(!server_task.is_finished());
        server_task.abort();
        let _ = server_task.await;

        Ok(())
    }

    #[tokio::test]
    pub async fn missing_input_fail() -> anyhow::Result<()> {
        let mut wdl_file = NamedTempFile::new()?;
        wdl_file.write_all(EXAMPLE_WDL_FILE.as_bytes())?;

        let submit_result = submit(
            Args {
                client_args: SprocketClientConnectionArgs {
                    host: Some("127.0.0.1".to_string()),
                    port: Some(1234),
                },
                run_request_args: SubmitRunRequestArgs {
                    source: Source::File(
                        url::Url::from_file_path(wdl_file.path())
                            .expect("tempfile path should work"),
                    ),
                    inputs: Vec::new(),
                    index_on: None,
                    target: Some("my_task".to_string()),
                    report_mode: None,
                },
            },
            Config::default(),
            false,
        )
        .await;

        let Err(CommandError::Single(err)) = submit_result else {
            anyhow::bail!("did not fail in expected way: {:?}", submit_result);
        };

        assert_eq!(
            err.to_string(),
            "missing required input `name` to task `my_task`".to_string()
        );

        Ok(())
    }

    #[tokio::test]
    pub async fn invalid_wdl_fails() -> anyhow::Result<()> {
        let mut wdl_file = NamedTempFile::new()?;
        wdl_file.write_all(INVALID_FILE.as_bytes())?;

        let submit_result = submit(
            Args {
                client_args: SprocketClientConnectionArgs {
                    host: Some("127.0.0.1".to_string()),
                    port: Some(1234),
                },
                run_request_args: SubmitRunRequestArgs {
                    source: Source::File(
                        url::Url::from_file_path(wdl_file.path())
                            .expect("tempfile path should work"),
                    ),
                    inputs: Vec::new(),
                    index_on: None,
                    target: Some("my_task".to_string()),
                    report_mode: None,
                },
            },
            Config::default(),
            false,
        )
        .await;

        let Err(CommandError::Single(err)) = submit_result else {
            anyhow::bail!("did not fail in expected way: {:?}", submit_result);
        };

        assert_eq!(
            err.to_string(),
            "failed to submit WDL document to server due to analysis errors".to_string()
        );

        Ok(())
    }
}
