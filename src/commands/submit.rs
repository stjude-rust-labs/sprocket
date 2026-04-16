//! Implementation of the `submit` subcommand.
//!
//! A wrapper around the Sprocket REST API to submit a new workflow!

use anyhow::Context;
use clap::Args as ClapArgs;
use clap::Parser;
use wdl::ast::AstNode;
use wdl::ast::Severity;
use wdl::engine::Inputs;

use crate::analysis::Analysis;
use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::commands::run::inputs_to_json;
use crate::commands::validate::resolve_target_and_inputs;
use crate::config::Config;
use wdl::diagnostics::Mode;
use wdl::diagnostics::emit_diagnostics;
use crate::server::ErrorResponse;
use crate::server::SubmitRunRequest;

/// CLI Arguments for connecting to a Sprocket server instance.
#[derive(ClapArgs, Debug)]
pub struct SprocketClientConnectionArgs {
    /// The hostname of the running Sprocket server to talk to.
    /// If not provided, falls back to the value in the Sprocket Config.
    #[arg(long)]
    host: Option<String>,
    /// The port of the running Sprocket server to talk to.
    /// If not provided, falls back to the value in the Sprocket Config.
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

/// CLI Arguments for specifying the body of the SubmitRunRequest.
#[derive(ClapArgs, Debug)]
pub struct SubmitRunRequestArgs {
    /// WDL source path (local file path or HTTP/HTTPS URL).
    #[clap(value_name = "SOURCE")]
    source: Source,

    /// The inputs for the task or workflow.
    ///
    /// An input can be either a local file path or URL to an input file or
    /// key-value pairs passed in on the command line.
    #[arg(short, long)]
    inputs: Vec<String>,

    /// Optional output name to index on.
    /// If provided, the run outputs will be indexed.
    #[arg(long)]
    index_on: Option<String>,

    /// Optional target workflow or task name to execute.
    ///
    /// This argument is required if trying to run a task or workflow without
    /// any inputs.
    ///
    /// If `target` is not specified, all inputs (from both files and
    /// key-value pairs) are expected to be prefixed with the name of the
    /// workflow or task being run.
    ///
    /// If `target` is specified, it will be appended with a `.` delimiter
    /// and then prepended to all key-value pair inputs on the command line.
    /// Keys specified within files are unchanged by this argument.
    #[clap(short, long, value_name = "NAME")]
    pub target: Option<String>,

    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,
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
    // Ensure the document is valid before sending to the server.
    let results = Analysis::default()
        .add_source(args.run_request_args.source.clone())
        .fallback_version(config.common.wdl.fallback_version.inner().cloned())
        .run()
        .await
        .map_err(CommandError::from)?;

    // SAFETY: this must exist, as we added it as the only source to be analyzed
    // above.
    let document = results
        .filter(&[&args.run_request_args.source])
        .next()
        .unwrap()
        .document();

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
            "Failed to submit WDL document to server due to analysis errors."
        )
        .into());
    }

    let (target, inputs) = resolve_target_and_inputs(
        &args.run_request_args.inputs,
        args.run_request_args.target.clone(),
        document,
    )
    .await?;

    match &inputs {
        Inputs::Task(inputs) => {
            // SAFETY: we wouldn't have a task inputs if a task didn't exist
            // that matched the user's criteria.
            let task = document.task_by_name(&target).unwrap();
            inputs.validate(document, task, None)?
        }
        Inputs::Workflow(inputs) => {
            // SAFETY: we wouldn't have a workflow inputs if a workflow didn't
            // exist that matched the user's criteria.
            let workflow = document.workflow().unwrap();
            inputs.validate(document, workflow, None)?
        }
    }

    let target_json_inputs = serde_json::from_str(&inputs_to_json(&target, &inputs)?)
        .context("Deserializing previously serialized inputs shouldn't fail")?;

    let url = format!(
        "{base}/api/v1/runs",
        base = args.client_args.base_url(&config)
    );

    let request = SubmitRunRequest {
        source: args.run_request_args.source.to_string(),
        inputs: target_json_inputs,
        target: args.run_request_args.target,
        index_on: args.run_request_args.index_on,
    };

    let resp = reqwest::Client::new()
        .post(url)
        .json(&request)
        .send()
        .await
        .context("Sending Request")?;

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
        .context("Expected a response body for successful SubmitRunRequest")?;

    println!("{}", submit_response);

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use anyhow::anyhow;
    use tempfile::NamedTempFile;
    use tokio::net::TcpListener;
    use url::Url;

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
        port: u16,
    }

    async fn start_server(mut config: Config) -> anyhow::Result<ServerTestFixture> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let wdl_file = NamedTempFile::new()?;

        let prefix = wdl_file.path().parent().unwrap();

        let allowed_url = Source::File(
            Url::from_directory_path(prefix)
                .map_err(|e| anyhow!("Failed to build Url from directory path: {:?}", e))?,
        )
        .to_string();

        config.server.allowed_urls.push(allowed_url);

        let db_path = tempfile::NamedTempFile::new()?;

        config.server.database.url = Some(
            db_path
                .path()
                .to_str()
                .expect("Tempfile should have valid path")
                .to_string(),
        );

        let port = listener.local_addr()?.port();
        let server_task = tokio::task::spawn(async {
            run_with_listener(config, listener).await?;
            anyhow::Result::<()>::Ok(())
        });

        Ok(ServerTestFixture {
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
    pub async fn can_submit_simple_file() -> anyhow::Result<()> {
        let ServerTestFixture {
            server_task,
            mut wdl_file,
            port,
        } = start_server(Config::default()).await?;

        wdl_file.write_all(EXAMPLE_WDL_FILE.as_bytes())?;

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
        .expect("Should be able to submit file");

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
            anyhow::bail!("Did not fail in expected way: {:?}", submit_result);
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
            anyhow::bail!("Did not fail in expected way: {:?}", submit_result);
        };

        assert_eq!(
            err.to_string(),
            "Failed to submit WDL document to server due to analysis errors.".to_string()
        );

        Ok(())
    }
}
