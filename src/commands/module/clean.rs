//! `sprocket module cache`.

use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use bytesize::ByteSize;
use clap::Parser;
use clap::Subcommand;
use walkdir::WalkDir;
use wdl_modules::module::Module;

use crate::commands::CommandResult;
use crate::commands::module::ActionColor;
use crate::commands::module::Locator;
use crate::commands::module::build_resolver;
use crate::commands::module::discover;
use crate::commands::module::load_lockfile;
use crate::commands::module::print_action;
use crate::commands::module::trace_project;
use crate::config::Config;

/// Subcommands of `sprocket module cache`.
#[derive(Subcommand, Debug)]
pub enum CacheCommands {
    /// Remove cached modules.
    Clean(Args),
}

/// Arguments to `sprocket module cache clean`.
#[derive(Parser, Debug)]
pub struct Args {
    /// Remove every module from the cache instead of this module's lock tree.
    #[arg(long)]
    pub all: bool,

    /// Shared module locator.
    #[command(flatten)]
    pub locator: Locator,
}

/// Runs `sprocket module cache`.
pub async fn cache(command: CacheCommands, config: Config, colorize: bool) -> CommandResult<()> {
    match command {
        CacheCommands::Clean(args) => clean(args, config, colorize).await,
    }
}

/// Runs `sprocket module cache clean`.
pub async fn clean(args: Args, config: Config, colorize: bool) -> CommandResult<()> {
    tracing::trace!(all = args.all, "starting `sprocket module cache clean`");
    let project = discover(&args.locator)?;
    trace_project("module cache clean", &project);
    let cache_root = config
        .modules
        .cache_path
        .clone()
        .unwrap_or_else(|| crate::analysis::default_cache_root(project.manifest_path.parent()));

    tracing::debug!(
        cache = %cache_root.display(),
        all = args.all,
        "resolved module cache path"
    );

    if args.all {
        let modules = count_cache_leaves(&cache_root)?;
        let bytes = path_size(&cache_root)?;
        remove_cache_dir(&cache_root)?;
        tracing::debug!(
            cache = %cache_root.display(),
            modules,
            bytes,
            "removed entire module cache"
        );
        print_removed_summary(modules, bytes, colorize);
        return Ok(());
    }

    let lock = load_lockfile(&project)?
        .ok_or_else(|| anyhow::anyhow!("no `module-lock.json`; run `sprocket module lock`"))?;
    let module = Module::new(project.manifest.clone(), project.root.clone());
    let resolver = build_resolver(&config, &project, lock)?;
    let leaves = resolver
        .locked_cache_leaves(&module)
        .map_err(anyhow::Error::from)?;

    let mut modules = 0usize;
    let mut bytes = 0u64;
    for leaf in leaves {
        if !leaf.exists() {
            continue;
        }
        modules += 1;
        bytes = bytes.saturating_add(path_size(&leaf)?);
        remove_cache_dir(&leaf)?;
        prune_empty_parents(leaf.parent(), &cache_root)?;
    }

    tracing::debug!(
        cache = %cache_root.display(),
        modules,
        bytes,
        "removed locked module cache leaves"
    );
    print_removed_summary(modules, bytes, colorize);
    Ok(())
}

fn print_removed_summary(modules: usize, bytes: u64, colorize: bool) {
    print_action(
        "Removed",
        format!(
            "{} cached {} ({:.1} GiB)",
            modules,
            if modules == 1 { "module" } else { "modules" },
            (ByteSize::b(bytes).as_gib() * 10.0).ceil() / 10.0
        ),
        colorize,
        ActionColor::Green,
    );
}

fn count_cache_leaves(path: &Path) -> anyhow::Result<usize> {
    if !path.exists() {
        return Ok(0);
    }

    let mut count = 0usize;
    for entry in WalkDir::new(path) {
        let entry = entry.with_context(|| format!("walking `{}`", path.display()))?;
        if entry.file_type().is_dir() && is_commit_dir(entry.path()) {
            count += 1;
        }
    }
    Ok(count)
}

fn path_size(path: &Path) -> anyhow::Result<u64> {
    if !path.exists() {
        return Ok(0);
    }

    let mut bytes = 0u64;
    for entry in WalkDir::new(path) {
        let entry = entry.with_context(|| format!("walking `{}`", path.display()))?;
        if entry.file_type().is_file() {
            bytes = bytes.saturating_add(
                entry
                    .metadata()
                    .with_context(|| format!("reading metadata for `{}`", entry.path().display()))?
                    .len(),
            );
        }
    }
    Ok(bytes)
}

fn is_commit_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name.len() == 40 && name.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn remove_cache_dir(path: &Path) -> anyhow::Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            tracing::trace!(cache = %path.display(), "module cache was already absent");
            Ok(())
        }
        Err(error) => Err(error).with_context(|| format!("removing `{}`", path.display())),
    }
}

fn prune_empty_parents(start: Option<&Path>, stop: &Path) -> anyhow::Result<()> {
    let Some(mut current) = start.map(PathBuf::from) else {
        return Ok(());
    };

    while current.starts_with(stop) && current != stop {
        match std::fs::remove_dir(&current) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) if error.kind() == ErrorKind::DirectoryNotEmpty => break,
            Err(error) => {
                return Err(error).with_context(|| format!("removing `{}`", current.display()));
            }
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }
    Ok(())
}
