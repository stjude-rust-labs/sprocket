//! Implementation of the run command.

use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use chrono;
use clap::Parser;
use tracing_log::log;
use wdl::cli::run as wdl_run;

use crate::Mode;
use crate::get_display_config;

/// Arguments for the run command.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct RunArgs {
    /// The path or URL to the WDL document containing the task to run.
    #[arg(value_name = "PATH or URL")]
    pub file: String,

    /// The path to the input JSON file.
    ///
    /// If not provided, an empty set of inputs will be sent to the task.
    #[arg(short, long, value_name = "JSON", conflicts_with = "name")]
    pub inputs: Option<PathBuf>,

    /// The name of the task to run.
    ///
    /// Required if no `inputs` file is provided.
    #[arg(short, long, value_name = "NAME", conflicts_with = "inputs")]
    pub name: Option<String>,

    /// The output directory; defaults to the task name.
    ///
    /// If no output directory is provided, a default nested directory is
    /// created based on the task name and the current time in the form
    /// `sprocket_runs/<execution_name>/<timestamp>/`.
    #[arg(short, long, value_name = "DIR")]
    pub output: Option<PathBuf>,

    /// Overwrite the output directory if it exists.
    #[arg(long)]
    pub overwrite: bool,

    /// Disables color output.
    #[arg(long)]
    pub no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, default_value_t, value_name = "MODE")]
    pub report_mode: Mode,
}

/// Creates the output directory for the task.
fn create_output_dir(output_dir: Option<PathBuf>, name: &str, overwrite: bool) -> Result<PathBuf> {
    let output_dir = output_dir.unwrap_or_else(|| {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H-%M-%S");
        PathBuf::from(format!("sprocket_runs/{}/{}", name, timestamp))
    });

    let output_dir = if output_dir.exists() {
        if overwrite {
            fs::remove_dir_all(&output_dir).with_context(|| {
                format!(
                    "failed to remove output directory `{dir}`",
                    dir = output_dir.display()
                )
            })?;
            output_dir
        } else {
            bail!(
                "output directory `{dir}` already exists; use the `--overwrite` option to \
                 overwrite it",
                dir = output_dir.display()
            );
        }
    } else {
        output_dir
    };

    fs::create_dir_all(&output_dir).with_context(|| {
        format!(
            "failed to create output directory `{dir}`",
            dir = output_dir.display()
        )
    })?;

    log::info!(
        "output directory created: `{dir}`",
        dir = output_dir.display()
    );

    Ok(output_dir)
}

/// Runs a task.
pub async fn run(args: RunArgs) -> Result<()> {
    eprintln!(
        "the `run` command is in alpha testing and does not currently support workflows or using \
         containers."
    );

    let (config, mut stream) = get_display_config(args.report_mode, args.no_color);

    let output_dir = create_output_dir(
        args.output,
        args.name.as_deref().unwrap_or("task"),
        args.overwrite,
    )?;

    wdl_run(
        &args.file,
        args.inputs,
        args.name,
        output_dir,
        &mut stream,
        &config,
    )
    .await
}
