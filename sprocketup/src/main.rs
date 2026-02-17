//! Sprocketup - a [Sprocket](https://sprocket.bio) installation manager.

mod components;
mod dirs;
mod downloads;
mod init;
mod manifest;
mod update;

use clap::Parser;
use tracing::level_filters::LevelFilter;

use crate::init::Profile;

/// Base URL for downloading the latest versions of sprocket components.
const SPROCKET_UPDATE_ROOT: &str =
    "https://github.com/Serial-ATA/sprocket/releases/latest/download/";
// TODO: new repo?
// /// Base URL for downloading the latest version of `sprocketup`.
// const SPROCKETUP_UPDATE_ROOT: &str =
//     "https://github.com/Serial-ATA/sprocketup/releases/latest/download/";

/// The `sprocketup` CLI arguments.
#[derive(Debug, Parser)]
#[command(name = "sprocketup")]
struct Cli {
    /// Use verbose output
    #[arg(short, long, conflicts_with = "quiet")]
    verbose: bool,

    /// Disable progress output and general log messages
    #[arg(short, long, conflicts_with = "verbose")]
    quiet: bool,

    /// The subcommand to run.
    #[command(subcommand)]
    subcommand: Subcommand,
}

// TODO: self update? requires a separate repo
/// `sprocketup` subcommands.
#[derive(Debug, clap::Subcommand)]
#[command(name = "sprocketup")]
enum Subcommand {
    /// Update the currently installed `sprocket` components
    #[command(aliases = ["upgrade", "up"])]
    Update,
    /// Initialize a `sprocket` profile with the latest published versions.
    Init {
        /// The profile to install.
        #[arg(long, value_enum, default_value_t)]
        profile: Profile,
    },

    /// Modify the installed components
    Component {
        /// The `component` subcommand.
        #[command(subcommand)]
        subcmd: ComponentSubcommand,
    },
}

/// Subcommands for the `sprocket component` subcommand.
#[derive(Debug, clap::Subcommand)]
#[command(arg_required_else_help = true, subcommand_required = true)]
enum ComponentSubcommand {
    /// List installed and available components
    List {
        /// List only installed components
        #[arg(long)]
        installed: bool,

        /// Force the output to be a single column
        #[arg(long, short)]
        quiet: bool,
    },

    /// Add a component to a Sprocket installation.
    Add {
        /// The list of components to add.
        #[arg(required = true, num_args = 1..)]
        component: Vec<String>,
    },

    /// Remove a component from a Sprocket installation.
    #[command(aliases = ["uninstall", "rm", "delete", "del"])]
    Remove {
        /// The list of components to remove.
        #[arg(required = true, num_args = 1..)]
        component: Vec<String>,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .without_time()
        .init();

    if let Err(e) = real_main(cli).await {
        tracing::error!("{e}");
        std::process::exit(1);
    }
}

/// The actual main logic.
async fn real_main(cli: Cli) -> anyhow::Result<()> {
    // TODO: Create a lock file to block multiple invocations.
    match cli.subcommand {
        Subcommand::Update => update::update().await,
        Subcommand::Init { profile } => init::init(profile).await,
        Subcommand::Component { subcmd } => match subcmd {
            ComponentSubcommand::List { installed, quiet } => {
                components::list_components(installed, quiet)
            }
            ComponentSubcommand::Add { component } => {
                components::add_components(component, None).await
            }
            ComponentSubcommand::Remove { component } => {
                components::remove_components(component).await
            }
        },
    }
}
