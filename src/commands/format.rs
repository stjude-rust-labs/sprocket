//! Implementation of the `format` subcommand.

use std::fs;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use clap::Parser;
use clap::Subcommand;
use tracing::info;
use tracing::warn;
use wdl::analysis::Document;
use wdl::ast::AstNode;
use wdl::ast::Node;
use wdl::format::Config as FormatConfig;
use wdl::format::Formatter;
use wdl::format::Indent;
use wdl::format::MaxLineLength;
use wdl::format::element::node::AstNodeFormatExt;

use crate::Config;
use crate::analysis::Analysis;
use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::diagnostics::Mode;
use crate::diagnostics::emit_diagnostics;

/// Arguments for the `format` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The report mode for any emitted diagnostics.
    #[arg(short = 'm', long, value_name = "MODE", global = true)]
    pub report_mode: Option<Mode>,

    /// Use tabs for indentation (default is spaces).
    #[arg(short = 't', long, global = true)]
    pub with_tabs: bool,

    /// The number of spaces to use for indentation levels (default is 4).
    #[arg(
        short,
        long,
        value_name = "SIZE",
        conflicts_with = "with_tabs",
        global = true
    )]
    pub indentation_size: Option<usize>,

    /// The maximum line length (default is 90).
    #[arg(long, value_name = "LENGTH", global = true)]
    pub max_line_length: Option<usize>,

    /// The newline style to use 
    #[arg(long, value_name = "STYLE", global = true, value_parser = ["auto", "unix", "windows"])]
    pub newline_style: Option<String>,
    /// Subcommand for the `format` command.
    #[command(subcommand)]
    pub command: FormatSubcommand,
}

/// Vec of Source arguments (may be empty).
#[derive(Parser, Debug, Clone)]
pub struct OptionalSources {
    /// Sources to format.
    sources: Vec<Source>,
}

/// Source argument that is required.
#[derive(Parser, Debug, Clone)]
pub struct RequiredSource {
    /// Source to format.
    source: Source,
}

/// Subcommands for the `format` command.
#[derive(Subcommand, Debug, Clone)]
pub enum FormatSubcommand {
    /// Check if files are formatted correctly and print diff if not.
    Check(OptionalSources),

    /// Format a document and send the result to STDOUT.
    View(RequiredSource),

    /// Reformat all WDL documents via overwriting.
    Overwrite(OptionalSources),
}

/// Formats a document.
fn format_document(
    formatter: &Formatter,
    document: &Document,
    mode: Mode,
    colorize: bool,
) -> Result<(String, String)> {
    let source = document.root().text().to_string();
    let diagnostics = document
        .parse_diagnostics()
        .iter()
        .filter(|d| d.severity().is_error())
        .collect::<Vec<_>>();
    if !diagnostics.is_empty() {
        let path = document.path();
        emit_diagnostics(&path, source.clone(), diagnostics, &[], mode, colorize)?;
        return Err(anyhow!("cannot format a malformed document"));
    }

    let ast = document
        .root()
        .ast()
        .into_v1()
        .expect("only WDL v1.x documents are supported");
    let element = Node::Ast(ast).into_format_element();
    Ok((source, formatter.format(&element)?))
}

