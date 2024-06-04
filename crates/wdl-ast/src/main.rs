//! A command-line tool for exploring an abstract syntax tree constructed from a
//! Workflow Description Language (WDL) grammar.
//!
//! **Note:** this tool is intended to be used as a utility to test and develop
//! the [`wdl-ast`](https://crates.io/crates/wdl-ast) crate. It is not intended
//! to be used by a general audience.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

use clap::Parser;
use clap::Subcommand;
use log::LevelFilter;

mod commands;

use crate::commands::parse;

/// Subcommands for the `wdl-grammar` command-line tool.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Parses and displays an abstract syntax tree.
    Parse(parse::Args),
}

/// Parse and testing Workflow Description Language (WDL) grammar.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
struct Args {
    /// The subcommand to execute.
    #[command(subcommand)]
    command: Command,

    /// Detailed information, including debug information, is logged in the
    /// console.
    #[arg(short, long, global = true)]
    debug: bool,

    /// Enables logging for all modules (not just `wdl-grammar`).
    #[arg(short, long, global = true)]
    log_all_modules: bool,

    /// Only errors are logged to the console.
    #[arg(short, long, global = true)]
    quiet: bool,

    /// All available information, including trace information, is logged in
    /// the console.
    #[arg(short, long, global = true)]
    trace: bool,

    /// Additional information is logged in the console.
    #[arg(short, long, global = true)]
    verbose: bool,
}

/// The inner function for the binary.
async fn inner() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let level = if args.trace {
        LevelFilter::max()
    } else if args.debug {
        LevelFilter::Debug
    } else if args.verbose {
        LevelFilter::Info
    } else if args.quiet {
        LevelFilter::Error
    } else {
        LevelFilter::Warn
    };

    let module = match args.log_all_modules {
        true => None,
        false => Some("wdl_ast"),
    };

    env_logger::builder().filter(module, level).init();

    match args.command {
        Command::Parse(args) => parse::parse(args)?,
    };

    Ok(())
}

#[tokio::main]
async fn main() {
    match inner().await {
        Ok(_) => {}
        Err(err) => eprintln!("error: {}", err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_arguments() {
        use clap::CommandFactory;
        Args::command().debug_assert()
    }
}
