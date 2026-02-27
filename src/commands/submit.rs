//! Implementation of the `submit` subcommand
//!
//! A wrapper around the Sprocket REST API to submit a new workflow!

use std::fs::File;
use std::path::PathBuf;

use anyhow::Context;
use clap::Args as ClapArgs;
use clap::Parser;

use crate::commands::CommandError;
use crate::config::Config;
use crate::server::ErrorResponse;
use crate::server::SubmitResponse;
use crate::server::SubmitRunRequest;

/// CLI Arguments for connecting to a Sprocket server instance.
#[derive(ClapArgs, Debug)]
pub struct SprocketClientConnectionArgs {
    /// The hostname of the running Sprocket server to talk to.
    /// If not provided, falls back to the value in the Sprocket Config.
    #[arg(long)]
    host: Option<String>,
    /// The port of the running Sprocket server to talk to.
    /// If not provided, falls back to the value in the Sprocket Config.
    #[arg(long)]
    port: Option<u16>,
}

// TODO: This duplicates a fair bit from `SubmitRunRequest`. Most notably
// the doc comments. We could just point people to the documentation, but maybe
// worth the duplication to have it in the CLI help message?
//
/// CLI Arguments for specifying the body of the SubmitRunRequest.
#[derive(ClapArgs, Debug)]
pub struct SubmitRunRequestArgs {
    /// Optional output name to index on.
    /// If provided, the run outputs will be indexed.
    #[arg(long)]
    index_on: Option<String>,

    /// Run inputs as a JSON object.
    #[arg(long, required_unless_present = "inputs_path")]
    inputs: Option<String>,

    /// A file to read the given inputs from.
    #[arg(long, conflicts_with = "inputs")]
    inputs_path: Option<PathBuf>,

    /// WDL source path (local file path or HTTP/HTTPS URL).
    #[arg(long)]
    source: String,

    #[rustfmt::skip]
    /// Optional target workflow or task name to execute.
    ///
    /// If not provided, will attempt to automatically select:
    /// 1.) The workflow in the document (if one exists)
    /// 2.) The single task in the document (if no workflow but exactly one task)
    /// 3.) Error if ambiguous (no workflow and multiple tasks)
    #[arg(long, verbatim_doc_comment)]
    target: Option<String>,
}

impl TryFrom<SubmitRunRequestArgs> for SubmitRunRequest {
    type Error = anyhow::Error;

    fn try_from(value: SubmitRunRequestArgs) -> Result<Self, Self::Error> {
        let SubmitRunRequestArgs {
            index_on,
            inputs,
            inputs_path,
            source,
            target,
        } = value;

        let inputs = match (inputs, inputs_path) {
            (Some(value), None) => {
                serde_json::from_str(&value).context("Parsing workflow inputs as json")?
            }
            (None, Some(path_to_inputs)) => serde_json::from_reader(
                &File::open(path_to_inputs).context("Opening provided inputs file")?,
            )?,
            otherwise => {
                return Err(anyhow::anyhow!(
                    "Invalid `inputs` options should be blocked by Clap Arg parsing: {:?}",
                    otherwise
                ));
            }
        };

        Ok(Self {
            index_on,
            inputs,
            source,
            target,
        })
    }
}

/// Arguments for the `submit` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    #[command(flatten)]
    client_args: SprocketClientConnectionArgs,

    #[command(flatten)]
    run_request_args: SubmitRunRequestArgs,
}

/// Handles the `submit` subcommand.
///
/// Submits a workflow to a Sprocket server based on the Args / Config.
pub async fn submit(args: Args, config: Config) -> Result<(), CommandError> {
    let host = args.client_args.host.unwrap_or(config.server.host);
    let port = args.client_args.port.unwrap_or(config.server.port);

    let url = format!("http://{host}:{port}/api/v1/runs", host = host, port = port);
    let request = SubmitRunRequest::try_from(args.run_request_args)
        .context("Building the SubmitRunRequest from CLI Args")?;

    let resp = reqwest::Client::new()
        .post(url)
        .json(&request)
        .send()
        .await
        .context("Sending Request")?;

    if !resp.status().is_success() {
        let error_response: ErrorResponse = resp
            .json()
            .await
            .context("Expected a JSON API Error Response from the Sprocket API")?;

        return Err(CommandError::Single(anyhow::anyhow!(
            "Error from Sprocket Server ({}): {}",
            error_response.kind,
            error_response.message
        )));
    }

    let submit_response: SubmitResponse = resp
        .json()
        .await
        .context("Expected a response body for successful SubmitRunRequest")?;

    println!(
        "Run Created: \nuuid = {} \nname = {}",
        submit_response.uuid, submit_response.name
    );

    Ok(())
}
