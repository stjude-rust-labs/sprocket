//! Implementation of the `validate-inputs` command.

use std::path::PathBuf;

use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use wdl::cli::validate_inputs as wdl_validate_inputs;

use crate::Mode;
use crate::emit_diagnostics;
use crate::input;
use crate::input::override::InputOverride;

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

    /// Input overrides in key=value format
    #[arg(value_name = "KEY=VALUE")]
    pub overrides: Vec<String>,

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
    // Parse base JSON/YAML
    let (_, mut json_value) = input::parse_input_file(&args.inputs)?;

    // Parse and apply overrides
    let overrides: Vec<InputOverride> = args.overrides
        .iter()
        .map(|s| InputOverride::parse(s))
        .collect::<Result<_>>()?;

    if !overrides.is_empty() {
        json_value = input::override_mod::apply_overrides(json_value, &overrides)?;
    }

    if let Some(diagnostic) = wdl_validate_inputs(&args.document, &json_value).await? {
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
    Ok(())
}
