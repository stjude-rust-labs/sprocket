//! Lockfile freshness enforcement before `run` and `submit`.
//!
//! This is the intentional cross-group module API: `run` and `submit` call
//! [`ensure_lockfile_current`] before executing, which either refreshes a stale
//! or missing lockfile or, under `--locked`, refuses to execute against one.

use std::path::Path;

use super::project::WriteIntent;
use super::project::load_lockfile;
use super::project::write_lockfile;
use super::relock::RelockPlanner;
use super::signer_policy::SignerChangeMode;
use crate::commands::output::CommandOutput;
use crate::config::Config;

/// What to do when `module-lock.json` is missing or out of date.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LockfilePolicy {
    /// Regenerate the lockfile so execution proceeds against a current tree.
    Regenerate,
    /// Refuse to execute, leaving the lockfile untouched.
    RequireCurrent,
}

/// Brings `module-lock.json` in line with the governing `module.json` before
/// execution, or fails when `policy` forbids regenerating it.
pub(crate) async fn ensure_lockfile_current(
    config: &Config,
    start: &Path,
    policy: LockfilePolicy,
) -> anyhow::Result<()> {
    let Some(project) = wdl_modules::project::ModuleProject::discover(start)? else {
        return Ok(());
    };
    if project.manifest().dependencies.is_empty() {
        return Ok(());
    }

    // Avoid taking the lockfile's exclusive lock when the lockfile already
    // satisfies the manifest.
    let existing = load_lockfile(&project)?;
    if existing
        .as_ref()
        .is_some_and(|lock| lock.satisfies_manifest(project.manifest()))
    {
        return Ok(());
    }
    if policy == LockfilePolicy::RequireCurrent {
        anyhow::bail!(
            "`module-lock.json` is missing or out of date with `module.json`; run `sprocket dev \
             module lock`"
        );
    }

    tracing::info!(
        manifest = %project.manifest_path().display(),
        lockfile_present = existing.is_some(),
        "`module-lock.json` is missing or out of date; regenerating before execution"
    );
    let baseline = existing.unwrap_or_default();
    let planner = RelockPlanner::new(config, &project, &baseline);
    let outcome = planner
        .plan_and_enforce(
            std::sync::Arc::new(project.manifest().clone()),
            SignerChangeMode::Strict,
            CommandOutput::new(false),
        )
        .await?;
    write_lockfile(
        &project,
        &outcome.lockfile,
        project.manifest(),
        WriteIntent::Satisfy,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use wdl_modules::Lockfile;
    use wdl_modules::Manifest;

    use super::*;

    #[tokio::test]
    async fn ensure_lockfile_current_regenerates_missing_lockfile() {
        let work = tempfile::tempdir().unwrap();
        let dep_dir = work.path().join("dep");
        std::fs::create_dir_all(&dep_dir).unwrap();
        std::fs::write(
            dep_dir.join("module.json"),
            br#"{"name":"dep","license":"MIT"}"#,
        )
        .unwrap();
        std::fs::write(dep_dir.join("index.wdl"), b"version 1.3\n").unwrap();

        let consumer_dir = work.path().join("consumer");
        std::fs::create_dir_all(&consumer_dir).unwrap();
        std::fs::write(
            consumer_dir.join("module.json"),
            br#"{"name":"consumer","license":"MIT","dependencies":{"dep":{"path":"../dep"}}}"#,
        )
        .unwrap();

        let lockfile_path = consumer_dir.join(wdl_modules::LOCKFILE_FILENAME);
        assert!(!lockfile_path.exists());

        let mut config = Config::default();
        config.modules.cache_path = Some(work.path().join("cache"));
        ensure_lockfile_current(&config, &consumer_dir, LockfilePolicy::Regenerate)
            .await
            .expect("regeneration should succeed for a local path dependency");

        assert!(lockfile_path.exists(), "lockfile should be created");
        let bytes = std::fs::read(&lockfile_path).unwrap();
        let lock = Lockfile::parse(&bytes).unwrap();
        let consumer_manifest =
            Manifest::parse(&std::fs::read(consumer_dir.join("module.json")).unwrap()).unwrap();
        assert!(lock.satisfies_manifest(&consumer_manifest));
    }

    #[tokio::test]
    async fn ensure_lockfile_current_is_noop_without_dependencies() {
        let work = tempfile::tempdir().unwrap();
        std::fs::write(
            work.path().join("module.json"),
            br#"{"name":"solo","license":"MIT"}"#,
        )
        .unwrap();

        ensure_lockfile_current(&Config::default(), work.path(), LockfilePolicy::Regenerate)
            .await
            .expect("no dependencies means nothing to lock");
        assert!(
            !work.path().join(wdl_modules::LOCKFILE_FILENAME).exists(),
            "a dependency-free module needs no lockfile"
        );
    }
}
