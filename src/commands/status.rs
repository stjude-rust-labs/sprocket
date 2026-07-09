//! Implementation of the `status` subcommand.

use anyhow::Context;
use clap::Parser;
use colored::Colorize as _;

use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::commands::client::ServerConnectionArgs;
use crate::commands::client::fetch_task_counts;
use crate::commands::client::get_json;
use crate::commands::client::resolve_run_id;
use crate::commands::client::send_json;
use crate::commands::inspect::status_color;
use crate::commands::inspect::task_counts_summary;
use crate::config::Config;
use crate::server::ListRunsResponse;
use crate::server::RunResponse;
use crate::server::RunStatus;
use crate::server::paths;

/// Arguments for the `status` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The run to inspect.
    ///
    /// May be a UUID or the human-readable generated name of the run (e.g.
    /// `happy-dolphin-42`). If omitted, all runs are listed.
    #[clap(value_name = "RUN")]
    run_id: Option<String>,

    /// Filter the run list by status.
    ///
    /// Valid values: `queued`, `running`, `completed`, `failed`, `canceling`,
    /// `canceled`. Only used when no `RUN` is provided.
    #[clap(long, value_name = "STATUS")]
    status: Option<String>,

    /// Maximum number of runs to return per page.
    ///
    /// Only used when no `RUN` is provided.
    #[clap(long, value_name = "N", default_value = "100", value_parser = clap::value_parser!(i64).range(1..))]
    limit: i64,

    /// Output the raw JSON response instead of the formatted summary.
    #[clap(long)]
    json: bool,

    #[command(flatten)]
    client_args: ServerConnectionArgs,
}

/// Handles the `status` subcommand.
///
/// With a `RUN`, prints a brief summary of that run. Without one, lists all
/// runs one per line.
pub async fn status(args: Args, config: Config, colorize: bool) -> CommandResult<()> {
    let base_url = args.client_args.base_url(&config);

    // Parse the optional status filter string into a RunStatus.
    let status_filter = args
        .status
        .as_deref()
        .map(|s| {
            s.parse::<RunStatus>()
                .map_err(|_| CommandError::Single(anyhow::anyhow!("invalid status `{s}`")))
        })
        .transpose()?;

    if let Some(run_id) = &args.run_id {
        status_single(run_id, &base_url, args.json, colorize).await
    } else {
        status_list(&base_url, status_filter, args.limit, args.json, colorize).await
    }
}

/// Prints a brief single-run summary.
async fn status_single(
    run_id: &str,
    base_url: &str,
    json: bool,
    colorize: bool,
) -> CommandResult<()> {
    let uuid = resolve_run_id(run_id, base_url).await?;

    let url = format!("{base_url}{path}", path = paths::get_run(uuid));
    let counts = fetch_task_counts(base_url, uuid).await?;

    if json {
        let mut raw: serde_json::Value = get_json(&url, "run").await?;
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

    let body: RunResponse = get_json(&url, "run").await?;

    let run = &body.run;

    let status_str = run.status.to_string();
    let status_display = if colorize {
        status_str
            .color(status_color(&run.status))
            .bold()
            .to_string()
    } else {
        status_str
    };

    // Calculate elapsed time if available.
    let elapsed = match (run.started_at, run.completed_at) {
        (Some(start), Some(end)) => {
            let secs = (end - start).num_seconds();
            format!(" ({secs}s)")
        }
        (Some(start), None) => {
            let secs = (chrono::Utc::now() - start).num_seconds();
            format!(" ({secs}s elapsed)")
        }
        _ => String::new(),
    };

    let name_display = format!("{}", run.name);
    println!(
        "{short_uuid}  {name:<32}  {status}{elapsed}",
        short_uuid = &run.uuid.to_string()[..8],
        name = name_display,
        status = status_display,
        elapsed = elapsed,
    );

    if let Some(target) = &run.target {
        println!("{:>14}  {target}", "Target:");
    }

    if let Some(summary) = task_counts_summary(&counts, colorize) {
        println!("{:>14}  {summary}", "Tasks:");
    }

    Ok(())
}

/// Lists all runs one per line, paginating through all pages.
async fn status_list(
    base_url: &str,
    status_filter: Option<RunStatus>,
    limit: i64,
    json: bool,
    colorize: bool,
) -> CommandResult<()> {
    let client = reqwest::Client::new();
    let mut next_token: Option<String> = None;
    let mut all_runs = Vec::new();

    loop {
        let mut url = format!("{base_url}{path}?limit={limit}", path = paths::LIST_RUNS,);
        if let Some(s) = &status_filter {
            url.push_str(&format!("&status={s}"));
        }
        if let Some(token) = &next_token {
            url.push_str(&format!("&next_token={token}"));
        }

        let page: ListRunsResponse = send_json(client.get(&url), "run list").await?;

        all_runs.extend(page.runs);
        next_token = page.next_token;

        if next_token.is_none() {
            break;
        }
    }

    if json {
        let value = serde_json::json!({ "runs": all_runs });
        println!(
            "{}",
            serde_json::to_string_pretty(&value).context("failed to pretty-print response")?
        );
        return Ok(());
    }

    let total = all_runs.len();

    for run in &all_runs {
        let status_str = run.status.to_string();
        let status_display = if colorize {
            status_str
                .color(status_color(&run.status))
                .bold()
                .to_string()
        } else {
            status_str
        };

        let target = run
            .target
            .as_deref()
            .map(|target| format!("{target}"))
            .unwrap_or_else(|| "-".to_string());
        let name_display = format!("{}", run.name);
        let timestamp = run
            .completed_at
            .or(run.started_at)
            .unwrap_or(run.created_at)
            .format("%Y-%m-%d %H:%M:%S UTC");

        println!(
            "{short_uuid}  {name:<32}  {status:<12}  {target:<22}  {timestamp}",
            short_uuid = &run.uuid.to_string()[..8],
            name = name_display,
            status = status_display,
            target = target,
            timestamp = timestamp,
        );
    }

    println!("{total} run(s) shown.");

    Ok(())
}
