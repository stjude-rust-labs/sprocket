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
use crate::server::Run;
use crate::server::RunResponse;
use crate::server::RunStatus;
use crate::server::paths;

/// Visible width of the status column in the list view.
const STATUS_COLUMN_WIDTH: usize = 12;

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
    /// Valid values: `queued`, `analyzing`, `running`, `completed`, `failed`,
    /// `canceling`, `canceled`. Only used when no `RUN` is provided.
    #[clap(long, value_name = "STATUS")]
    status: Option<String>,

    /// Maximum number of runs to display.
    ///
    /// When set, fetches a single page of at most `N` runs. When omitted, all
    /// runs are displayed by paginating through the server's results. Only
    /// used when no `RUN` is provided.
    #[clap(long, value_name = "N", value_parser = clap::value_parser!(i64).range(1..))]
    limit: Option<i64>,

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

    let name_display = &run.name;
    println!(
        "{short_uuid}  {name:<45}  {status}{elapsed}",
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

/// Lists runs.
///
/// When `limit` is `Some(n)`, fetches a single page of at most `n` runs. When
/// `limit` is `None`, paginates through all runs from the server. Prints one
/// run per line followed by a footer summarizing the count actually displayed
/// alongside the server-reported total (which reflects any active
/// `status_filter`).
async fn status_list(
    base_url: &str,
    status_filter: Option<RunStatus>,
    limit: Option<i64>,
    json: bool,
    colorize: bool,
) -> CommandResult<()> {
    let client = reqwest::Client::new();
    let (runs, total_runs) = fetch_run_list(&client, base_url, status_filter, limit).await?;

    if json {
        let value = serde_json::json!({ "runs": runs, "total": total_runs });
        println!(
            "{}",
            serde_json::to_string_pretty(&value).context("failed to pretty-print response")?
        );
        return Ok(());
    }

    let total = runs.len();

    for run in &runs {
        let status_str = run.status.to_string();
        let status_display = if colorize {
            status_str
                .color(status_color(&run.status))
                .bold()
                .to_string()
        } else {
            status_str.clone()
        };

        // Account for the ANSI color codes when padding the status column so
        // the visible width stays aligned.
        let status_pad = status_display.len() - status_str.len() + STATUS_COLUMN_WIDTH;

        let target_full = run
            .target
            .as_deref()
            .map(|target| target.to_string())
            .unwrap_or_else(|| "-".to_string());
        let name_display = &run.name;
        let timestamp = run
            .completed_at
            .or(run.started_at)
            .unwrap_or(run.created_at)
            .format("%Y-%m-%d %H:%M:%S UTC");

        let target = if target_full.chars().count() > 22 {
            format!("{}…", target_full.chars().take(21).collect::<String>())
        } else {
            target_full
        };

        println!(
            "{short_uuid}  {name:<45}  {status:<status_pad$}  {target:<22}  {timestamp}",
            short_uuid = &run.uuid.to_string()[..8],
            name = name_display,
            status = status_display,
            status_pad = status_pad,
            target = target,
            timestamp = timestamp,
        );
    }

    // The footer distinguishes between "total in the system" (no filter) and
    // "total matching" (a `--status` filter is in effect). This matters
    // because the server's `total` reflects the applied filter, so reporting
    // it as a global count would be misleading when the filter is set.
    if status_filter.is_some() {
        println!("{total} run(s) shown. {total_runs} total matching run(s).");
    } else {
        println!("{total} run(s) shown. {total_runs} total run(s) in the system.");
    }

    Ok(())
}

/// Builds the URL for a `list_runs` request.
///
/// Extracted for testability. `limit` is passed as a `?limit=N` query
/// parameter when `Some`; `status_filter` is added as `?status=X` when `Some`;
/// `next_token` is added as `?next_token=T` when `Some`.
fn build_list_runs_url(
    base_url: &str,
    status_filter: Option<RunStatus>,
    limit: Option<i64>,
    next_token: Option<&str>,
) -> String {
    let mut url = format!("{base_url}{path}", path = paths::LIST_RUNS);
    let mut params = Vec::new();
    if let Some(n) = limit {
        params.push(format!("limit={n}"));
    }
    if let Some(s) = status_filter {
        params.push(format!("status={s}"));
    }
    if let Some(t) = next_token {
        params.push(format!("next_token={t}"));
    }
    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }
    url
}

