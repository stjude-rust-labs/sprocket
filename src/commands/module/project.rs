//! Module project discovery and lockfile loading.

use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use clap::Args as ClapArgs;
use wdl_modules::Lockfile;
use wdl_modules::Manifest;
use wdl_modules::project::LockedLockfile;
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
    let lockfile = project.load_lockfile()?;
    match &lockfile {
        Some(lockfile) => {
            tracing::debug!(
                lockfile = %project.lockfile_path().display(),
                dependencies = lockfile.dependencies.len(),
                "loaded module lockfile"
            );
        }
        None => {
            tracing::trace!(
                lockfile = %project.lockfile_path().display(),
                "module lockfile is absent"
            );
        }
    }
    Ok(lockfile)
}

/// The `--locked` flag shared by commands that read `module-lock.json`.
#[derive(ClapArgs, Debug, Clone, Copy)]
pub(super) struct LockedFlag {
    /// Fail if `module-lock.json` is missing or out of date with `module.json`.
    #[arg(long = "locked")]
    pub enabled: bool,
}

/// Loads `module-lock.json`, failing when it is absent.
///
/// With `--locked`, a lockfile that no longer satisfies `module.json` fails as
/// well, so a stale lockfile cannot silently change what a command reads.
pub(super) fn require_lockfile(project: &Project, locked: LockedFlag) -> anyhow::Result<Lockfile> {
    let lockfile = load_lockfile(project)?
        .ok_or_else(|| anyhow::anyhow!("no `module-lock.json`; run `sprocket dev module lock`"))?;
    if locked.enabled && !lockfile.satisfies_manifest(project.manifest()) {
        anyhow::bail!("`module-lock.json` is out of date with `module.json`");
    }
    Ok(lockfile)
}

/// What a command intends when it writes `module-lock.json`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WriteIntent {
    /// Bring the lockfile in line with the manifest, as `add`, `remove`,
    /// `lock`, and automatic locking do.
    Satisfy,
    /// Install a freshly resolved lockfile even when the current one already
    /// satisfies the manifest, as `update` and `upgrade` do.
    Refresh,
}

/// Whether [`write_lockfile`] replaced `module-lock.json`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LockfileWrite {
    /// The lockfile was replaced.
    Wrote,
    /// The lockfile on disk was left as it stood.
    Kept,
}

/// Writes `module-lock.json` under its exclusive advisory lock.
///
/// The lock is taken here rather than at command start, so a command that
/// decides not to write never creates the file. `manifest` is the manifest the
/// caller intends the lockfile to match, which for `add`, `remove`, and
/// `upgrade` is the edited document rather than the manifest read at discovery.
///
/// Nothing is written when the file on disk already holds this exact lockfile.
/// Under [`WriteIntent::Satisfy`] nothing is written when the file on disk
/// already satisfies `manifest` either, which keeps a concurrent process's
/// equally-current result rather than replacing it.
pub(super) fn write_lockfile(
    project: &Project,
    lockfile: &Lockfile,
    manifest: &Manifest,
    intent: WriteIntent,
) -> anyhow::Result<LockfileWrite> {
    let guard = LockedLockfile::acquire(project.lockfile_path())?;
    if let Some(current) = guard.current()? {
        if current == *lockfile {
            tracing::debug!("module lockfile is already up to date; not rewriting it");
            return Ok(LockfileWrite::Kept);
        }
        if intent == WriteIntent::Satisfy && current.satisfies_manifest(manifest) {
            tracing::debug!("another process wrote a satisfying module lockfile; keeping it");
            return Ok(LockfileWrite::Kept);
        }
    }
    guard.write(lockfile)?;
    tracing::debug!(lockfile = %project.lockfile_path().display(), "wrote module lockfile");
    Ok(LockfileWrite::Wrote)
}

#[cfg(test)]
mod tests {
    use wdl_modules::Lockfile;

    use super::*;

    /// A manifest declaring one Git dependency constrained to `^1`.
    const MANIFEST: &[u8] = br#"{"name":"consumer","license":"MIT","dependencies":{"foo":{"git":"https://github.com/openwdl/foo","version":"^1"}}}"#;

