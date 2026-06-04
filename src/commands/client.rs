//! Shared client utilities for commands that communicate with a Sprocket server.

use anyhow::Context;
use clap::Args as ClapArgs;
use uuid::Uuid;

use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::config::Config;
use crate::server::ErrorResponse;
use crate::server::ListRunsResponse;

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
                .map(|u| u.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            Err(CommandError::Single(anyhow::anyhow!(
                "multiple runs found with name `{input}` ({ids}); \
                 pass a UUID directly to disambiguate"
            )))
        }
    }
}
