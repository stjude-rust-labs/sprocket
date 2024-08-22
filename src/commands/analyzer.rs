use clap::Parser;
use wdl::lsp::Server;
use wdl::lsp::ServerOptions;

/// Arguments for the `analyzer` subcommand.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct AnalyzerArgs {
    /// Use stdin and stdout for the RPC transport.
    #[clap(long, required = true)]
    stdio: bool,

    /// Whether or not to enable all lint rules.
    #[clap(long)]
    lint: bool,
}

/// Runs the `analyzer` command.
pub async fn analyzer(args: AnalyzerArgs) -> anyhow::Result<()> {
    Server::run(ServerOptions {
        name: Some("Sprocket".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        lint: args.lint,
    })
    .await
}
