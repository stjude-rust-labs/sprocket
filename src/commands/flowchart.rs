//! Implementation of the `dev flowchart` subcommand.

pub mod mermaid;

use clap::Subcommand;

use crate::Config;
use crate::commands::CommandResult;

/// Arguments for the `dev flowchart` subcommand.
#[derive(Subcommand, Debug)]
pub enum Args {
    /// Renders a `WDL` workflow as a `Mermaid` flowchart diagram.
    Mermaid(mermaid::Args),
}

/// Runs the `dev flowchart` subcommand.
pub async fn flowchart(args: Args, config: &Config, colorize: bool) -> CommandResult<()> {
    match args {
        Args::Mermaid(args) => mermaid::mermaid(args, config, colorize).await,
    }
}
