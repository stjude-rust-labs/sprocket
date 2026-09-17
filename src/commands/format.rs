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
use wdl::diagnostics::Mode;
use wdl::diagnostics::emit_diagnostics;
use wdl::format::Formatter;
use wdl::format::element::node::AstNodeFormatExt;

use crate::Config;
use crate::analysis::Analysis;
use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;

/// Arguments for the `format` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
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
        emit_diagnostics(&path, &source, diagnostics, mode, colorize)?;
        return Err(anyhow!("cannot format a malformed document"));
    }

    let ast = document
        .root()
        .ast_with_version_fallback(document.config().fallback_version())
        .into_v1()
        .expect("only WDL v1.x documents are supported");
    let element = Node::Ast(ast).into_format_element();
    Ok((source, formatter.format(&element)?))
}

/// Runs the `format` command.
pub async fn format(args: Args, config: Config, colorize: bool) -> CommandResult<()> {
    let report_mode = config.common.report_mode;
    let fallback_version = config.common.wdl.fallback_version.into();
    let feature_flags = config.common.wdl.feature_flags;
    let modules_config = config.modules.clone();
    let ignore_filename = config.common.ignore_filename();

    let formatter = Formatter::new(config.format);

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
                .modules_config(modules_config.clone())
                .feature_flags(feature_flags)
                .ignore_filename(ignore_filename.clone())
                .run(report_mode, colorize)
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
                    let newline_only = {
                        let formatted_lines = formatted.lines();
                        let source_lines = source.lines();

                        formatted_lines.zip(source_lines).all(|(f, s)| f == s)
                    };
                    if newline_only {
                        eprintln!("incorrect newline style");
                    } else if colorize {
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
                .modules_config(modules_config.clone())
                .feature_flags(feature_flags)
                .ignore_filename(ignore_filename.clone())
                .run(report_mode, colorize)
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
                .modules_config(modules_config.clone())
                .feature_flags(feature_flags)
                .ignore_filename(ignore_filename.clone())
                .run(report_mode, colorize)
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
