//! Implementation of sprocket CLI commands.

pub mod analyzer;
pub mod check;
pub mod doc;
pub mod explain;
pub mod format;
pub mod run;
pub mod validate;

use clap::Subcommand;

/// Developmental and experimental commands.
#[derive(Subcommand, Debug)]
pub enum DevCommands {
    /// Document a workspace.
    Doc(doc::Args),
}
