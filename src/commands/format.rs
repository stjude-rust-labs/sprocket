//! Implementation of the `format` subcommand.

use std::ffi::OsStr;
use std::fs;
use std::io::IsTerminal;
use std::io::Read;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use clap::Parser;
use clap::Subcommand;
use walkdir::WalkDir;
use wdl::ast::Document;
use wdl::ast::Node;
use wdl::cli::analysis::Source;
use wdl::format::Config;
use wdl::format::Formatter;
use wdl::format::config::Builder;
use wdl::format::config::Indent;
use wdl::format::config::MaxLineLength;
use wdl::format::element::node::AstNodeFormatExt;

use crate::Mode;
use crate::emit_diagnostics;

/// Arguments for the `format` subcommand.
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about,
)]
pub struct Args {
    /// Disables color output.
    #[arg(long)]
    pub no_color: bool,

    /// The report mode for any emitted diagnostics.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,

    /// Use tabs for indentation (default is spaces).
    #[arg(long, global = true)]
    pub with_tabs: bool,

    /// The number of spaces to use for indentation levels (default is 4).
    #[arg(long, value_name = "SIZE", conflicts_with = "with_tabs", global = true)]
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

/// Source argument for all `format` subcommands.
#[derive(Parser, Debug, Clone)]
pub struct SourceArg {
    source: Option<Source>,
}

/// Subcommands for the `format` command.
#[derive(Subcommand, Debug, Clone)]
pub enum FormatSubcommand {
    /// Check if files are formatted correctly and print diff if not.
    Check(SourceArg),

    /// Format a document and send the result to STDOUT.
    View(SourceArg),

    /// Reformat all WDL documents via overwriting.
    Overwrite(SourceArg),
}

/// Formats a document.
///
/// If `check_only` is true, checks if the document is formatted correctly and
/// prints the diff if not then exits. Else will format and overwrite the
/// document.
///
/// If the document failed to parse, this emits the diagnostics and returns
/// `Ok(count)` of the diagnostics to the caller.
///
/// A return value of `Ok(0)` indicates the document was formatted.
fn format_document(
    config: Config,
    path: &Path,
    report_mode: Mode,
    no_color: bool,
    check_only: bool,
) -> Result<usize> {
    // let source = read_source(path)?;
    // let (document, diagnostics) = Document::parse(&source);
    // if !diagnostics.is_empty() {
    //     emit_diagnostics(
    //         path.as_os_str().to_str().expect("path is not UTF-8"),
    //         source,
    //         &diagnostics,
    //         &[],
    //         report_mode,
    //         no_color,
    //     )
    //     .context("failed to emit diagnostics")?;

    //     return Ok(diagnostics.len());
    // }

    // let document = Node::Ast(
    //     document
    //         .ast()
    //         .into_v1()
    //         .ok_or_else(|| anyhow!("only WDL 1.x documents are currently supported"))?,
    // )
    // .into_format_element();

    // let formatter = Formatter::new(config);
    // let formatted = formatter.format(&document)?;

    // if check_only {
    //     if formatted != source {
    //         if !no_color && std::io::stderr().is_terminal() {
    //             eprint!(
    //                 "{}",
    //                 pretty_assertions::StrComparison::new(&source, &formatted)
    //             );
    //         } else {
    //             let diff = similar::TextDiff::from_lines(&source, &formatted);
    //             eprint!("{}", diff.unified_diff());
    //         }
    //         return Ok(1);
    //     }
    //     println!("`{path}` is formatted correctly", path = path.display());
    //     return Ok(0);
    // }

    // // Write file because check is not true
    // fs::write(path, formatted)
    //     .with_context(|| format!("failed to write `{path}`", path = path.display()))?;

    Ok(0)
}

/// Runs the `format` command.
pub fn format(args: Args) -> Result<()> {
    let source = match args.command {
        FormatSubcommand::Check(s) => s.source,
        FormatSubcommand::Overwrite(s) => s.source,
        FormatSubcommand::View(s) => s.source,
    };
    let source = source.unwrap_or_default();
    let indent = match Indent::try_new(args.with_tabs, args.indentation_size) {
        Ok(indent) => indent,
        Err(e) => bail!("failed to create indentation configuration: {}", e),
    };

    let max_line_length = match args.max_line_length {
        Some(length) => match MaxLineLength::try_new(length) {
            Ok(max_line_length) => max_line_length,
            Err(e) => bail!("failed to create max line length configuration: {}", e),
        },
        None => MaxLineLength::default(),
    };

    let config = Builder::default()
        .indent(indent)
        .max_line_length(max_line_length)
        .build();

    let mut diagnostics = 0;
    // if let Source::Directory(path) = source {
    //     for entry in WalkDir::new(&path) {
    //         let entry = entry.with_context(|| {
    //             format!("failed to walk directory `{path}`", path = path.display())
    //         })?;
    //         let path = entry.path();
    //         if !path.is_file() || path.extension().and_then(OsStr::to_str) != Some("wdl") {
    //             continue;
    //         }

    //         // diagnostics += format_document(
    //         //     config,
    //         //     path,
    //         //     args.report_mode.unwrap_or_default(),
    //         //     args.no_color,
    //         //     args.mode.check,
    //         // )?;
    //     }
    // } else if let Source::File(path) = source {
    //     // diagnostics += format_document(
    //     //     config,
    //     //     &path.to_file_path().expect("should be local file path"),
    //     //     args.report_mode.unwrap_or_default(),
    //     //     args.no_color,
    //     //     args.mode.check,
    //     // )?;
    // } else {
    //     unreachable!()
    // }

    if diagnostics > 0 {
        bail!(
            "aborting due to previous {diagnostics} diagnostic{s}",
            s = if diagnostics == 1 { "" } else { "s" }
        );
    }

    Ok(())
}
