//! The `sprocket dev module` command group.

use clap::Subcommand;

use crate::commands::output::CommandOutput;
use crate::config::Config;

pub mod add;
pub mod clean;
mod display;
pub mod fetch;
pub mod init;
pub mod lock;
mod manifest;
mod mutation;
mod project;
pub mod remove;
mod resolution;
pub mod sign;
pub mod tree;
pub mod trust;
mod trust_policy;
pub mod update;
pub mod upgrade;
pub mod verify;

pub(crate) use display::update_details;
pub(crate) use manifest::align_temp_permissions;
pub(crate) use manifest::parse_manifest_value;
pub use manifest::read_manifest_value;
pub use manifest::remove_dependency;
pub use manifest::set_dependency;
pub use manifest::write_lockfile;
pub use manifest::write_manifest_value;
pub(crate) use mutation::LockedProject;
pub use project::Locator;
pub use project::Project;
pub use project::discover;
pub use project::load_lockfile;
pub(crate) use project::require_lockfile;
pub(crate) use project::trace_project;
pub use resolution::TrustModeArg;
pub use resolution::build_resolver;
pub(crate) use resolution::enforce_lockfile_signer_policy;
pub(crate) use resolution::ensure_lockfile_current;
pub(crate) use resolution::resolve_relock_for_manifest;
pub(crate) use resolution::resolve_relock_plan;
pub(crate) use resolution::resolve_relock_with_signer_mode;
pub(crate) use resolution::signer_change_mode;
pub(crate) use trust_policy::accept_lockfile_signers;
pub(crate) use trust_policy::load_trust_store;
pub(crate) use trust_policy::render_signer;
pub(crate) use trust_policy::save_trust_store;

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
