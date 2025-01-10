//! Implementation of the format command.

use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::num::NonZeroUsize;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use clap::Parser;
use colored::Colorize;
use pretty_assertions::StrComparison;
use walkdir::WalkDir;
use wdl::ast::Document;
use wdl::ast::Node;
use wdl::format::Config;
use wdl::format::Formatter;
use wdl::format::config::Builder;
use wdl::format::config::Indent;
use wdl::format::element::node::AstNodeFormatExt;

use crate::emit_diagnostics;
use crate::Mode;

/// The maximum acceptable indentation size.
const MAX_INDENT_SIZE: usize = 16;

/// The default number of tabs to use for indentation.
const DEFAULT_TAB_INDENT_SIZE: usize = 1;

/// The default number of spaces to use for indentation.
const DEFAULT_SPACE_IDENT_SIZE: usize = 4;

/// Arguments for the `format` subcommand.
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about,
    after_help = "By default, the `format` command will print a single formatted WDL \
                  document.\n\nUse the `--overwrite` option to replace a WDL document, or a \
                  directory containing WDL documents, with the formatted source."
)]
pub struct FormatArgs {
    /// The path to the WDL document to format (`-` for STDIN); the path may be
    /// a directory when `--overwrite` is specified.
    #[arg(value_name = "PATH")]
    pub path: PathBuf,

    /// Disables color output.
    #[arg(long)]
    pub no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, default_value_t, value_name = "MODE")]
    pub report_mode: Mode,

    /// Use tabs for indentation (default is spaces).
    #[arg(long)]
    pub with_tabs: bool,

    /// The number of characters to use for indentation levels (defaults to 4
    /// for spaces and 1 for tabs).
    #[arg(long, value_name = "SIZE")]
    pub indentation_size: Option<usize>,

    /// Overwrite the WDL documents with the formatted versions
    #[arg(long, conflicts_with = "check")]
    pub overwrite: bool,

    /// Check if files are formatted correctly and print diff if not
    #[arg(long, conflicts_with = "overwrite")]
    pub check: bool,
}

/// Reads source from the given path.
///
/// If the path is simply `-`, the source is read from STDIN.
fn read_source(path: &Path) -> Result<String> {
    if path.as_os_str() == "-" {
        let mut source = String::new();
        std::io::stdin()
            .read_to_string(&mut source)
            .context("failed to read source from STDIN")?;
        Ok(source)
    } else {
        Ok(fs::read_to_string(path).with_context(|| {
            format!("failed to read source file `{path}`", path = path.display())
        })?)
    }
}

/// Formats a document.
///
/// If the document failed to parse, this emits the diagnostics and returns
/// `Ok(count)` of the diagnostics to the caller.
///
/// A return value of `Ok(0)` indicates the document was formatted.
fn format_document(
    config: Config,
    path: &Path,
    overwrite: bool,
    report_mode: Mode,
    no_color: bool,
    check: bool,
) -> Result<usize> {
    if path.to_str() != Some("-") {
        let action = if check { "checking" } else { "formatting" };
        println!(
            "{action_colored} `{path}`",
            action_colored = if no_color {
                action.normal()
            } else {
                action.green()
            },
            path = path.display()
        );
    }

    let source = read_source(path)?;
    let (document, diagnostics) = Document::parse(&source);
    if !diagnostics.is_empty() {
        emit_diagnostics(
            &diagnostics,
            path.to_str().unwrap_or(""),
            &source,
            report_mode,
            no_color,
        );

        return Ok(diagnostics.len());
    }

    let document = Node::Ast(
        document
            .ast()
            .into_v1()
            .ok_or_else(|| anyhow!("only WDL 1.x documents are currently supported"))?,
    )
    .into_format_element();

    let formatter = Formatter::new(config);
    let formatted = formatter.format(&document)?;

    if check {
        if formatted != source {
            print!("{}", StrComparison::new(&source, &formatted));
            return Ok(1);
        }
        println!("`{path}` is formatted correctly", path = path.display());
        return Ok(0);
    }

    if overwrite {
        fs::write(path, formatted)
            .with_context(|| format!("failed to write `{path}`", path = path.display()))?;
    } else {
        print!("{formatted}");
    }

    Ok(0)
}

/// Runs the `format` command.
pub fn format(args: FormatArgs) -> Result<()> {
    let indentation_size = NonZeroUsize::new(args.indentation_size.unwrap_or(if args.with_tabs {
        DEFAULT_TAB_INDENT_SIZE
    } else {
        DEFAULT_SPACE_IDENT_SIZE
    }))
    .ok_or_else(|| anyhow!("indentation size must be a value greater than zero"))?;
    if indentation_size.get() > MAX_INDENT_SIZE {
        bail!("indentation size cannot be greater than {MAX_INDENT_SIZE}");
    }

    let config = Builder::default()
        .indent(if args.with_tabs {
            Indent::Tabs(indentation_size)
        } else {
            Indent::Spaces(indentation_size)
        })
        .try_build()?;

    let mut diagnostics = 0;
    if args.path.to_str() != Some("-") && args.path.is_dir() {
        if !args.overwrite && !args.check {
            bail!("formatting a directory requires the `--overwrite` or `--check` option");
        }

        for entry in WalkDir::new(&args.path) {
            let entry = entry.with_context(|| {
                format!(
                    "failed to walk directory `{path}`",
                    path = args.path.display()
                )
            })?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(OsStr::to_str) != Some("wdl") {
                continue;
            }

            diagnostics += format_document(
                config,
                path,
                args.overwrite,
                args.report_mode,
                args.no_color,
                args.check,
            )?;
        }
    } else {
        diagnostics += format_document(
            config,
            &args.path,
            args.overwrite,
            args.report_mode,
            args.no_color,
            args.check,
        )?;
    }

    if diagnostics > 0 {
        bail!(
            "aborting due to previous {diagnostics} diagnostic{s}",
            s = if diagnostics == 1 { "" } else { "s" }
        );
    }

    Ok(())
}
