//! Implementation of the `retry` subcommand.

use anyhow::Context;
use clap::Parser;
use wdl::ast::AstNode;
use wdl::ast::Severity;
use wdl::diagnostics::Mode;
use wdl::diagnostics::emit_diagnostics;

use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::commands::client::SprocketClientConnectionArgs;
use crate::commands::client::check_response;
use crate::commands::client::resolve_run_id;
use crate::commands::run::inputs_to_json;
use crate::commands::validate::analyze_source;
use crate::commands::validate::validate_inputs;
use crate::config::Config;
use crate::server::RunResponse;
use crate::server::SubmitRunRequest;

/// Arguments for the `retry` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The run to retry.
    ///
    /// May be a UUID or the human-readable generated name of the run (e.g.
    /// `happy-dolphin-42`). The original run's source, target, and inputs are
    /// reused as the base for the new submission.
    #[clap(value_name = "RUN_ID")]
    run_id: String,

    /// Input overrides for the new run.
    ///
    /// Overrides use the same syntax as `dev submit`: key-value pairs
    /// (e.g. `task.name=value`), input files prefixed with `@` (e.g.
    /// `@inputs.json`), or bare values appended to the preceding key's array.
    /// Any key provided here takes precedence over the value from the original
    /// run.
    pub overrides: Vec<String>,

    /// Override the target task or workflow name.
    #[clap(short, long, value_name = "NAME")]
    target: Option<String>,

    /// Override the output name to index on.
    #[clap(long, value_name = "OUTPUT_NAME")]
    index_on: Option<String>,

    /// Skip local re-analysis of the WDL source file.
    ///
    /// By default, `retry` re-analyzes the source file to catch errors before
    /// submitting. Use this flag if the source file is no longer accessible
    /// from the client.
    #[clap(long)]
    no_validate: bool,

    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    report_mode: Option<Mode>,

    #[command(flatten)]
    client_args: SprocketClientConnectionArgs,
}

/// Handles the `retry` subcommand.
///
/// Fetches the original run's details, optionally re-analyzes the source,
/// merges any input overrides, then submits a new run.
pub async fn retry(args: Args, config: Config, colorize: bool) -> CommandResult<()> {
    let base_url = args.client_args.base_url(&config);
    let uuid = resolve_run_id(&args.run_id, &base_url).await?;

    // Fetch the original run.
    let url = format!("{base_url}/api/v1/runs/{uuid}");
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .context("failed to connect to Sprocket server")?;

    let resp = check_response(resp).await?;
    let body: RunResponse = resp
        .json()
        .await
        .context("failed to deserialize run response")?;

    let original = &body.run;

    // Resolve the effective target: CLI override > stored target > None.
    let effective_target = args.target.clone().or_else(|| original.target.clone());

    // Parse the original inputs JSON (pre-prefixed keys, e.g. `task.name`).
    let mut merged_inputs: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&original.inputs)
            .context("failed to deserialize stored run inputs")?;

    // Parse the source string into an analysis `Source`.
    let source: Source = original
        .source
        .parse()
        .with_context(|| format!("failed to parse source `{}`", original.source))?;

    if !args.no_validate {
        // Re-analyze the WDL source locally, mirroring what `submit` does.
        let document = analyze_source(
            &source,
            config.common.wdl.fallback_version.inner().cloned(),
        )
        .await
        .map_err(|e| {
            // Wrap with a hint to use --no-validate if the file is unreachable.
            match e {
                CommandError::Single(inner) => CommandError::Single(
                    inner.context(format!(
                        "cannot re-analyze source `{source}`; \
                         use --no-validate to skip local analysis",
                        source = original.source
                    )),
                ),
                other => other,
            }
        })?;

        let mut errors = document
            .diagnostics()
            .filter(|d| d.severity() == Severity::Error)
            .peekable();

        if errors.peek().is_some() {
            let path = document.path().to_string();
            let source_text = document.root().text().to_string();
            emit_diagnostics(
                &path,
                &source_text,
                errors,
                args.report_mode.unwrap_or_default(),
                colorize,
            )
            .context("failed to emit diagnostics")?;

            return Err(
                anyhow::anyhow!("failed to retry run due to analysis errors in source").into(),
            );
        }

        // If there are override inputs, validate and serialize them via the
        // same path as `submit`, then merge them on top of the original.
        if !args.overrides.is_empty() || effective_target.is_some() {
            let (override_target, override_inputs) = validate_inputs(
                &document,
                &args.overrides,
                effective_target.clone(),
            )
            .await?;

            let override_json: serde_json::Map<String, serde_json::Value> =
                serde_json::from_str(
                    &inputs_to_json(&override_target, &override_inputs)
                        .context("failed to serialize override inputs")?,
                )
                .context("failed to deserialize override inputs")?;

            // Override keys win over the original values.
            for (k, v) in override_json {
                merged_inputs.insert(k, v);
            }
        }
    } else if !args.overrides.is_empty() {
        // --no-validate: still parse key=value overrides but without
        // document-level validation. Treat each `key=value` as a literal JSON
        // string value merge on top of the existing map.
        for item in &args.overrides {
            if let Some((key, value)) = item.split_once('=') {
                merged_inputs.insert(
                    key.to_string(),
                    serde_json::Value::String(value.to_string()),
                );
            }
        }
    }

    // Submit the new run.
    let submit_url = format!("{base_url}/api/v1/runs");
    let request = SubmitRunRequest {
        source: original.source.clone(),
        inputs: serde_json::Value::Object(merged_inputs),
        target: effective_target,
        index_on: args.index_on,
    };

    let resp = reqwest::Client::new()
        .post(&submit_url)
        .json(&request)
        .send()
        .await
        .context("failed to connect to Sprocket server")?;

    let resp = check_response(resp).await?;

    let submit_response: serde_json::Value = resp
        .json()
        .await
        .context("expected a response body for successful retry submission")?;

    println!(
        "{}",
        serde_json::to_string_pretty(&submit_response)
            .context("failed to pretty-print response")?
    );

    Ok(())
}
