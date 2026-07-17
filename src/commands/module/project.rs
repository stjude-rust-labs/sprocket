//! Module project discovery and lockfile loading.

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context as _;
use clap::Args as ClapArgs;
use wdl_modules::Lockfile;
use wdl_modules::Manifest;

/// Parsed module project context shared by porcelain subcommands.
#[derive(Debug, Clone)]
pub struct Project {
    /// Path to the discovered `module.json`.
    pub manifest_path: PathBuf,
    /// Root directory containing `module.json`.
    pub root: PathBuf,
    /// Parsed manifest discovered from disk.
    pub manifest: Arc<Manifest>,
    /// Path to the sibling `module-lock.json`.
    pub lockfile_path: PathBuf,
}

impl Project {
    /// Reloads the manifest while a project mutation lock is held.
    pub(crate) fn reload(&mut self) -> anyhow::Result<()> {
        let bytes = std::fs::read(&self.manifest_path)
            .with_context(|| format!("reading `{}`", self.manifest_path.display()))?;
        self.manifest = Arc::new(
            Manifest::parse(&bytes)
                .with_context(|| format!("parsing `{}`", self.manifest_path.display()))?,
        );
        Ok(())
    }
}

/// Locates the governing `module.json`.
#[derive(ClapArgs, Debug, Clone)]
pub struct Locator {
    /// Path to the `module.json` or its directory. Defaults to an upward
    /// search from the current directory.
    #[arg(long, value_name = "PATH", global = true)]
    pub manifest_path: Option<PathBuf>,
}

/// Discovers the governing project manifest based on the locator.
pub fn discover(locator: &Locator) -> anyhow::Result<Project> {
    let start = match locator.manifest_path.as_deref() {
        Some(path) if path.is_file() => path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
        Some(path) if path.is_dir() => path.to_path_buf(),
        Some(path) => anyhow::bail!("manifest path `{}` does not exist", path.display()),
        None => std::env::current_dir().context("reading current directory")?,
    };

    let (manifest_path, manifest) = crate::analysis::discover_manifest_upward(&start)?
        .with_context(|| "no `module.json` found; run `sprocket dev module init` first")?;

    let root = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let lockfile_path = manifest_path.with_file_name(wdl_modules::LOCKFILE_FILENAME);

    Ok(Project {
        manifest_path,
        root,
        manifest: Arc::new(manifest),
        lockfile_path,
    })
}

/// Traces the discovered module project for a command.
pub(crate) fn trace_project(command: &'static str, project: &Project) {
    tracing::debug!(
        command,
        module = %project.manifest.name,
        root = %project.root.display(),
        manifest = %project.manifest_path.display(),
        lockfile = %project.lockfile_path.display(),
        dependencies = project.manifest.dependencies.len(),
        "discovered module project"
    );
}

/// Loads `module-lock.json` when present.
pub fn load_lockfile(project: &Project) -> anyhow::Result<Option<Lockfile>> {
    if !project.lockfile_path.exists() {
        tracing::trace!(lockfile = %project.lockfile_path.display(), "module lockfile is absent");
        return Ok(None);
    }

    tracing::trace!(lockfile = %project.lockfile_path.display(), "reading module lockfile");
    let bytes = std::fs::read(&project.lockfile_path)
        .with_context(|| format!("reading `{}`", project.lockfile_path.display()))?;
    let lock = Lockfile::parse(&bytes)
        .with_context(|| format!("parsing `{}`", project.lockfile_path.display()))?;
    tracing::debug!(
        lockfile = %project.lockfile_path.display(),
        dependencies = lock.dependencies.len(),
        "loaded module lockfile"
    );
    Ok(Some(lock))
}

/// Loads `module-lock.json`, failing when it is absent.
pub(crate) fn require_lockfile(project: &Project) -> anyhow::Result<Lockfile> {
    load_lockfile(project)?
        .ok_or_else(|| anyhow::anyhow!("no `module-lock.json`; run `sprocket dev module lock`"))
}
