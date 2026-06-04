//! Implementation of the `inspect` subcommand.

use anyhow::Context;
use clap::Parser;
use colored::Color;
use colored::Colorize as _;

use crate::commands::CommandResult;
use crate::commands::client::SprocketClientConnectionArgs;
use crate::commands::client::check_response;
use crate::commands::client::resolve_run_id;
use crate::config::Config;
use crate::server::RunResponse;
use crate::server::RunStatus;

/// Arguments for the `inspect` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The run to inspect.
    ///
    /// May be a UUID or the human-readable generated name of the run (e.g.
    /// `happy-dolphin-42`).
    #[clap(value_name = "RUN_ID")]
    run_id: String,

    /// Output the raw JSON response instead of the formatted summary.
    #[clap(long)]
    json: bool,

    #[command(flatten)]
    client_args: SprocketClientConnectionArgs,
}

/// Returns the color to use when displaying a run status.
pub fn status_color(status: &RunStatus) -> Color {
    match status {
        RunStatus::Completed => Color::Green,
        RunStatus::Failed => Color::Red,
        RunStatus::Canceled => Color::Yellow,
        RunStatus::Canceling => Color::Yellow,
        RunStatus::Running => Color::Cyan,
        RunStatus::Queued => Color::White,
    }
}

/// Handles the `inspect` subcommand.
///
/// Fetches and displays detailed information about a single run.
pub async fn inspect(args: Args, config: Config, colorize: bool) -> CommandResult<()> {
    let base_url = args.client_args.base_url(&config);
    let uuid = resolve_run_id(&args.run_id, &base_url).await?;

    let url = format!("{base_url}/api/v1/runs/{uuid}");
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .context("failed to connect to Sprocket server")?;

    let resp = check_response(resp).await?;

    if args.json {
        let raw: serde_json::Value = resp
            .json()
            .await
            .context("failed to deserialize run response")?;
        println!(
            "{}",
            serde_json::to_string_pretty(&raw).context("failed to pretty-print response")?
        );
        return Ok(());
    }

    let body: RunResponse = resp
        .json()
        .await
        .context("failed to deserialize run response")?;

    let run = &body.run;

    /// Width of the label column.
    const LABEL_WIDTH: usize = 12;

    macro_rules! field {
        ($label:expr, $value:expr) => {
            println!("{:>width$}  {}", $label, $value, width = LABEL_WIDTH);
        };
    }

    let status_str = run.status.to_string();
    let status_display = if colorize {
        status_str
            .color(status_color(&run.status))
            .bold()
            .to_string()
    } else {
        status_str
    };

    field!("Name:", run.name);
    field!("UUID:", run.uuid);
    field!("Status:", status_display);

    if let Some(target) = &run.target {
        field!("Target:", target);
    }

    field!("Source:", run.source);
    field!("Created:", run.created_at.format("%Y-%m-%d %H:%M:%S UTC"));

    if let Some(started_at) = run.started_at {
        field!("Started:", started_at.format("%Y-%m-%d %H:%M:%S UTC"));
    }

    if let Some(completed_at) = run.completed_at {
        field!("Completed:", completed_at.format("%Y-%m-%d %H:%M:%S UTC"));
    }

    if let Some(directory) = &run.directory {
        field!("Directory:", directory);

        // Note the outputs file location if outputs are available.
        if run.outputs.is_some() {
            let outputs_path = std::path::Path::new(directory).join("outputs.json");
            field!(
                "Outputs:",
                format!("available at {}", outputs_path.display())
            );
        }
    }

    if let Some(error) = &run.error {
        let error_display = if colorize {
            error.red().to_string()
        } else {
            error.clone()
        };
        field!("Error:", error_display);
    }

    Ok(())
}
