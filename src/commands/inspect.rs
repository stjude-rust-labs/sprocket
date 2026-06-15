//! Implementation of the `inspect` subcommand.

use anyhow::Context;
use chrono::DateTime;
use chrono::Utc;
use clap::Parser;
use colored::Color;
use colored::Colorize as _;

use crate::commands::CommandResult;
use crate::commands::client::SprocketClientConnectionArgs;
use crate::commands::client::check_response;
use crate::commands::client::fetch_run_tasks;
use crate::commands::client::fetch_task_counts;
use crate::commands::client::resolve_run_id;
use crate::config::Config;
use crate::server::RunResponse;
use crate::server::RunStatus;
use crate::server::RunTaskCountsResponse;
use crate::server::Task;
use crate::server::TaskStatus;

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

    /// Include a per-task breakdown for the run.
    ///
    /// Lists every task with its status, duration, and any error. In `--json`
    /// mode, embeds the full task list under a `tasks` key.
    #[clap(long)]
    detailed: bool,

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

/// Returns the color to use when displaying a task status.
pub fn task_status_color(status: TaskStatus) -> Color {
    match status {
        TaskStatus::Pending => Color::White,
        TaskStatus::Running => Color::Cyan,
        TaskStatus::Completed => Color::Green,
        TaskStatus::Failed => Color::Red,
        TaskStatus::Canceled => Color::Yellow,
        TaskStatus::Preempted => Color::Yellow,
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

    // Fixed display order. Labels mirror the `TaskStatus` `Display` output and
    // colors come from `task_status_color`, the single source of truth.
    let entries = [
        ("pending", counts.pending, TaskStatus::Pending),
        ("running", counts.running, TaskStatus::Running),
        ("completed", counts.completed, TaskStatus::Completed),
        ("failed", counts.failed, TaskStatus::Failed),
        ("canceled", counts.canceled, TaskStatus::Canceled),
        ("preempted", counts.preempted, TaskStatus::Preempted),
    ];

    let parts = entries
        .iter()
        .filter(|(_, count, _)| *count > 0)
        .map(|(label, count, status)| {
            let label = if colorize {
                label.color(task_status_color(*status)).to_string()
            } else {
                (*label).to_string()
            };
            format!("{count} {label}")
        })
        .collect::<Vec<_>>()
        .join(", ");

    Some(format!("{} total: {}", counts.total, parts))
}

/// Formats the duration of a task from its start and completion timestamps.
///
/// Returns a completed duration (`42s`), an in-progress duration (`12s
/// elapsed`) for tasks that have started but not finished, or an empty string
/// for tasks that have not started.
fn task_duration(started_at: Option<DateTime<Utc>>, completed_at: Option<DateTime<Utc>>) -> String {
    match (started_at, completed_at) {
        (Some(start), Some(end)) => format!("{}s", (end - start).num_seconds()),
        (Some(start), None) => format!("{}s elapsed", (Utc::now() - start).num_seconds()),
        _ => String::new(),
    }
}

/// Width of the task name column in the detailed task table.
const TASK_NAME_WIDTH: usize = 30;

/// Width of the task status column in the detailed task table.
const TASK_STATUS_WIDTH: usize = 12;

/// Width of the task duration column in the detailed task table.
const TASK_DURATION_WIDTH: usize = 14;

/// Builds a single aligned row describing a task for the detailed listing.
///
/// The row contains the task name, status, duration, and a trailing detail
/// (the error message, or `exit N` for non-zero exits). When `colorize` is set,
/// the status word is colored to match its meaning.
pub fn task_detail_line(task: &Task, colorize: bool) -> String {
    let status_str = task.status.to_string();
    let status_display = if colorize {
        status_str.color(task_status_color(task.status)).to_string()
    } else {
        status_str.clone()
    };

    // Account for the ANSI color codes when padding the status column so the
    // visible width stays aligned.
    let status_pad = status_display.len() - status_str.len() + TASK_STATUS_WIDTH;

    let duration = task_duration(task.started_at, task.completed_at);

    let detail = match &task.error {
        Some(error) => error.clone(),
        None => match task.exit_status {
            Some(code) if code != 0 => format!("exit {code}"),
            _ => String::new(),
        },
    };

    let detail_display = if colorize && !detail.is_empty() && task.error.is_some() {
        detail.red().to_string()
    } else {
        detail
    };

    format!(
        "  {name:<name_width$}  {status:<status_pad$}  {duration:<duration_width$}  {detail}",
        name = task.name,
        name_width = TASK_NAME_WIDTH,
        status = status_display,
        status_pad = status_pad,
        duration = duration,
        duration_width = TASK_DURATION_WIDTH,
        detail = detail_display,
    )
    .trim_end()
    .to_string()
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

    // Fetch the per-task breakdown only when requested.
    let tasks = if args.detailed {
        Some(fetch_run_tasks(&base_url, uuid).await?)
    } else {
        None
    };

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
            if let Some(tasks) = &tasks {
                map.insert(
                    "tasks".to_string(),
                    serde_json::to_value(tasks).context("failed to serialize tasks")?,
                );
            }
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

    // When requested, append a per-task breakdown below the run summary.
    if let Some(tasks) = &tasks {
        println!();

        if tasks.is_empty() {
            let note = "No tasks.";
            println!(
                "{}",
                if colorize {
                    note.dimmed().to_string()
                } else {
                    note.to_string()
                }
            );
        } else {
            println!(
                "  {name:<name_w$}  {status:<status_w$}  {dur:<dur_w$}  DETAIL",
                name = "NAME",
                status = "STATUS",
                dur = "DURATION",
                name_w = TASK_NAME_WIDTH,
                status_w = TASK_STATUS_WIDTH,
                dur_w = TASK_DURATION_WIDTH,
            );

            for task in tasks {
                println!("{}", task_detail_line(task, colorize));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

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

    /// Builds a [`Task`] for formatting tests. Timestamps are expressed as Unix
    /// seconds so durations are deterministic.
    fn task(
        name: &str,
        status: TaskStatus,
        exit_status: Option<i32>,
        error: Option<&str>,
        started: Option<i64>,
        completed: Option<i64>,
    ) -> Task {
        Task {
            name: name.to_string(),
            run_uuid: Uuid::nil(),
            status,
            exit_status,
            error: error.map(str::to_string),
            created_at: DateTime::<Utc>::from_timestamp(0, 0).unwrap(),
            started_at: started.map(|s| DateTime::<Utc>::from_timestamp(s, 0).unwrap()),
            completed_at: completed.map(|c| DateTime::<Utc>::from_timestamp(c, 0).unwrap()),
        }
    }

    #[test]
    fn detail_line_completed_shows_duration_and_no_exit() {
        let line = task_detail_line(
            &task(
                "align_reads",
                TaskStatus::Completed,
                Some(0),
                None,
                Some(1000),
                Some(1042),
            ),
            false,
        );
        assert!(line.contains("align_reads"));
        assert!(line.contains("completed"));
        assert!(line.contains("42s"));
        assert!(!line.contains("exit"));
        // No trailing whitespace and no color codes when not colorized.
        assert_eq!(line, line.trim_end());
        assert!(!line.contains('\u{1b}'));
    }

    #[test]
    fn detail_line_failed_prefers_error_over_exit() {
        let line = task_detail_line(
            &task(
                "call_variants",
                TaskStatus::Failed,
                Some(1),
                Some("out of memory"),
                Some(1000),
                Some(1003),
            ),
            false,
        );
        assert!(line.contains("failed"));
        assert!(line.contains("3s"));
        assert!(line.contains("out of memory"));
        assert!(!line.contains("exit"));
    }

    #[test]
    fn detail_line_failed_without_error_shows_exit_code() {
        let line = task_detail_line(
            &task(
                "annotate",
                TaskStatus::Failed,
                Some(127),
                None,
                Some(1000),
                Some(1001),
            ),
            false,
        );
        assert!(line.contains("failed"));
        assert!(line.contains("exit 127"));
    }

    #[test]
    fn detail_line_running_shows_elapsed() {
        // Started well in the past, never completed.
        let line = task_detail_line(
            &task("merge", TaskStatus::Running, None, None, Some(1000), None),
            false,
        );
        assert!(line.contains("running"));
        assert!(line.contains("elapsed"));
    }

    #[test]
    fn detail_line_pending_has_no_duration_or_detail() {
        let line = task_detail_line(
            &task("prepare", TaskStatus::Pending, None, None, None, None),
            false,
        );
        assert!(line.contains("prepare"));
        assert!(line.contains("pending"));
        assert!(!line.contains("elapsed"));
        assert!(!line.contains("exit"));
        assert_eq!(line, line.trim_end());
    }
}
