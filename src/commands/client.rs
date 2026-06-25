//! Shared client utilities for commands that communicate with a Sprocket
//! server.

use anyhow::Context;
use clap::Args as ClapArgs;
use uuid::Uuid;

use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::config::Config;
use crate::server::ErrorResponse;
use crate::server::ListRunsResponse;
use crate::server::ListTasksResponse;
use crate::server::RunTaskCountsResponse;
use crate::server::Task;

/// CLI arguments for connecting to a Sprocket server instance.
#[derive(ClapArgs, Debug)]
pub struct SprocketClientConnectionArgs {
    /// The hostname of the running Sprocket server to talk to.
    ///
    /// If not provided, falls back to the value in the Sprocket config.
    #[arg(long)]
    pub host: Option<String>,

    /// The port of the running Sprocket server to talk to.
    ///
    /// If not provided, falls back to the value in the Sprocket config.
    #[arg(short, long)]
    pub port: Option<u16>,
}

impl SprocketClientConnectionArgs {
    /// Returns the base URL for the Sprocket server.
    pub fn base_url(&self, config: &Config) -> String {
        let host = self.host.as_deref().unwrap_or(&config.server.host);
        let port = self.port.unwrap_or(config.server.port);
        format!("http://{host}:{port}")
    }
}

/// Checks an HTTP response for errors, returning a [`CommandError`] for
/// non-2xx responses.
pub async fn check_response(resp: reqwest::Response) -> CommandResult<reqwest::Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let msg = serde_json::from_str::<ErrorResponse>(&body)
        .map(|e| format!("{}: {}", e.kind, e.message))
        .unwrap_or_else(|_| format!("HTTP {status}: {body}"));

    Err(CommandError::Single(anyhow::anyhow!(msg)))
}

/// Fetches the per-status task counts for a run.
///
/// Queries `GET /api/v1/runs/{uuid}/tasks/counts`. The endpoint reports
/// all-zero counts (rather than an error) for unknown runs.
pub async fn fetch_task_counts(base_url: &str, uuid: Uuid) -> CommandResult<RunTaskCountsResponse> {
    let url = format!("{base_url}/api/v1/runs/{uuid}/tasks/counts");
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .context("failed to connect to Sprocket server")?;

    let resp = check_response(resp).await?;

    let counts = resp
        .json()
        .await
        .context("failed to deserialize task counts response")?;

    Ok(counts)
}

/// Fetches all tasks for a run, following pagination across every page.
///
/// Queries `GET /api/v1/runs/{uuid}/tasks`. The returned tasks are sorted by
/// creation time ascending (oldest first), so callers see them in execution
/// order regardless of the server's page ordering.
pub async fn fetch_run_tasks(base_url: &str, uuid: Uuid) -> CommandResult<Vec<Task>> {
    let client = reqwest::Client::new();
    let mut next_token: Option<String> = None;
    let mut tasks = Vec::new();

    loop {
        let mut url = format!("{base_url}/api/v1/runs/{uuid}/tasks?limit=100");
        if let Some(token) = &next_token {
            url.push_str(&format!("&next_token={token}"));
        }

        let resp = client
            .get(&url)
            .send()
            .await
            .context("failed to connect to Sprocket server")?;

        let resp = check_response(resp).await?;

        let page: ListTasksResponse = resp
            .json()
            .await
            .context("failed to deserialize task list response")?;

        tasks.extend(page.tasks);
        next_token = page.next_token;

        if next_token.is_none() {
            break;
        }
    }

    tasks.sort_by_key(|task| task.created_at);

    Ok(tasks)
}

/// Resolves a run identifier (either a UUID or a human-readable generated name)
/// to a [`Uuid`] by querying the server.
///
/// If `input` parses directly as a [`Uuid`], it is returned immediately without
/// making any network requests.
///
/// Otherwise, all pages of `GET /api/v1/runs` are scanned for a run whose
/// `name` matches `input` exactly. If exactly one match is found, its UUID is
/// returned. If no match is found, or if multiple runs share the same name, a
/// descriptive error is returned.
pub async fn resolve_run_id(input: &str, base_url: &str) -> CommandResult<Uuid> {
    // Fast path: the input is already a UUID.
    if let Ok(uuid) = input.parse::<Uuid>() {
        return Ok(uuid);
    }

    // Slow path: scan all pages for a matching name.
    let client = reqwest::Client::new();
    let mut next_token: Option<String> = None;
    let mut matches: Vec<Uuid> = Vec::new();

    loop {
        let mut url = format!("{base_url}/api/v1/runs?limit=100");
        if let Some(token) = &next_token {
            url.push_str(&format!("&next_token={token}"));
        }

        let resp = client
            .get(&url)
            .send()
            .await
            .context("failed to connect to Sprocket server")?;

        let resp = check_response(resp).await?;

        let page: ListRunsResponse = resp
            .json()
            .await
            .context("failed to deserialize run list response")?;

        for run in &page.runs {
            if run.name == input {
                matches.push(run.uuid);
            }
        }

        next_token = page.next_token;
        if next_token.is_none() {
            break;
        }
    }

    match matches.len() {
        0 => Err(CommandError::Single(anyhow::anyhow!(
            "no run found with name or UUID `{input}`"
        ))),
        1 => Ok(matches[0]),
        _ => {
            let ids = matches
                .iter()
                .map(|u| format!("`{u}`"))
                .collect::<Vec<_>>()
                .join(", ");
            Err(CommandError::Single(anyhow::anyhow!(
                "multiple runs found with name `{input}` ({ids}); pass a UUID directly to \
                 disambiguate"
            )))
        }
    }
}
