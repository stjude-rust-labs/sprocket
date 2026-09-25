//! Implementation of the `cancel` subcommand.

use clap::Parser;
use tracing::debug;

use crate::commands::CommandResult;
use crate::commands::client::ServerConnectionArgs;
use crate::commands::client::fetch_server_info;
use crate::commands::client::resolve_run_id;
use crate::commands::client::send_json;
use crate::config::Config;
use crate::server::CancelRunResponse;
use crate::server::ServerFailureMode;
use crate::server::paths;

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

    let url = format!("{base_url}{path}", path = paths::cancel_run(uuid));
    let body: CancelRunResponse = send_json(reqwest::Client::new().post(&url), "cancel").await?;

    println!(
        "Run `{uuid}` has been signaled for cancellation.",
        uuid = body.uuid,
    );

    // Only print the slow-cancel advisory when the server is actually running
    // in slow-failure mode. The fetch is best-effort: if `/info` is
    // unavailable (e.g. older server) we silently skip the note rather than
    // failing the overall command, since the cancel itself already succeeded.
    match fetch_server_info(&base_url).await {
        Ok(info) if info.failure_mode == ServerFailureMode::Slow => {
            println!(
                "Note: in slow-failure mode, currently executing tasks will be allowed to finish \
                 before the run is marked as canceled. Use `sprocket dev server status {uuid}` to \
                 track progress.",
                uuid = body.uuid,
            );
        }
        Ok(_) => {}
        Err(err) => {
            debug!("failed to fetch server info while preparing cancel advisory: {err:#}");
        }
    }

    Ok(())
}
