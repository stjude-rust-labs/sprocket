//! `sprocket dev module cache`.

use bytesize::ByteSize;
use clap::Parser;
use clap::Subcommand;
use wdl_modules::Lockfile;
use wdl_modules::module::Module;

use crate::commands::CommandResult;
use crate::commands::module::Locator;
use crate::commands::module::ModuleAction;
use crate::commands::module::ModuleOutput;
use crate::commands::module::build_resolver;
use crate::commands::module::discover;
use crate::commands::module::require_lockfile;
use crate::commands::module::trace_project;
use crate::commands::printer::Printer;
use crate::config::Config;

/// Subcommands of `sprocket dev module cache`.
#[derive(Subcommand, Debug)]
pub enum CacheCommands {
    /// Remove cached modules.
    Clean(Args),
}

/// Arguments to `sprocket dev module cache clean`.
#[derive(Parser, Debug)]
pub struct Args {
    /// Remove every module from the cache instead of this module's lock tree.
    #[arg(long)]
    pub all: bool,

    /// Shared module locator.
    #[command(flatten)]
    pub locator: Locator,
}

/// Runs `sprocket dev module cache`.
pub async fn cache(command: CacheCommands, config: Config, printer: Printer) -> CommandResult<()> {
    match command {
        CacheCommands::Clean(args) => clean(args, config, printer).await,
    }
}

/// Runs `sprocket dev module cache clean`.
pub async fn clean(args: Args, config: Config, printer: Printer) -> CommandResult<()> {
    tracing::trace!(all = args.all, "starting `sprocket dev module cache clean`");
    let cache_root = config
        .modules
        .cache_path
        .clone()
        .unwrap_or_else(crate::analysis::default_cache_root);

    tracing::debug!(
        cache = %cache_root.display(),
        all = args.all,
        "resolved module cache path"
    );

    if args.all {
        let resolver = build_resolver(&config, Lockfile::default())?;
        let stats = resolver.clean_all_cache().map_err(anyhow::Error::from)?;
        tracing::debug!(
            cache = %cache_root.display(),
            modules = stats.modules,
            bytes = stats.bytes,
            "removed entire module cache"
        );
        print_removed_summary(stats.modules, stats.bytes, printer);
        return Ok(());
    }

    let project = discover(&args.locator)?;
    trace_project("module cache clean", &project);
    let lock = require_lockfile(&project)?;
    let module = Module::new(project.manifest.clone(), project.root.clone());
    let resolver = build_resolver(&config, lock)?;
    let stats = resolver
        .clean_locked_cache(&module)
        .map_err(anyhow::Error::from)?;

    tracing::debug!(
        cache = %cache_root.display(),
        modules = stats.modules,
        bytes = stats.bytes,
        "removed locked module cache leaves"
    );
    print_removed_summary(stats.modules, stats.bytes, printer);
    Ok(())
}

/// Prints the cache-clean summary line.
fn print_removed_summary(modules: usize, bytes: u64, printer: Printer) {
    let output = ModuleOutput::new(printer);
    output.completed(
        ModuleAction::Clean,
        crate::commands::module::count_noun(modules, "cached module", "cached modules"),
    );
    output.detail("Reclaimed", ByteSize::b(bytes).display().iec());
}
