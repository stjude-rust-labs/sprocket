//! A command-line tool for parsing Workflow Description Language (WDL)
//! documents.
//!
//! **Note:** this tool is intended to be used as a utility to test and develop
//! the [`wdl-grammar`](https://crates.io/crates/wdl-grammar) and
//! [`wdl-ast`](https://crates.io/crates/wdl-ast) crates. It is not intended to
//! be used by a general audience.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

use clap::Parser as _;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use wdl_gauntlet as gauntlet;

/// The inner function for `wdl-gauntlet`.
async fn inner() -> Result<(), Box<dyn std::error::Error>> {
    let args = gauntlet::Args::parse();

    let level = if args.trace {
        LevelFilter::TRACE
    } else if args.debug {
        LevelFilter::DEBUG
    } else if args.verbose {
        LevelFilter::INFO
    } else if args.quiet {
        LevelFilter::ERROR
    } else {
        LevelFilter::WARN
    };

    let filter = match args.log_all_modules {
        true => EnvFilter::default().add_directive(level.into()),
        false => EnvFilter::default().add_directive(format!("wdl_gauntlet={}", level).parse()?),
    };

    tracing_subscriber::fmt().with_env_filter(filter).init();

    gauntlet::gauntlet(args).await?;

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
        gauntlet::Args::command().debug_assert()
    }
}