    /// Builds a lockfile that satisfies `MANIFEST` and pins `sha`.
    fn lockfile_pinning(sha: &str) -> anyhow::Result<Lockfile> {
        Ok(Lockfile::parse(
            format!(
                r#"{{
                    "version":1,
                    "dependencies":{{
                        "foo":{{
                            "source":{{
                                "git":"https://github.com/openwdl/foo",
                                "sha":"{sha}",
                                "selector":{{"version":"^1"}}
                            }},
                            "checksum":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                            "dependencies":{{}}
                        }}
                    }}
                }}"#
            )
            .as_bytes(),
        )?)
    }

    /// Writes `MANIFEST` into `root` and loads it as a project.
    fn project(root: &std::path::Path) -> anyhow::Result<Project> {
        std::fs::create_dir_all(root)?;
        std::fs::write(root.join("module.json"), MANIFEST)?;
        Ok(Project::load(root.join("module.json"))?)
    }

    #[test]
    fn writing_the_same_lockfile_twice_leaves_the_file_alone() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let project = project(directory.path())?;
        let lockfile = lockfile_pinning("0000000000000000000000000000000000000001")?;
        write_lockfile(
            &project,
            &lockfile,
            project.manifest(),
            WriteIntent::Satisfy,
        )?;
        let before = std::fs::metadata(project.lockfile_path())?.modified()?;

        let outcome = write_lockfile(
            &project,
            &lockfile,
            project.manifest(),
            WriteIntent::Satisfy,
        )?;

        assert_eq!(outcome, LockfileWrite::Kept);
        assert_eq!(
            std::fs::metadata(project.lockfile_path())?.modified()?,
            before,
            "an unchanged lockfile must not be rewritten"
        );
        Ok(())
    }

    #[test]
    fn satisfy_intent_keeps_a_lockfile_another_process_already_satisfied() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let project = project(directory.path())?;
        let existing = lockfile_pinning("0000000000000000000000000000000000000001")?;
        let resolved = lockfile_pinning("0000000000000000000000000000000000000002")?;
        write_lockfile(
            &project,
            &existing,
            project.manifest(),
            WriteIntent::Satisfy,
        )?;

        let outcome = write_lockfile(
            &project,
            &resolved,
            project.manifest(),
            WriteIntent::Satisfy,
        )?;

        assert_eq!(outcome, LockfileWrite::Kept);

        assert_eq!(
            LockedLockfile::read(project.lockfile_path())?.as_ref(),
            Some(&existing),
            "a satisfying lockfile must not be replaced under `Satisfy`"
        );
        Ok(())
    }

    #[test]
    fn refresh_intent_replaces_a_satisfying_lockfile() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let project = project(directory.path())?;
        let existing = lockfile_pinning("0000000000000000000000000000000000000001")?;
        let resolved = lockfile_pinning("0000000000000000000000000000000000000002")?;
        write_lockfile(
            &project,
            &existing,
            project.manifest(),
            WriteIntent::Satisfy,
        )?;

        let outcome = write_lockfile(
            &project,
            &resolved,
            project.manifest(),
            WriteIntent::Refresh,
        )?;

        assert_eq!(outcome, LockfileWrite::Wrote);
        assert_eq!(
            LockedLockfile::read(project.lockfile_path())?.as_ref(),
            Some(&resolved),
            "`Refresh` must install the freshly resolved lockfile"
        );
        Ok(())
    }

    #[test]
    fn satisfy_intent_writes_for_a_manifest_the_caller_is_about_to_save() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let project = project(directory.path())?;
        let current = lockfile_pinning("0000000000000000000000000000000000000001")?;
        write_lockfile(&project, &current, project.manifest(), WriteIntent::Satisfy)?;
        // The pending manifest declares a second dependency, so the lockfile on
        // disk satisfies the manifest read at discovery but not the one this
        // write targets.
        let pending = Manifest::parse(
            br#"{"name":"consumer","license":"MIT","dependencies":{"foo":{"git":"https://github.com/openwdl/foo","version":"^1"},"bar":{"git":"https://github.com/openwdl/bar","version":"^2"}}}"#,
        )?;
        let resolved = two_entry_lockfile()?;
        assert!(
            current.satisfies_manifest(project.manifest()),
            "the fixture must start from a lockfile current for the discovered manifest"
        );

        let outcome = write_lockfile(&project, &resolved, &pending, WriteIntent::Satisfy)?;

        assert_eq!(
            outcome,
            LockfileWrite::Wrote,
            "a lockfile that satisfies only the pre-edit manifest must not suppress the write"
        );
        assert_eq!(
            LockedLockfile::read(project.lockfile_path())?.as_ref(),
            Some(&resolved)
        );
        Ok(())
    }

    /// Builds a lockfile that satisfies the two-dependency pending manifest.
    fn two_entry_lockfile() -> anyhow::Result<Lockfile> {
        Ok(Lockfile::parse(
            br#"{
                "version":1,
                "dependencies":{
                    "foo":{
                        "source":{
                            "git":"https://github.com/openwdl/foo",
                            "sha":"0000000000000000000000000000000000000001",
                            "selector":{"version":"^1"}
                        },
                        "checksum":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                        "dependencies":{}
                    },
                    "bar":{
                        "source":{
                            "git":"https://github.com/openwdl/bar",
                            "sha":"0000000000000000000000000000000000000003",
                            "selector":{"version":"^2"}
                        },
                        "checksum":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                        "dependencies":{}
                    }
                }
            }"#,
        )?)
    }
}