/// Fetches the run list from the server.
///
/// When `limit` is `Some(n)`, issues a single request with `?limit=n` and
/// returns only that page. When `limit` is `None`, follows the server's
/// `next_token` chain and returns every matching run. Returns
/// `(runs, server_total)` where `server_total` reflects any active
/// `status_filter`.
async fn fetch_run_list(
    client: &reqwest::Client,
    base_url: &str,
    status_filter: Option<RunStatus>,
    limit: Option<i64>,
) -> CommandResult<(Vec<Run>, i64)> {
    let mut runs = Vec::new();
    let mut next_token: Option<String> = None;
    let total;

    loop {
        let url = build_list_runs_url(base_url, status_filter, limit, next_token.as_deref());
        let page: ListRunsResponse = send_json(client.get(&url), "run list").await?;
        runs.extend(page.runs);
        next_token = page.next_token;

        // Explicit `--limit N` caps the client at one page. Otherwise follow
        // the server's pagination chain until it stops.
        if limit.is_some() || next_token.is_none() {
            total = page.total;
            break;
        }
    }

    Ok((runs, total))
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::sync::Mutex;

    use axum::Json;
    use axum::Router;
    use axum::extract::Query;
    use axum::extract::State;
    use axum::routing::get;
    use chrono::Utc;
    use serde::Deserialize;
    use tokio::net::TcpListener;
    use uuid::Uuid;

    use super::*;
    use crate::server::ListRunsResponse;
    use crate::server::Run;

    /// State captured by the mock server for later assertions.
    #[derive(Default)]
    struct MockState {
        /// Every request URL/query received, in order.
        request_queries: Mutex<Vec<String>>,
    }

    /// Query params the mock cares about.
    #[derive(Deserialize)]
    struct MockQuery {
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        limit: Option<i64>,
        #[serde(default)]
        next_token: Option<String>,
    }

    /// Builds a fake `Run` for use in mock responses.
    fn fake_run(name: &str) -> Run {
        Run {
            uuid: Uuid::new_v4(),
            session_uuid: Uuid::new_v4(),
            name: name.to_string(),
            source: "test.wdl".to_string(),
            target: Some("test".to_string()),
            status: RunStatus::Running,
            inputs: "{}".to_string(),
            outputs: None,
            error: None,
            directory: None,
            index_directory: None,
            backend: None,
            transfer_totals: None,
            started_at: None,
            completed_at: None,
            created_at: Utc::now(),
        }
    }

    /// Handler that returns a fixed set of pages depending on `next_token`.
    ///
    /// If no `next_token` is provided, returns the first page of 3 runs with
    /// `next_token=Some("page-2")`. If `next_token=page-2` is provided,
    /// returns 2 runs with `next_token=None` (last page). Total across all
    /// pages: 5.
    async fn paginated_handler(
        State(state): State<Arc<MockState>>,
        Query(q): Query<MockQuery>,
    ) -> Json<ListRunsResponse> {
        let query_str = format!(
            "status={s:?}&limit={l:?}&next_token={t:?}",
            s = q.status,
            l = q.limit,
            t = q.next_token,
        );
        state.request_queries.lock().unwrap().push(query_str);

        match q.next_token.as_deref() {
            None => Json(ListRunsResponse {
                runs: vec![fake_run("a"), fake_run("b"), fake_run("c")],
                total: 5,
                next_token: Some("page-2".to_string()),
            }),
            Some("page-2") => Json(ListRunsResponse {
                runs: vec![fake_run("d"), fake_run("e")],
                total: 5,
                next_token: None,
            }),
            other => panic!("unexpected next_token: {other:?}"),
        }
    }

    /// Spins up an axum server on a random port that serves the paginated
    /// mock at `/api/v1/runs`. Returns the base URL and shared state.
    async fn start_mock_server() -> (String, Arc<MockState>, tokio::task::JoinHandle<()>) {
        let state = Arc::new(MockState::default());
        let router = Router::new()
            .route(paths::LIST_RUNS, get(paginated_handler))
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let base_url = format!("http://{addr}");
        (base_url, state, server)
    }

    #[tokio::test]
    async fn fetch_run_list_without_limit_paginates_through_all_pages() {
        let (base_url, state, server) = start_mock_server().await;
        let client = reqwest::Client::new();

        let (runs, total) = fetch_run_list(&client, &base_url, None, None)
            .await
            .expect("fetch should succeed");

        server.abort();

        assert_eq!(
            runs.len(),
            5,
            "should receive 5 runs across two pages when no --limit is set"
        );
        assert_eq!(total, 5, "server-reported total should be 5");

        // Verify the client made two requests: first without `next_token`,
        // second with `next_token=page-2`.
        let queries = state.request_queries.lock().unwrap();
        assert_eq!(queries.len(), 2, "should have made two requests");
        assert!(
            queries[0].contains("next_token=None"),
            "first request should have no next_token: got `{}`",
            queries[0]
        );
        assert!(
            queries[1].contains("next_token=Some(\"page-2\")"),
            "second request should have next_token=page-2: got `{}`",
            queries[1]
        );
    }

    #[tokio::test]
    async fn fetch_run_list_with_limit_fetches_single_page_only() {
        let (base_url, state, server) = start_mock_server().await;
        let client = reqwest::Client::new();

        let (runs, total) = fetch_run_list(&client, &base_url, None, Some(3))
            .await
            .expect("fetch should succeed");

        server.abort();

        assert_eq!(
            runs.len(),
            3,
            "should receive only the first page's 3 runs when --limit is set"
        );
        assert_eq!(total, 5, "server-reported total should still be 5");

        // Verify the client made exactly ONE request and did NOT follow the
        // pagination chain.
        let queries = state.request_queries.lock().unwrap();
        assert_eq!(
            queries.len(),
            1,
            "should have made exactly one request when --limit is set"
        );
        assert!(
            queries[0].contains("limit=Some(3)"),
            "request should include limit=3: got `{}`",
            queries[0]
        );
    }

    #[tokio::test]
    async fn fetch_run_list_forwards_status_filter_in_request() {
        let (base_url, state, server) = start_mock_server().await;
        let client = reqwest::Client::new();

        let (..) = fetch_run_list(&client, &base_url, Some(RunStatus::Running), Some(1))
            .await
            .expect("fetch should succeed");

        server.abort();

        // Verify the request URL included `status=running`.
        let queries = state.request_queries.lock().unwrap();
        assert_eq!(queries.len(), 1);
        assert!(
            queries[0].contains("status=Some(\"running\")"),
            "request should include the status filter: got `{}`",
            queries[0]
        );
    }

    #[test]
    fn build_list_runs_url_matrix() {
        // No params: no query string.
        let url = build_list_runs_url("http://example.com", None, None, None);
        assert_eq!(url, format!("http://example.com{}", paths::LIST_RUNS));

        // Limit only.
        let url = build_list_runs_url("http://example.com", None, Some(5), None);
        assert!(url.ends_with("?limit=5"), "got `{url}`");

        // Status only.
        let url = build_list_runs_url("http://example.com", Some(RunStatus::Failed), None, None);
        assert!(url.ends_with("?status=failed"), "got `{url}`");

        // Next token only.
        let url = build_list_runs_url("http://example.com", None, None, Some("abc"));
        assert!(url.ends_with("?next_token=abc"), "got `{url}`");

        // All three, in the fixed order limit → status → next_token.
        let url = build_list_runs_url(
            "http://example.com",
            Some(RunStatus::Running),
            Some(10),
            Some("t"),
        );
        assert!(
            url.ends_with("?limit=10&status=running&next_token=t"),
            "got `{url}`"
        );
    }
}
