//! Automatic lockfile regeneration before `run` and `submit`.
//!
//! This is the intentional cross-group module API: `run` and `submit` call
//! [`ensure_lockfile_current`] to refresh a stale or missing lockfile before
//! executing.

use std::path::Path;
use std::sync::Arc;

use super::LockedProject;
use super::Project;
use super::ProjectUpdate;
use super::load_lockfile;
use super::relock::RelockPlanner;
use super::signer_policy::SignerChangeMode;
use crate::commands::output::CommandOutput;
use crate::config::Config;

/// Regenerates `module-lock.json` before execution when it is missing or
/// out of date with the governing `module.json`.
pub(crate) async fn ensure_lockfile_current(config: &Config, start: &Path) -> anyhow::Result<()> {
    let Some((manifest_path, manifest)) = crate::analysis::discover_manifest_upward(start)? else {
        return Ok(());
    };
    if manifest.dependencies.is_empty() {
        return Ok(());
    }

    let root = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let lockfile_path = manifest_path.with_file_name(wdl_modules::LOCKFILE_FILENAME);
    let project = Project {
        manifest_path,
        root,
        manifest: Arc::new(manifest),
        lockfile_path,
    };

    let existing = load_lockfile(&project)?;
    if existing
        .as_ref()
        .is_some_and(|lock| lock.satisfies_manifest(&project.manifest))
    {
        return Ok(());
    }

    let project = LockedProject::acquire(project)?;
    let existing = load_lockfile(project.project())?;
    if existing
        .as_ref()
        .is_some_and(|lock| lock.satisfies_manifest(&project.project().manifest))
    {
        return Ok(());
    }

    tracing::info!(
        manifest = %project.project().manifest_path.display(),
        lockfile_present = existing.is_some(),
        "`module-lock.json` is missing or out of date; regenerating before execution"
    );
    let planner = RelockPlanner::new(config, project.project());
    let outcome = planner
        .plan_and_enforce(
            project.project().manifest.clone(),
            SignerChangeMode::Strict,
            CommandOutput::new(false),
        )
        .await?;
    project.commit(ProjectUpdate::Lockfile(&outcome.lockfile))?;
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
        ensure_lockfile_current(&config, &consumer_dir)
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

        ensure_lockfile_current(&Config::default(), work.path())
            .await
            .expect("no dependencies means nothing to lock");
        assert!(
            !work.path().join(wdl_modules::LOCKFILE_FILENAME).exists(),
            "a dependency-free module needs no lockfile"
        );
    }
}
