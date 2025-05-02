//! Implementation of the `completions` subcommand.

use std::io;

use anyhow::Result;
use clap::Command;
use clap::Parser;
use clap_complete::Shell;
use clap_complete::generate;

/// Arguments for the `completions` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// The shell to generate completions for
    #[arg(value_enum)]
    shell: Shell,
}

/// The main function for the `completions` subcommand.
pub async fn completions(args: Args, cmd: &mut Command) -> Result<()> {
    eprintln!("generating completion file for {:?}", args.shell);
    generate(
        args.shell,
        cmd,
        cmd.get_name().to_string(),
        &mut io::stdout(),
    );
    Ok(())
}
