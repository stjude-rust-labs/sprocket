//! Module project discovery and lockfile loading.

use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use clap::Args as ClapArgs;
use wdl_modules::Lockfile;
use wdl_modules::project::ModuleProject;

/// Parsed module project context shared by porcelain subcommands.
pub(super) type Project = ModuleProject;

/// Locates the governing `module.json`.
#[derive(ClapArgs, Debug, Clone)]
pub(super) struct Locator {
    /// Path to the `module.json` or its directory. Defaults to an upward
    /// search from the current directory.
    #[arg(long, value_name = "PATH", global = true)]
    pub manifest_path: Option<PathBuf>,
}

/// Discovers the governing project manifest based on the locator.
pub(super) fn discover(locator: &Locator) -> anyhow::Result<Project> {
    let start = match locator.manifest_path.as_deref() {
        Some(path) if path.is_file() => path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
        Some(path) if path.is_dir() => path.to_path_buf(),
        Some(path) => anyhow::bail!("manifest path `{}` does not exist", path.display()),
        None => std::env::current_dir().context("reading current directory")?,
    };

    ModuleProject::discover(&start)?
        .with_context(|| "no `module.json` found; run `sprocket dev module init` first")
}

/// Traces the discovered module project for a command.
pub(super) fn trace_project(command: &'static str, project: &Project) {
    tracing::debug!(
        command,
        module = %project.manifest().name,
        root = %project.root().display(),
        manifest = %project.manifest_path().display(),
        lockfile = %project.lockfile_path().display(),
        dependencies = project.manifest().dependencies.len(),
        "discovered module project"
    );
}

/// Loads `module-lock.json` when present.
pub(super) fn load_lockfile(project: &Project) -> anyhow::Result<Option<Lockfile>> {
    tracing::trace!(
        lockfile = %project.lockfile_path().display(),
        "reading module lockfile"
    );
    let lockfile = project.load_lockfile()?;
    if let Some(lockfile) = &lockfile {
        tracing::debug!(
            lockfile = %project.lockfile_path().display(),
            dependencies = lockfile.dependencies.len(),
            "loaded module lockfile"
        );
    }
    Ok(lockfile)
}

/// Loads `module-lock.json`, failing when it is absent.
pub(super) fn require_lockfile(project: &Project) -> anyhow::Result<Lockfile> {
    load_lockfile(project)?
        .ok_or_else(|| anyhow::anyhow!("no `module-lock.json`; run `sprocket dev module lock`"))
}
