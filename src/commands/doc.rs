//! Implementation of the `doc` command.

use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

use anyhow::Context;
use anyhow::anyhow;
use clap::Parser;
use wdl::analysis::Config as AnalysisConfig;
use wdl::analysis::DiagnosticsConfig;
use wdl::diagnostics::Mode;

use crate::ColorMode;
use crate::Config;
use crate::IGNORE_FILENAME;
use crate::analysis::Source;
use crate::commands::CommandDebugExt;
use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::sprocket_components_dir;

/// Arguments for the `doc` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    #[arg(from_global)]
    pub data_dir: Option<PathBuf>,
    /// Path to the local WDL workspace to document.
    pub workspace: Option<Source>,
    /// Output directory for the generated documentation.
    /// If not specified, the documentation will be generated in
    /// `<workspace>/docs`.
    #[arg(long, value_name = "DIR")]
    pub output: Option<PathBuf>,
    /// Open the generated documentation in the default web browser.
    #[arg(long)]
    pub open: bool,

    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,

    /// All remaining arguments, passed to `wdl-doc`.
    #[arg(last = true)]
    pub doc_args: Vec<String>,
}

/// The default output directory for the generated documentation.
const DEFAULT_OUTPUT_DIR: &str = "docs";
/// The name of the `wdl-doc` binary.
const COMPONENT_NAME: &str = "wdl-doc";

/// Generate documentation for a WDL workspace.
pub async fn doc(args: Args, config: Config, color_mode: ColorMode) -> CommandResult<()> {
    let component_dir = sprocket_components_dir(args.data_dir.as_deref())?;
    let wdl_doc_bin = component_dir.join(COMPONENT_NAME);
    if !wdl_doc_bin.exists() {
        return Err(CommandError::MissingComponent {
            component: COMPONENT_NAME,
            component_dir,
        });
    }

    let Source::Directory(workspace) = args.workspace.unwrap_or_default() else {
        return Err(anyhow!("`workspace` must be a local directory for the `doc` command").into());
    };

    let docs_dir = args.output.unwrap_or(workspace.join(DEFAULT_OUTPUT_DIR));

    let analysis_args = AnalysisConfig::default()
        .with_fallback_version(config.common.wdl.fallback_version)
        .with_ignore_filename(Some(IGNORE_FILENAME.to_string()))
        .with_diagnostics_config(DiagnosticsConfig::except_all())
        .as_args()
        .map_err(Into::<anyhow::Error>::into)?;
    let mut command = Command::new(wdl_doc_bin);
    command
        .args(args.doc_args)
        .arg(format!("--output={}", docs_dir.to_string_lossy()))
        .arg(&workspace)
        .env("WDL_ANALYSIS_ARGS", analysis_args)
        .env("WDL_DOC_COLOR_MODE", color_mode.to_string())
        .envs(std::env::vars())
        .stderr(Stdio::inherit())
        .stdout(Stdio::inherit());

    tracing::trace!(target: "commands", "Invoking `{:?}`", command.debug());
    let status = command.status().map_err(Into::<anyhow::Error>::into)?;
    if !status.success() {
        return Err(anyhow!("`{COMPONENT_NAME}` did not exit successfully").into());
    }

    if args.open {
        opener::open(docs_dir.join("index.html")).context("failed to open documentation")?;
    }

    Ok(())
}
