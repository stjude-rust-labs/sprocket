//! The `wdl-doc` CLI.

use std::io::IsTerminal;
use std::io::stderr;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use wdl_analysis::Config as AnalysisConfig;
use wdl_analysis::DiagnosticsConfig;
use wdl_ast::AstNode;
use wdl_ast::Severity;
use wdl_diagnostics::ColorMode;
use wdl_diagnostics::DiagnosticCounts;
use wdl_diagnostics::Mode;
use wdl_diagnostics::emit_diagnostics;
use wdl_doc::Config;
use wdl_doc::error::DocErrorKind;

/// Arguments for the `wdl-doc` binary.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Path to the local WDL workspace to document.
    pub workspace: PathBuf,
    /// Path to an SVG logo to embed on each page.
    ///
    /// If not supplied, the default Sprocket logo will be used.
    #[arg(long, value_name = "SVG FILE")]
    pub logo: Option<PathBuf>,
    /// Path to an alternate light mode SVG logo to embed on each page.
    ///
    /// If not supplied, the `--logo` SVG will be used; or if that is also not
    /// supplied, the default Sprocket logo will be used.
    #[arg(long, value_name = "SVG FILE")]
    pub alt_light_logo: Option<PathBuf>,
    /// Initialize pages in light mode instead of the default dark mode.
    #[arg(short, long)]
    pub light_mode: bool,
    /// Output directory for the generated documentation.
    #[arg(long, value_name = "DIR")]
    pub output: PathBuf,

    /// Controls output colorization.
    #[arg(long, default_value = "auto", env = "WDL_DOC_COLOR_MODE")]
    pub color: ColorMode,
    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    let analysis_config = AnalysisConfig::default()
        .with_ignore_filename(Some(String::from(".sprocketignore")))
        .with_diagnostics_config(DiagnosticsConfig::except_all());
    let config = Config::new(analysis_config, &args.workspace, &args.output)
        .init_light_mode(args.light_mode)
        .custom_logo(args.logo)
        .alt_logo(args.alt_light_logo);

    let color = match args.color {
        ColorMode::Auto => stderr().is_terminal(),
        ColorMode::Always => true,
        ColorMode::Never => false,
    };

    let mut counts = DiagnosticCounts::default();
    if let Err(e) = wdl_doc::document_workspace(config).await {
        match e.kind() {
            DocErrorKind::AnalysisFailed(analysis_results) => {
                for result in analysis_results {
                    let path = result.document().path().to_string();
                    let source = result.document().root().text().to_string();

                    emit_diagnostics(
                        &path,
                        source,
                        result.document().diagnostics().filter(|d| {
                            if d.severity() == Severity::Error {
                                counts.errors += 1;
                                return true;
                            }

                            false
                        }),
                        args.report_mode.unwrap_or_default(),
                        color,
                    )
                    .context("failed to emit diagnostics")?;
                }
            }
            _ => {
                return Err(anyhow::Error::new(e).context(format!(
                    "failed to generate documentation for workspace at `{}`",
                    args.workspace.display()
                )));
            }
        }
    }

    if let Some(e) = counts.verify_no_errors() {
        return Err(e);
    }

    Ok(())
}
