//! The `sprocket dev module` command group.

use clap::Subcommand;

use crate::commands::output::CommandOutput;
use crate::config::Config;

pub mod add;
pub(crate) mod auto_lock;
pub mod clean;
mod display;
pub mod fetch;
pub mod init;
pub mod lock;
mod manifest;
mod mutation;
mod project;
mod relock;
pub mod remove;
mod resolver;
pub mod sign;
mod signer_policy;
pub mod tree;
pub mod trust;
mod trust_store;
pub mod update;
pub mod upgrade;
pub mod verify;

/// Subcommands of `sprocket dev module`.
#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ModuleCommands {
    /// Initialize a new module in the current or given directory.
    Init(init::Args),
    /// Add a dependency to `module.json` and relock.
    Add(add::Args),
    /// Remove a dependency from `module.json` and relock.
    Remove(remove::Args),
    /// Resolve dependencies and write `module-lock.json`.
    Lock(lock::Args),
    /// Update locked dependencies within manifest constraints.
    Update(update::Args),
    /// Raise manifest constraints to the latest versions, then relock.
    Upgrade(upgrade::Args),
    /// Print the resolved dependency tree.
    Tree(tree::TreeArgs),
    /// List dependencies in a flat table.
    List(tree::ListArgs),
    /// Verify module signatures and locked dependencies.
    Verify(verify::Args),
    /// Pre-populate the cache from the lockfile.
    Fetch(fetch::Args),
    /// Manage the module cache.
    #[command(subcommand)]
    Cache(clean::CacheCommands),
    /// Sign the module, writing `module.sig`.
    Sign(sign::Args),
    /// Manage the user trust store.
    #[command(subcommand)]
    Trust(trust::TrustCommands),
}

/// Dispatches a `sprocket dev module` subcommand.
pub async fn run(
    command: ModuleCommands,
    config: Config,
    output: CommandOutput,
) -> crate::commands::CommandResult<()> {
    match command {
        ModuleCommands::Init(args) => init::init(args, output).await,
        ModuleCommands::Add(args) => add::add(args, config, output).await,
        ModuleCommands::Remove(args) => remove::remove(args, config, output).await,
        ModuleCommands::Lock(args) => lock::lock(args, config, output).await,
        ModuleCommands::Update(args) => update::update(args, config, output).await,
        ModuleCommands::Upgrade(args) => upgrade::upgrade(args, config, output).await,
        ModuleCommands::Tree(args) => tree::tree(args, output).await,
        ModuleCommands::List(args) => tree::list(args, output).await,
        ModuleCommands::Verify(args) => verify::verify(args, config, output).await,
        ModuleCommands::Fetch(args) => fetch::fetch(args, config, output).await,
        ModuleCommands::Cache(args) => clean::cache(args, config, output).await,
        ModuleCommands::Sign(args) => sign::sign(args, output).await,
        ModuleCommands::Trust(args) => trust::trust(args, output).await,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_root_is_not_an_internal_reexport_barrel() {
        let source = include_str!("module.rs");
        let pub_use = ["pub", " use "].concat();
        let pub_crate_use = ["pub(crate)", " use "].concat();
        assert!(!source.contains(&pub_use));
        assert!(!source.contains(&pub_crate_use));
    }
}
