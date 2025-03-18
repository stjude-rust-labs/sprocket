//! Implementation of the `validate-inputs` command.

use std::path::PathBuf;

use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use wdl::cli::validate_inputs as wdl_validate_inputs;

use crate::Mode;
use crate::emit_diagnostics;
use crate::input;

/// Arguments for the `validate-inputs` command.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct ValidateInputsArgs {
    /// The path or URL to the WDL document.
    #[arg(required = true)]
    #[clap(value_name = "PATH or URL")]
    pub document: String,

    /// The path to the input file (JSON or YAML).
    #[arg(short, long, value_name = "FILE")]
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
/// * Any provided `File` or `Directory` inputs exist.
///
/// This command supports both JSON and YAML input files.
pub async fn validate_inputs(args: ValidateInputsArgs) -> Result<()> {
    // Get a path to a JSON file
    // For JSON inputs, this returns the original path
    // For YAML inputs, this creates a temporary JSON file
    let input_path = input::get_json_file_path(args.inputs)?;

    if let Some(diagnostic) = wdl_validate_inputs(&args.document, &input_path).await? {
        let source = std::fs::read_to_string(&args.document)?;
        emit_diagnostics(
            &[diagnostic],
            &args.document,
            &source,
            args.report_mode,
            args.no_color,
        );
        bail!("Invalid inputs");
    }
    println!("All inputs are valid");
    anyhow::Ok(())
}
