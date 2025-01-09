//! Implementation of the `validate-inputs` command.

use std::path::PathBuf;

use anyhow::Result;
use anyhow::Ok;
use clap::Parser;
use wdl::cli::validate_inputs as wdl_validate_inputs;

use crate::Mode;
use crate::get_display_config;

/// Arguments for the `validate-inputs` command.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct ValidateInputsArgs {
    /// The path or URL to the WDL document.
    #[arg(required = true)]
    #[clap(value_name = "PATH or URL")]
    pub document: String,

    /// The path to the input JSON file.
    #[arg(short, long, value_name = "JSON")]
    pub inputs: PathBuf,

    /// Disables color output.
    #[arg(long)]
    pub no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, default_value_t, value_name = "MODE")]
    pub report_mode: Mode,
}

/// Validates the inputs for a task or workflow.
///
/// * Every required input is supplied.
/// * Every supplied input is correctly typed.
/// * No extraneous inputs are provided.
pub async fn validate_inputs(args: ValidateInputsArgs) -> Result<()> {
    let (config, mut stream) = get_display_config(args.report_mode, args.no_color);

    wdl_validate_inputs(&args.document, &args.inputs, &mut stream, &config).await?;
    println!("All inputs are valid");
    Ok(())
}
