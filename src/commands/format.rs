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
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::emit;
use colored::Colorize;
use walkdir::WalkDir;
use wdl::ast::Document;
use wdl::ast::Node;
use wdl::format::Config;
use wdl::format::Formatter;
use wdl::format::config::Builder;
use wdl::format::config::Indent;
use wdl::format::element::node::AstNodeFormatExt;

use super::Mode;
use crate::commands::get_display_config;

/// The maximum acceptable indentation size.
const MAX_INDENT_SIZE: usize = 16;

/// The default number of tabs to use for indentation.
const DEFAULT_TAB_INDENT_SIZE: usize = 1;

/// The default number of spaces to use for indentation.
const DEFAULT_SPACE_IDENT_SIZE: usize = 4;

/// Arguments for the `analyzer` subcommand.
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
    #[arg(long)]
    pub overwrite: bool,
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
) -> Result<usize> {
    if path.to_str() != Some("-") {
        println!(
            "{formatting} `{path}`",
            formatting = if no_color {
                "formatting".normal()
            } else {
                "formatting".green()
            },
            path = path.display()
        );
    }

    let source = read_source(path)?;
    let (document, diagnostics) = Document::parse(&source);
    if !diagnostics.is_empty() {
        let (config, mut stream) = get_display_config(report_mode, no_color);
        let file = SimpleFile::new(path.to_string_lossy(), source);
        for diagnostic in diagnostics.iter() {
            emit(&mut stream, &config, &file, &diagnostic.to_codespan())
                .context("failed to emit diagnostic")?;
        }

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
        if !args.overwrite {
            bail!("formatting a directory requires the `--overwrite` option");
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
            )?;
        }
    } else {
        diagnostics += format_document(
            config,
            &args.path,
            args.overwrite,
            args.report_mode,
            args.no_color,
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
