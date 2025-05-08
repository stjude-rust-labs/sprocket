//! Implementation of the language server protocol (LSP) subcommand.

use clap::Parser;
use wdl::lsp::Server;
use wdl::lsp::ServerOptions;

/// Arguments for the `analyzer` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Use stdin and stdout for the RPC transport.
    #[clap(long, required = true)]
    pub stdio: bool,

    /// Whether or not to enable all lint rules.
    #[clap(long)]
    pub lint: bool,
}

/// Runs the `analyzer` command.
pub async fn analyzer(args: Args) -> anyhow::Result<()> {
    Server::run(ServerOptions {
        name: Some("Sprocket".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        lint: args.lint,
    })
    .await
}
