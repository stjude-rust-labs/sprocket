//! The `wdl-doc` CLI.

use std::io::IsTerminal;
use std::io::stderr;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::bail;
use clap::Parser;
use clap::ValueEnum;
use wdl_analysis::Config as AnalysisConfig;
use wdl_ast::AstNode;
use wdl_ast::Severity;
use wdl_diagnostics::DiagnosticCounts;
use wdl_diagnostics::Mode;
use wdl_diagnostics::emit_diagnostics;
use wdl_doc::AdditionalScript;
use wdl_doc::Config;
use wdl_doc::error::DocErrorKind;

/// Represents the supported output color modes.
#[derive(Debug, Default, Clone, ValueEnum, Copy, PartialEq, Eq, Hash)]
pub enum ColorMode {
    /// Automatically colorize output depending on output device.
    #[default]
    Auto,
    /// Always colorize output.
    Always,
    /// Never colorize output.
    Never,
}

/// Arguments for the `wdl-doc` binary.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Path to the local WDL workspace to document.
    pub workspace: PathBuf,
    /// Path to a Markdown file to embed in the `<output>/index.html` file.
    #[arg(long, value_name = "MARKDOWN FILE")]
    pub homepage: Option<PathBuf>,
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
    /// Initialize pages on the "Workflows" view instead of the "Full
    /// Directory" view of the left nav bar.
    #[arg(short, long)]
    pub prioritize_workflows_view: bool,
    /// Output directory for the generated documentation.
    #[arg(long, value_name = "DIR")]
    pub output: PathBuf,
    /// Overwrite any existing documentation.
    ///
    /// If specified, any existing files in the output directory will be
    /// deleted. Otherwise, the command will ignore existing files.
    /// Regardless of this flag, the command will overwrite any existing
    /// files which conflict with the generated documentation.
    #[arg(long)]
    pub overwrite: bool,
    /// Path to a `.js` file that should have its contents embedded in a
    /// `<script>` tag for each HTML page, immediately after the opening
    /// `<head>` tag.
    #[arg(long, value_name = "JS FILE", conflicts_with_all = [
        "javascript_head_close", "javascript_body_open", "javascript_body_close"
    ])]
    pub javascript_head_open: Option<PathBuf>,
    /// Path to a `.js` file that should have its contents embedded in a
    /// `<script>` tag for each HTML page, immediately before the closing
    /// `<head>` tag.
    #[arg(long, value_name = "JS FILE", conflicts_with_all = [
        "javascript_body_open", "javascript_body_close"
    ])]
    pub javascript_head_close: Option<PathBuf>,
    /// Path to a `.js` file that should have its contents embedded in a
    /// `<script>` tag for each HTML page, immediately after the opening
    /// `<body>` tag.
    #[arg(long, value_name = "JS FILE", conflicts_with_all = [
        "javascript_body_close"
    ])]
    pub javascript_body_open: Option<PathBuf>,
    /// Path to a `.js` file that should have its contents embedded in a
    /// `<script>` tag for each HTML page, immediately before the closing
    /// `<body>` tag.
    #[arg(long, value_name = "JS FILE")]
    pub javascript_body_close: Option<PathBuf>,
    /// An optional path to a custom theme directory.
    ///
    /// This argument is meant to be used by developers of the `wdl` crates;
    /// customizing the theme used for the generated documentation is currently
    /// unsupported.
    #[arg(long, value_name = "DIR")]
    pub theme: Option<PathBuf>,
    /// Install the theme if it is not already installed.
    ///
    /// `npm` and `npx` are expected to be available in the environment.
    #[arg(long, requires = "theme")]
    pub install: bool,
    /// Enables support for documentation comments
    ///
    /// This option is *experimental* and will be removed in a future major
    /// version. Follow the pre-RFC discussion here: <https://github.com/openwdl/wdl/issues/757>.
    #[arg(long)]
    pub with_doc_comments: bool,

    /// Controls output colorization.
    #[arg(long, default_value = "auto")]
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

    if args.with_doc_comments {
        tracing::warn!(
            "the `--with-doc-comments` flag is **experimental** and will be removed in a future major version. See https://github.com/openwdl/wdl/issues/757"
        );
    }

    if args.install {
        if let Some(theme_path) = &args.theme {
            wdl_doc::install_theme(theme_path).with_context(|| {
                format!("failed to install theme from `{}`", theme_path.display())
            })?;
        } else {
            bail!("the `--install` flag requires the `--theme` argument to be specified");
        }
    }

    if let Some(theme) = &args.theme {
        wdl_doc::build_stylesheet(theme).with_context(|| {
            format!(
                "failed to build stylesheet for theme at `{}`",
                theme.display()
            )
        })?;
        wdl_doc::build_web_components(theme).with_context(|| {
            format!(
                "failed to build web components for theme at `{}`",
                theme.display()
            )
        })?;
    }

    let addl_js = match (
        &args.javascript_head_open,
        &args.javascript_head_close,
        &args.javascript_body_open,
        &args.javascript_body_close,
    ) {
        (Some(path), ..) => {
            let js = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read JavaScript file: {}", path.display()))?;
            AdditionalScript::HeadOpen(js)
        }
        (_, Some(path), ..) => {
            let js = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read JavaScript file: {}", path.display()))?;
            AdditionalScript::HeadClose(js)
        }
        (_, _, Some(path), _) => {
            let js = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read JavaScript file: {}", path.display()))?;
            AdditionalScript::BodyOpen(js)
        }
        (_, _, _, Some(path)) => {
            let js = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read JavaScript file: {}", path.display()))?;
            AdditionalScript::BodyClose(js)
        }
        _ => AdditionalScript::None,
    };

    if args.overwrite && args.output.exists() {
        std::fs::remove_dir_all(&args.output).context("failed to delete docs directory")?;
    }

    let analysis_config = AnalysisConfig::load()?;
    let config = Config::new(analysis_config, &args.workspace, &args.output)
        .homepage(args.homepage)
        .init_light_mode(args.light_mode)
        .custom_theme(args.theme)
        .custom_logo(args.logo)
        .alt_logo(args.alt_light_logo)
        .additional_javascript(addl_js)
        .prefer_full_directory(!args.prioritize_workflows_view)
        .enable_doc_comments(args.with_doc_comments);

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
