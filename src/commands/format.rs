//! Implementation of the `format` subcommand.

use std::fs;
use std::io::IsTerminal;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use clap::Parser;
use clap::Subcommand;
use tracing::info;
use tracing::warn;
use wdl::analysis::Document;
use wdl::ast::AstNode;
use wdl::ast::Node;
use wdl::cli::Analysis;
use wdl::cli::analysis::Source;
use wdl::format::Formatter;
use wdl::format::config::Builder;
use wdl::format::config::Indent;
use wdl::format::config::MaxLineLength;
use wdl::format::element::node::AstNodeFormatExt;

use crate::IGNORE_FILENAME;
use crate::Mode;
use crate::emit_diagnostics;

/// Arguments for the `format` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Disables color output.
    #[arg(long, global = true)]
    pub no_color: bool,

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

    /// Subcommand for the `format` command.
    #[command(subcommand)]
    pub command: FormatSubcommand,
}

impl Args {
    /// Applies the configuration to the command arguments.
    pub fn apply(mut self, config: crate::config::Config) -> Self {
        self.no_color = self.no_color || !config.common.color;
        if self.report_mode.is_none() {
            self.report_mode = Some(config.common.report_mode);
        }
        self.with_tabs = self.with_tabs || config.format.with_tabs;
        if self.indentation_size.is_none() {
            self.indentation_size = Some(config.format.indentation_size);
        }
        if self.max_line_length.is_none() {
            self.max_line_length = Some(config.format.max_line_length);
        }
        self
    }
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
    no_color: bool,
) -> Result<(String, String)> {
    let source = document.root().text().to_string();
    let diagnostics = document
        .parse_diagnostics()
        .iter()
        .filter(|d| d.severity().is_error())
        .collect::<Vec<_>>();
    if !diagnostics.is_empty() {
        let path = document.path();
        emit_diagnostics(&path, source.clone(), diagnostics, &[], mode, no_color)?;
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
pub async fn format(args: Args) -> Result<()> {
    let indent = Indent::try_new(args.with_tabs, args.indentation_size)
        .context("failed to create indentation configuration")?;

    let max_line_length = match args.max_line_length {
        Some(length) => MaxLineLength::try_new(length)
            .context("failed to create max line length configuration")?,
        None => MaxLineLength::default(),
    };

    let config = Builder::default()
        .indent(indent)
        .max_line_length(max_line_length)
        .build();
    let formatter = Formatter::new(config);

    let mut errors = 0;
    match args.command {
        FormatSubcommand::Check(s) => {
            let mut sources = s.sources;
            if sources.is_empty() {
                sources.push(Source::default());
            }

            let results = match Analysis::default()
                .extend_sources(sources.clone())
                .ignore_filename(Some(IGNORE_FILENAME.to_string()))
                .run()
                .await
            {
                Ok(results) => results,
                Err(errors) => {
                    // SAFETY: this is a non-empty, so it must always have a first
                    // element.
                    bail!(errors.into_iter().next().unwrap())
                }
            };
            let sources = sources.iter().collect::<Vec<_>>();
            let results = results.filter(sources.as_slice()).collect::<Vec<_>>();
            for result in results {
                info!("checking `{}`", result.document().path());

                if let Some(err) = result.error() {
                    errors += 1;
                    warn!("error analyzing `{}`: {}", result.document().path(), err);
                    continue;
                }

                let (source, formatted) = match format_document(
                    &formatter,
                    result.document(),
                    args.report_mode.unwrap_or_default(),
                    args.no_color,
                ) {
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
                    if !args.no_color && std::io::stderr().is_terminal() {
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
                Source::File(_) | Source::Remote(_) => {}
                Source::Directory(p) => {
                    bail!(
                        "the `format view` command does not support formatting directory `{path}`",
                        path = p.display()
                    );
                }
            };

            let results = match Analysis::default().add_source(source.clone()).run().await {
                Ok(results) => results,
                Err(errors) => {
                    // SAFETY: this is a non-empty, so it must always have a first
                    // element.
                    bail!(errors.into_iter().next().unwrap())
                }
            };
            let result = results.filter(&[&source]).next().unwrap();

            if let Some(err) = result.error() {
                bail!(
                    "error analyzing `{path}`: {err:#}",
                    path = result.document().path()
                );
            }

            let (_source, formatted) = format_document(
                &formatter,
                result.document(),
                args.report_mode.unwrap_or_default(),
                args.no_color,
            )
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

            let results = match Analysis::default()
                .extend_sources(sources.clone())
                .ignore_filename(Some(IGNORE_FILENAME.to_string()))
                .run()
                .await
            {
                Ok(results) => results,
                Err(errors) => {
                    // SAFETY: this is a non-empty, so it must always have a first
                    // element.
                    bail!(errors.into_iter().next().unwrap())
                }
            };
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

                let (_source, formatted) = match format_document(
                    &formatter,
                    result.document(),
                    args.report_mode.unwrap_or_default(),
                    args.no_color,
                ) {
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
        bail!(
            "failing due to previous {errors} error{s}",
            s = if errors == 1 { "" } else { "s" }
        );
    }

    Ok(())
}
