//! Implementation of the format command.
//! Implementation of the analyzer command.

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
use wdl::ast::Document;
use wdl::ast::Node;
use wdl::format::Formatter;
use wdl::format::config::Builder;
use wdl::format::config::Indent;
use wdl::format::element::node::AstNodeFormatExt;

use super::Mode;
use crate::commands::get_display_config;

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

/// Arguments for the `analyzer` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct FormatArgs {
    /// The path to the source WDL file (`-` for STDIN).
    #[arg(value_name = "PATH")]
    pub path: PathBuf,

    /// Disables color output.
    #[arg(long)]
    pub no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, default_value_t, value_name = "MODE")]
    pub report_mode: Mode,

    /// Use tabs instead of spaces for indentation.
    pub with_tabs: bool,

    /// The number of spaces that represents an indentation level.
    #[arg(value_name = "SIZE", default_value = "4", conflicts_with = "with_tabs")]
    pub indentation_size: usize,
}

/// Runs the `format` command.
pub fn format(args: FormatArgs) -> Result<()> {
    let source = read_source(&args.path)?;

    let (document, diagnostics) = Document::parse(&source);
    if !diagnostics.is_empty() {
        let (config, mut stream) = get_display_config(args.report_mode, args.no_color);
        let file = SimpleFile::new(args.path.to_string_lossy(), source);
        for diagnostic in diagnostics.iter() {
            emit(&mut stream, &config, &file, &diagnostic.to_codespan())
                .context("failed to emit diagnostic")?;
        }

        bail!(
            "aborting due to previous {count} diagnostic{s}",
            count = diagnostics.len(),
            s = if diagnostics.len() == 1 { "" } else { "s" }
        );
    }

    let document = Node::Ast(
        document
            .ast()
            .into_v1()
            .ok_or_else(|| anyhow!("only WDL 1.x documents are currently supported"))?,
    )
    .into_format_element();
    let config = Builder::default()
        .indent(if args.with_tabs {
            Indent::Tabs(NonZeroUsize::new(1).unwrap())
        } else {
            Indent::Spaces(
                NonZeroUsize::new(args.indentation_size)
                    .ok_or_else(|| anyhow!("indentation size must be non-zero"))?,
            )
        })
        .try_build()?;

    let formatter = Formatter::new(config);
    let formatted = formatter.format(&document)?;

    print!("{formatted}");

    Ok(())
}
