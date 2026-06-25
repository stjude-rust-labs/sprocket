//! Implementation of the `cancel` subcommand.

use anyhow::Context;
use clap::Parser;

use crate::commands::CommandResult;
use crate::commands::client::ServerConnectionArgs;
use crate::commands::client::check_response;
use crate::commands::client::resolve_run_id;
use crate::config::Config;
use crate::server::CancelRunResponse;

/// Arguments for the `cancel` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The run to cancel.
    ///
    /// May be a UUID or the human-readable generated name of the run (e.g.
    /// `happy-dolphin-42`).
    #[clap(value_name = "RUN")]
    run_id: String,

    #[command(flatten)]
    client_args: ServerConnectionArgs,
}

/// Handles the `cancel` subcommand.
///
/// Sends a cancellation request to the server for the specified run.
pub async fn cancel(args: Args, config: Config) -> CommandResult<()> {
    let base_url = args.client_args.base_url(&config);
    let uuid = resolve_run_id(&args.run_id, &base_url).await?;

    let url = format!("{base_url}/api/v1/runs/{uuid}/cancel");
    let resp = reqwest::Client::new()
        .post(&url)
        .send()
        .await
        .context("failed to connect to Sprocket server")?;

    let resp = check_response(resp).await?;

    let body: CancelRunResponse = resp
        .json()
        .await
        .context("failed to deserialize cancel response")?;

    println!(
        "Run `{uuid}` has been signaled for cancellation.",
        uuid = body.uuid,
    );
    println!(
        "Note: in slow-failure mode, currently executing tasks will be allowed to finish before \
         the run is marked as canceled. Use `sprocket dev server status {uuid}` to track progress.",
        uuid = body.uuid,
    );

    Ok(())
}
