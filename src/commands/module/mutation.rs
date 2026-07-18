//! Serialized and recoverable module project mutations.

mod transaction;

use std::fs::File;
use std::fs::OpenOptions;
use std::fs::TryLockError;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use wdl_modules::Lockfile;

use self::transaction::ProjectTransaction;
use super::Project;

const STATE_DIRECTORY: &str = ".sprocket";
const LOCK_FILENAME: &str = "module-mutation.lock";

/// A non-empty update applied atomically to a module project.
///
/// The variants are exhaustive and each carries at least one payload, so an
/// empty update can never be constructed.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ProjectUpdate<'a> {
    /// Rewrite only the manifest.
    Manifest(&'a serde_json::Value),
    /// Rewrite only the lockfile.
    Lockfile(&'a Lockfile),
    /// Rewrite both the manifest and the lockfile.
    Both {
        /// The updated manifest value.
        manifest: &'a serde_json::Value,
        /// The updated lockfile.
        lockfile: &'a Lockfile,
    },
}

impl<'a> ProjectUpdate<'a> {
    /// Returns the manifest payload when this update rewrites the manifest.
    fn manifest(self) -> Option<&'a serde_json::Value> {
        match self {
            Self::Manifest(manifest) | Self::Both { manifest, .. } => Some(manifest),
            Self::Lockfile(_) => None,
        }
    }

    /// Returns the lockfile payload when this update rewrites the lockfile.
    fn lockfile(self) -> Option<&'a Lockfile> {
        match self {
            Self::Lockfile(lockfile) | Self::Both { lockfile, .. } => Some(lockfile),
            Self::Manifest(_) => None,
        }
    }
}

/// A refreshed module project held under its exclusive mutation lock.
#[derive(Debug)]
pub(crate) struct LockedProject {
    project: Project,
    mutation: ProjectMutation,
}

impl LockedProject {
    /// Acquires the project lock, recovers interrupted work, and reloads the
    /// manifest under the lock.
    pub(crate) fn acquire(mut project: Project) -> anyhow::Result<Self> {
        let mutation = ProjectMutation::acquire(&project)?;
        project.reload()?;
        Ok(Self { project, mutation })
    }

    /// Returns the refreshed project snapshot protected by this lock.
    pub(crate) fn project(&self) -> &Project {
        &self.project
    }

    /// Atomically applies a non-empty manifest and/or lockfile update.
    pub(crate) fn commit(&self, update: ProjectUpdate<'_>) -> anyhow::Result<()> {
        self.mutation.commit(&self.project, update)
    }
}

/// An exclusive lock for mutations to one module project.
#[derive(Debug)]
struct ProjectMutation {
    _lock: File,
}

impl ProjectMutation {
    /// Acquires the project lock and recovers an interrupted mutation.
    pub(crate) fn acquire(project: &Project) -> anyhow::Result<Self> {
        let state = state_directory(&project.root)?;
        let lock_path = state.join(LOCK_FILENAME);
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("opening project mutation lock `{}`", lock_path.display()))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                tracing::info!(
                    lock = %lock_path.display(),
                    "waiting for another module command to finish"
                );
                lock.lock().with_context(|| {
                    format!("acquiring project mutation lock `{}`", lock_path.display())
                })?;
            }
            Err(TryLockError::Error(source)) => {
                return Err(source).with_context(|| {
                    format!("acquiring project mutation lock `{}`", lock_path.display())
                });
            }
        }

        transaction::remove_pending_directory(&state)?;
        transaction::recover_active_mutation(project, &state)?;
        Ok(Self { _lock: lock })
    }

    /// Atomically applies a non-empty manifest and/or lockfile update.
    pub(crate) fn commit(
        &self,
        project: &Project,
        update: ProjectUpdate<'_>,
    ) -> anyhow::Result<()> {
        transaction::validate_updates(update)?;
        let transaction = ProjectTransaction::begin(project)?;
        let result = (|| {
            if let Some(manifest) = update.manifest() {
                super::write_manifest_value(&project.manifest_path, manifest)?;
            }
            if let Some(lockfile) = update.lockfile() {
                super::write_lockfile(project, lockfile)?;
            }
            transaction::sync_project_files(&project.manifest_path, &project.lockfile_path)?;
            Ok(())
        })();

        match result {
            Ok(()) => transaction.finish(),
            Err(source) => match transaction.rollback() {
                Ok(()) => Err(source),
                Err(rollback) => Err(transaction::rollback_error(
                    source,
                    rollback,
                    &project.manifest_path,
                    &project.lockfile_path,
                )),
            },
        }
    }
}

