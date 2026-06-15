//! Implementation of the `inspect` subcommand.

use anyhow::Context;
use clap::Parser;
use colored::Color;
use colored::Colorize as _;

use crate::commands::CommandResult;
use crate::commands::client::SprocketClientConnectionArgs;
use crate::commands::client::check_response;
use crate::commands::client::fetch_task_counts;
use crate::commands::client::resolve_run_id;
use crate::config::Config;
use crate::server::RunResponse;
use crate::server::RunStatus;
use crate::server::RunTaskCountsResponse;

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

/// Builds a one-line summary of a run's per-status task counts.
///
/// Returns `None` when the run has no tasks, so callers can omit the line
/// entirely. Otherwise returns a string like `12 total: 3 running, 8 completed,
/// 1 failed`, listing only the statuses with a non-zero count. When `colorize`
/// is set, each status word is colored to match its meaning.
pub fn task_counts_summary(counts: &RunTaskCountsResponse, colorize: bool) -> Option<String> {
    if counts.total == 0 {
        return None;
    }

    // Fixed display order with a color per status. Labels mirror the
    // `TaskStatus` `Display` output.
    let entries = [
        ("pending", counts.pending, Color::White),
        ("running", counts.running, Color::Cyan),
        ("completed", counts.completed, Color::Green),
        ("failed", counts.failed, Color::Red),
        ("canceled", counts.canceled, Color::Yellow),
        ("preempted", counts.preempted, Color::Yellow),
    ];

    let parts = entries
        .iter()
        .filter(|(_, count, _)| *count > 0)
        .map(|(label, count, color)| {
            let label = if colorize {
                label.color(*color).to_string()
            } else {
                (*label).to_string()
            };
            format!("{count} {label}")
        })
        .collect::<Vec<_>>()
        .join(", ");

    Some(format!("{} total: {}", counts.total, parts))
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

    let counts = fetch_task_counts(&base_url, uuid).await?;

    if args.json {
        let mut raw: serde_json::Value = resp
            .json()
            .await
            .context("failed to deserialize run response")?;
        if let serde_json::Value::Object(map) = &mut raw {
            map.insert(
                "task_counts".to_string(),
                serde_json::to_value(&counts).context("failed to serialize task counts")?,
            );
        }
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

    if let Some(summary) = task_counts_summary(&counts, colorize) {
        field!("Tasks:", summary);
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a [`RunTaskCountsResponse`] from per-status counts, deriving the
    /// total automatically.
    fn counts(
        pending: i64,
        running: i64,
        completed: i64,
        failed: i64,
        canceled: i64,
        preempted: i64,
    ) -> RunTaskCountsResponse {
        RunTaskCountsResponse {
            pending,
            running,
            completed,
            failed,
            canceled,
            preempted,
            total: pending + running + completed + failed + canceled + preempted,
        }
    }

    #[test]
    fn summary_is_none_when_no_tasks() {
        assert_eq!(task_counts_summary(&counts(0, 0, 0, 0, 0, 0), false), None);
    }

    #[test]
    fn summary_lists_only_non_zero_statuses_in_order() {
        let summary = task_counts_summary(&counts(0, 3, 8, 1, 0, 0), false);
        assert_eq!(
            summary.as_deref(),
            Some("12 total: 3 running, 8 completed, 1 failed")
        );
    }

    #[test]
    fn summary_includes_every_status_when_all_present() {
        let summary = task_counts_summary(&counts(1, 2, 3, 4, 5, 6), false);
        assert_eq!(
            summary.as_deref(),
            Some("21 total: 1 pending, 2 running, 3 completed, 4 failed, 5 canceled, 6 preempted")
        );
    }
}
