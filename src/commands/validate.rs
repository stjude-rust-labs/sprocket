//! Implementation of the `validate-inputs` command.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use wdl::cli::validate_inputs as wdl_validate_inputs;

use crate::Mode;
use crate::emit_diagnostics;

/// Arguments for the `validate-inputs` command.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct ValidateInputsArgs {
    /// The path or URL to the WDL document.
    #[arg(required = true)]
    #[clap(value_name = "PATH or URL")]
    pub document: String,

    /// The path to the input JSON or YAML file.
    #[arg(short, long, value_name = "INPUTS")]
    pub inputs: PathBuf,

    /// Disables color output.
    #[arg(long)]
    pub no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,
}

/// Validates the inputs for a task or workflow.
///
/// * Every required input is supplied.
/// * Every supplied input is correctly typed.
/// * No extraneous inputs are provided.
/// * Any provided `File` or `Directory` inputs exist.
pub async fn validate_inputs(args: ValidateInputsArgs) -> Result<()> {
    if let Some(diagnostic) = wdl_validate_inputs(&args.document, &args.inputs).await? {
        let source = std::fs::read_to_string(&args.document)?;
        emit_diagnostics(
            &[diagnostic],
            &args.document,
            &source,
            args.report_mode.unwrap_or_default(),
            args.no_color,
        );
        anyhow::bail!("Invalid inputs");
    }
    println!("All inputs are valid");
    anyhow::Ok(())
}