/// Ensures the private project state directory is a regular directory.
fn state_directory(root: &Path) -> anyhow::Result<PathBuf> {
    let state = root.join(STATE_DIRECTORY);
    match std::fs::symlink_metadata(&state) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            anyhow::bail!(
                "module state path `{}` is not a regular directory",
                state.display()
            );
        }
        Ok(_) => {}
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&state).with_context(|| {
                format!("creating module state directory `{}`", state.display())
            })?;
        }
        Err(source) => {
            return Err(source)
                .with_context(|| format!("inspecting module state path `{}`", state.display()));
        }
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn test_project(root: &Path, lockfile_path: PathBuf) -> anyhow::Result<Project> {
        let manifest_path = root.join(wdl_modules::MANIFEST_FILENAME);
        std::fs::write(&manifest_path, br#"{"name":"test","license":"MIT"}"#)?;
        let manifest = Arc::new(wdl_modules::Manifest::parse(&std::fs::read(
            &manifest_path,
        )?)?);
        Ok(Project {
            manifest_path,
            root: root.to_path_buf(),
            manifest,
            lockfile_path,
        })
    }

    #[test]
    fn project_update_exposes_exactly_three_non_empty_shapes() {
        let manifest = serde_json::json!({"name": "test", "license": "MIT"});
        let lockfile = Lockfile::default();

        assert!(matches!(
            ProjectUpdate::Manifest(&manifest),
            ProjectUpdate::Manifest(_)
        ));
        assert!(matches!(
            ProjectUpdate::Lockfile(&lockfile),
            ProjectUpdate::Lockfile(_)
        ));
        let both = ProjectUpdate::Both {
            manifest: &manifest,
            lockfile: &lockfile,
        };
        assert!(matches!(both, ProjectUpdate::Both { .. }));
    }

    #[test]
    fn commit_writes_manifest_and_lockfile() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let lockfile_path = directory.path().join(wdl_modules::LOCKFILE_FILENAME);
        let project = test_project(directory.path(), lockfile_path.clone())?;
        let mutation = ProjectMutation::acquire(&project)?;

        let manifest = serde_json::json!({"name": "updated", "license": "MIT"});
        let lockfile = Lockfile::default();
        mutation.commit(
            &project,
            ProjectUpdate::Both {
                manifest: &manifest,
                lockfile: &lockfile,
            },
        )?;

        assert_eq!(
            super::super::read_manifest_value(&project.manifest_path)?,
            manifest
        );
        assert!(lockfile_path.is_file());
        let state = directory.path().join(STATE_DIRECTORY);
        assert!(!state.join("module-mutation.pending").exists());
        assert!(!state.join("module-mutation").exists());
        Ok(())
    }

    #[test]
    fn rejects_non_directory_transaction_state() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        std::fs::write(directory.path().join(STATE_DIRECTORY), b"not a directory")?;
        let project = test_project(
            directory.path(),
            directory.path().join(wdl_modules::LOCKFILE_FILENAME),
        )?;

        let error = ProjectMutation::acquire(&project)
            .expect_err("a non-directory transaction state should fail");

        assert!(error.to_string().contains("is not a regular directory"));
        Ok(())
    }
}
