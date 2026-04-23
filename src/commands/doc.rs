//! Implementation of the `doc` command.

use std::path::PathBuf;

use anyhow::Context;
use anyhow::anyhow;
use clap::Parser;
use url::Url;
use wdl::analysis::Config as AnalysisConfig;
use wdl::analysis::DiagnosticsConfig;
use wdl::ast::AstNode;
use wdl::ast::Severity;
use wdl::diagnostics::DiagnosticCounts;
use wdl::diagnostics::Mode;
use wdl::diagnostics::emit_diagnostics;
use wdl::doc::Config as DocConfig;
use wdl::doc::build_stylesheet;
use wdl::doc::build_web_components;
use wdl::doc::config::AdditionalHtml;
use wdl::doc::config::ExternalUrls;
use wdl::doc::document_workspace;
use wdl::doc::error::DocErrorKind;
use wdl::doc::install_theme;

use crate::Config;
use crate::IGNORE_FILENAME;
use crate::analysis::Source;
use crate::commands::CommandResult;

/// Arguments for the `doc` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Path to the local WDL workspace to document.
    pub workspace: Option<Source>,
    /// Analyze the documents without producing an output.
    #[arg(long, conflicts_with_all = ["output", "open", "overwrite"])]
    pub check: bool,
    /// Path to a Markdown file to embed in the `<output>/index.html` file.
    #[arg(long, value_name = "MARKDOWN FILE")]
    pub index_page: Option<PathBuf>,
    /// Path to an SVG logo to embed on each page.
    ///
    /// If not supplied, the default Sprocket logo will be used.
    #[arg(long, value_name = "SVG FILE")]
    pub logo: Option<PathBuf>,
    /// An optional link to the project's homepage.
    #[arg(long, value_name = "LINK TO HOMEPAGE")]
    pub homepage_url: Option<Url>,
    /// An optional link to the project's GitHub repository.
    #[arg(long, value_name = "LINK TO GITHUB")]
    pub github_url: Option<Url>,
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
    /// Path to an HTML file that should have its contents embedded in
    /// each HTML page, immediately before the closing `<head>` tag.
    #[arg(long, value_name = "FILE")]
    pub html_head: Option<PathBuf>,
    /// Path to an HTML file that should have its contents embedded in
    /// each HTML page, immediately after the opening `<body>` tag.
    #[arg(long, value_name = "FILE")]
    pub html_body_open: Option<PathBuf>,
    /// Path to an HTML file that should have its contents embedded in
    /// each HTML page, immediately before the closing `<body>` tag.
    #[arg(long, value_name = "FILE")]
    pub html_body_close: Option<PathBuf>,
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
    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,
}

/// The default output directory for the generated documentation.
const DEFAULT_OUTPUT_DIR: &str = "docs";

/// Generate documentation for a WDL workspace.
pub async fn doc(args: Args, config: Config, colorize: bool) -> CommandResult<()> {
    if args.with_doc_comments {
        tracing::warn!(
            "the `--with-doc-comments` flag is **experimental** and will be removed in a future major version. See https://github.com/openwdl/wdl/issues/757"
        );
    } else if config.doc.with_doc_comments {
        tracing::warn!(
            "documentation comments support is **experimental**. See https://github.com/openwdl/wdl/issues/757"
        );
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

    let head = args
        .html_head
        .or(config.doc.extra_html.head())
        .map(|path| {
            std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read HTML file: {}", path.display()))
        })
        .transpose()?;
    let body_open = args
        .html_body_open
        .or(config.doc.extra_html.body_open())
        .map(|path| {
            std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read HTML file: {}", path.display()))
        })
        .transpose()?;
    let body_close = args
        .html_body_close
        .or(config.doc.extra_html.body_close())
        .map(|path| {
            std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read HTML file: {}", path.display()))
        })
        .transpose()?;
    let addl_html = AdditionalHtml::new(head, body_open, body_close);

    let docs_dir = args.output.unwrap_or(workspace.join(DEFAULT_OUTPUT_DIR));

    if args.overwrite && docs_dir.exists() {
        std::fs::remove_dir_all(&docs_dir).context("failed to delete docs directory")?;
    }

    let analysis_config = AnalysisConfig::default()
        .with_fallback_version(config.common.wdl.fallback_version.inner().cloned())
        .with_ignore_filename(Some(IGNORE_FILENAME.to_string()))
        .with_diagnostics_config(DiagnosticsConfig::except_all());

    let index_page = args.index_page.or(config.doc.index_page());
    let light_mode = args.light_mode || config.doc.light_mode;
    let logo = args.logo.or(config.doc.logo());
    let alt_light_logo = args.alt_light_logo.or(config.doc.alt_light_logo());
    let homepage_url = args.homepage_url.or(config.doc.homepage_url());
    let github_url = args.github_url.or(config.doc.github_url());
    let with_doc_comments = args.with_doc_comments || config.doc.with_doc_comments;

    let config = DocConfig::new(analysis_config, &workspace, &docs_dir)
        .index_page(index_page)
        .init_light_mode(light_mode)
        .custom_theme(args.theme)
        .custom_logo(logo)
        .alt_logo(alt_light_logo)
        .external_urls(ExternalUrls {
            homepage: homepage_url,
            github: github_url,
        })
        .additional_html(addl_html)
        .enable_doc_comments(with_doc_comments)
        .check(args.check);

    let mut counts = DiagnosticCounts::default();
    if let Err(e) = document_workspace(config).await {
        match e.kind() {
            DocErrorKind::AnalysisFailed(analysis_results) => {
                for result in analysis_results {
                    let path = result.document().path().to_string();
                    let source = result.document().root().text().to_string();

                    emit_diagnostics(
                        &path,
                        &source,
                        result.document().diagnostics().filter(|d| {
                            if d.severity() == Severity::Error {
                                counts.errors += 1;
                                return true;
                            }

                            false
                        }),
                        args.report_mode.unwrap_or_default(),
                        colorize,
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
