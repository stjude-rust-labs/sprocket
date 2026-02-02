//! Implementation of the `doc` command.

use std::path::PathBuf;

use anyhow::Context;
use anyhow::anyhow;
use clap::Parser;
use wdl::analysis::Config as AnalysisConfig;
use wdl::analysis::DiagnosticsConfig;
use wdl::ast::AstNode;
use wdl::ast::Severity;
use wdl::doc::AdditionalScript;
use wdl::doc::Config;
use wdl::doc::build_stylesheet;
use wdl::doc::build_web_components;
use wdl::doc::document_workspace;
use wdl::doc::error::DocErrorKind;
use wdl::doc::install_theme;

use crate::IGNORE_FILENAME;
use crate::analysis::Source;
use crate::commands::CommandResult;
use crate::diagnostics::DiagnosticCounts;
use crate::diagnostics::Mode;
use crate::diagnostics::emit_diagnostics;

/// Arguments for the `doc` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Path to the local WDL workspace to document.
    pub workspace: Option<Source>,
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
    /// If not specified, the documentation will be generated in
    /// `<workspace>/docs`.
    #[arg(long, value_name = "DIR")]
    pub output: Option<PathBuf>,
    /// Overwrite any existing documentation.
    ///
    /// If specified, any existing files in the output directory will be
    /// deleted. Otherwise, the command will ignore existing files.
    /// Regardless of this flag, the command will overwrite any existing
    /// files which conflict with the generated documentation.
    #[arg(long)]
    pub overwrite: bool,
    /// Open the generated documentation in the default web browser.
    #[arg(long)]
    pub open: bool,
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

    // Diagnostics
    /// Disables color output.
    #[arg(long)]
    pub no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,
}

/// The default output directory for the generated documentation.
const DEFAULT_OUTPUT_DIR: &str = "docs";

/// Generate documentation for a WDL workspace.
pub async fn doc(args: Args) -> CommandResult<()> {
    if args.with_doc_comments {
        tracing::warn!("the `--with-doc-comments` flag is **experimental** and will be removed in a future major version. See https://github.com/openwdl/wdl/issues/757");
    }

    let workspace = if let Source::Directory(workspace) = args.workspace.unwrap_or_default() {
        workspace
    } else {
        return Err(anyhow!("`workspace` must be a local directory for the `doc` command").into());
    };
    if args.install {
        if let Some(theme_path) = &args.theme {
            install_theme(theme_path).with_context(|| {
                format!("failed to install theme from `{}`", theme_path.display())
            })?;
        } else {
            return Err(anyhow!(
                "the `--install` flag requires the `--theme` argument to be specified"
            )
            .into());
        }
    }

    if let Some(theme) = &args.theme {
        build_stylesheet(theme).with_context(|| {
            format!(
                "failed to build stylesheet for theme at `{}`",
                theme.display()
            )
        })?;
        build_web_components(theme).with_context(|| {
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

    let docs_dir = args.output.unwrap_or(workspace.join(DEFAULT_OUTPUT_DIR));

    if args.overwrite && docs_dir.exists() {
        std::fs::remove_dir_all(&docs_dir).context("failed to delete docs directory")?;
    }

    let analysis_config = AnalysisConfig::default()
        .with_ignore_filename(Some(IGNORE_FILENAME.to_string()))
        .with_diagnostics_config(DiagnosticsConfig::except_all());
    let config = Config::new(analysis_config, &workspace, &docs_dir)
        .homepage(args.homepage)
        .init_light_mode(args.light_mode)
        .custom_theme(args.theme)
        .custom_logo(args.logo)
        .alt_logo(args.alt_light_logo)
        .additional_javascript(addl_js)
        .prefer_full_directory(!args.prioritize_workflows_view)
        .enable_doc_comments(args.with_doc_comments);

    let mut counts = DiagnosticCounts::default();
    if let Err(e) = document_workspace(config).await {
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
                        &[],
                        args.report_mode.unwrap_or_default(),
                        args.no_color,
                    )
                    .context("failed to emit diagnostics")?;
                }
            }
            _ => {
                return Err(anyhow::Error::new(e)
                    .context(format!(
                        "failed to generate documentation for workspace at `{}`",
                        workspace.display()
                    ))
                    .into());
            }
        }
    }

    if let Some(e) = counts.verify_no_errors() {
        return Err(e.into());
    }

    if args.open {
        opener::open(docs_dir.join("index.html")).context("failed to open documentation")?;
    }

    Ok(())
}
