//! Implementation of the `format` subcommand.

use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use clap::Parser;
use colored::Colorize;
use pretty_assertions::StrComparison;
use serde::Deserialize;
use serde::Serialize;
use walkdir::WalkDir;
use wdl::ast::Document;
use wdl::ast::Node;
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
    after_help = "Use the `--overwrite` option to replace a WDL document or a directory \
                  containing WDL documents with the formatted source.\nUse the `--check` option \
                  to verify that a document or a directory containing WDL documents is already \
                  formatted and print the diff if not."
)]
pub struct Args {
    /// The path to the WDL document or a directory containing WDL documents to
    /// format or check (`-` for STDIN).
    #[arg(value_name = "PATH or DIR")]
    pub path: PathBuf,

    /// Disables color output.
    #[arg(long)]
    pub no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,

    /// Use tabs for indentation (default is spaces).
    #[arg(long)]
    pub with_tabs: bool,

    /// The number of spaces to use for indentation levels (default is 4).
    #[arg(long, value_name = "SIZE", conflicts_with = "with_tabs")]
    pub indentation_size: Option<usize>,

    /// The maximum line length (default is 90).
    #[arg(long, value_name = "LENGTH")]
    pub max_line_length: Option<usize>,

    /// Argument group defining the mode of behavior.
    #[command(flatten)]
    pub mode: ModeGroup,
}

impl Args {
    /// Applies the configuration to the command arguments.
    pub fn apply(mut self, config: crate::config::Config) -> Self {
        self.no_color = self.no_color || !config.common.color;
        self.report_mode = match self.report_mode {
            Some(mode) => Some(mode),
            None => Some(config.common.report_mode),
        };
        self.with_tabs = self.with_tabs || config.format.with_tabs;
        self.indentation_size = match self.indentation_size {
            Some(size) => Some(size),
            None => Some(config.format.indentation_size),
        };
        self.max_line_length = match self.max_line_length {
            Some(length) => Some(length),
            None => Some(config.format.max_line_length),
        };
        self
    }
}

/// Argument group defining the mode of behavior
#[derive(Parser, Debug, Deserialize, Serialize)]
#[group(required = true, multiple = false)]
pub struct ModeGroup {
    /// Overwrite the WDL documents with the formatted versions.
    #[arg(long, conflicts_with = "check")]
    pub overwrite: bool,

    /// Check if files are formatted correctly and print diff if not.
    #[arg(long)]
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
    if path.to_str() != Some("-") {
        let action = if check_only { "checking" } else { "formatting" };
        println!(
            "{action_colored} `{path}`",
            action_colored = if no_color {
                action.normal()
            } else {
                action.green()
            },
            path = path.display()
        );
    } else if !check_only {
        bail!("cannot overwrite STDIN");
    }

    let source = read_source(path)?;
    let (document, diagnostics) = Document::parse(&source);
    if !diagnostics.is_empty() {
        emit_diagnostics(
            path.as_os_str().to_str().expect("path is not UTF-8"),
            source,
            &diagnostics,
            &[],
            report_mode,
            no_color,
        )
        .context("failed to emit diagnostics")?;

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

    if check_only {
        if formatted != source {
            print!("{}", StrComparison::new(&source, &formatted));
            return Ok(1);
        }
        println!("`{path}` is formatted correctly", path = path.display());
        return Ok(0);
    }

    // Write file because check is not true
    fs::write(path, formatted)
        .with_context(|| format!("failed to write `{path}`", path = path.display()))?;

    Ok(0)
}

/// Runs the `format` command.
pub fn format(args: Args) -> Result<()> {
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
    if args.path.to_str() != Some("-") && args.path.is_dir() {
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
                args.report_mode.unwrap_or_default(),
                args.no_color,
                args.mode.check,
            )?;
        }
    } else {
        diagnostics += format_document(
            config,
            &args.path,
            args.report_mode.unwrap_or_default(),
            args.no_color,
            args.mode.check,
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