/// Runs the `format` command.
pub async fn format(args: Args, config: Config, colorize: bool) -> CommandResult<()> {
    let report_mode = args.report_mode.unwrap_or(config.common.report_mode);
    let fallback_version = config.common.wdl.fallback_version;

    let indent = Indent::try_new(
        args.with_tabs || config.format.with_tabs,
        Some(
            args.indentation_size
                .unwrap_or(config.format.indentation_size),
        ),
    )
    .context("failed to create indentation configuration")?;

    let max_line_length = MaxLineLength::try_new(
        args.max_line_length
            .unwrap_or(config.format.max_line_length),
    )
    .context("failed to create max line length configuration")?;
    let newline_style = match args.newline_style.as_deref() {
    Some("unix") => wdl::format::NewlineStyle::Unix,
    Some("windows") => wdl::format::NewlineStyle::Windows,
    _ => config.format.newline_style,
};

    let format_config = FormatConfig::default()
        .indent(indent)
        .max_line_length(max_line_length)
        .newline_style(newline_style);

    let formatter = Formatter::new(format_config);

    let mut errors = 0;
    match args.command {
        FormatSubcommand::Check(s) => {
            let mut sources = s.sources;
            if sources.is_empty() {
                sources.push(Source::default());
            }

            let results = Analysis::default()
                .extend_sources(sources.clone())
                .fallback_version(fallback_version)
                .run()
                .await
                .map_err(CommandError::from)?;
            let sources = sources.iter().collect::<Vec<_>>();
            let results = results.filter(sources.as_slice()).collect::<Vec<_>>();
            for result in results {
                info!("checking `{}`", result.document().path());

                if let Some(err) = result.error() {
                    errors += 1;
                    warn!("error analyzing `{}`: {}", result.document().path(), err);
                    continue;
                }

                let (source, formatted) =
                    match format_document(&formatter, result.document(), report_mode, colorize) {
                        Ok(r) => r,
                        Err(e) => {
                            errors += 1;
                            warn!(
                                "skipping format check for `{}`: {e}",
                                result.document().path()
                            );
                            continue;
                        }
                    };
                if formatted != source {
                    warn!("difference in `{}`", result.document().path());
                    if colorize {
                        eprint!(
                            "{}",
                            pretty_assertions::StrComparison::new(&source, &formatted)
                        );
                    } else {
                        let diff = similar::TextDiff::from_lines(&source, &formatted);
                        eprint!("{}", diff.unified_diff().header("input", "formatted"));
                    }
                    errors += 1;
                } else {
                    println!("`{}` is formatted correctly", result.document().path())
                }
            }
        }
        FormatSubcommand::View(s) => {
            let source = s.source;
            match &source {
                Source::File(_) | Source::Url(_) => {}
                Source::Directory(p) => {
                    return Err(anyhow!(
                        "the `format view` command does not support formatting directory `{path}`",
                        path = p.display()
                    )
                    .into());
                }
            };

            let results = Analysis::default()
                .add_source(source.clone())
                .fallback_version(fallback_version)
                .run()
                .await
                .map_err(CommandError::from)?;
            let result = results.filter(&[&source]).next().unwrap();

            if let Some(err) = result.error() {
                return Err(anyhow!(
                    "error analyzing `{path}`: {err:#}",
                    path = result.document().path()
                )
                .into());
            }

            let (_source, formatted) =
                format_document(&formatter, result.document(), report_mode, colorize)
                    .with_context(|| {
                        format!(
                            "could not view document `{path}`",
                            path = result.document().path()
                        )
                    })?;
            print!("{}", formatted);
        }
        FormatSubcommand::Overwrite(s) => {
            let mut sources = s.sources;
            if sources.is_empty() {
                sources.push(Source::default());
            }

            let results = Analysis::default()
                .extend_sources(sources.clone())
                .fallback_version(fallback_version)
                .run()
                .await
                .map_err(CommandError::from)?;
            let sources = sources.iter().collect::<Vec<_>>();
            let results = results.filter(sources.as_slice()).collect::<Vec<_>>();
            for result in results {
                info!("formatting `{}`", result.document().path());

                if let Some(err) = result.error() {
                    errors += 1;
                    warn!(
                        "error analyzing `{path}`: {err:#}",
                        path = result.document().path()
                    );
                    continue;
                }

                let (_source, formatted) =
                    match format_document(&formatter, result.document(), report_mode, colorize) {
                        Ok(r) => r,
                        Err(e) => {
                            errors += 1;
                            warn!(
                                "not overwriting document `{path}` due to error: {e:#}",
                                path = result.document().path()
                            );
                            continue;
                        }
                    };

                fs::write(result.document().uri().to_file_path().unwrap(), formatted)
                    .with_context(|| {
                        format!("failed to overwrite `{}`", result.document().path())
                    })?;
            }
        }
    }

    if errors > 0 {
        return Err(anyhow!(
            "failing due to previous {errors} error{s}",
            s = if errors == 1 { "" } else { "s" }
        )
        .into());
    }

    Ok(())
}
