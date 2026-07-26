mod transaction;

use std::fs::File;
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;

use sha2::Digest as _;
use sha2::Sha256;
use thiserror::Error;

use super::ManifestDocument;
use super::ManifestDocumentError;
use super::ModuleProject;
use super::ProjectError;
use crate::Lockfile;

/// A non-empty update applied atomically to a module project.
#[derive(Clone, Copy, Debug)]
pub enum ProjectUpdate<'a> {
    /// Rewrite only the manifest.
    Manifest(&'a ManifestDocument),
    /// Rewrite only the lockfile.
    Lockfile(&'a Lockfile),
    /// Rewrite both the manifest and the lockfile.
    Both {
        /// The updated manifest document.
        manifest: &'a ManifestDocument,
        /// The updated lockfile.
        lockfile: &'a Lockfile,
    },
}

impl<'a> ProjectUpdate<'a> {
    fn manifest(self) -> Option<&'a ManifestDocument> {
        match self {
            Self::Manifest(manifest) | Self::Both { manifest, .. } => Some(manifest),
            Self::Lockfile(_) => None,
        }
    }

    fn lockfile(self) -> Option<&'a Lockfile> {
        match self {
            Self::Lockfile(lockfile) | Self::Both { lockfile, .. } => Some(lockfile),
            Self::Manifest(_) => None,
        }
    }
}

/// An error applying a mutation to a module project.
#[derive(Debug, Error)]
pub enum ProjectMutationError {
    /// A project operation failed.
    #[error("project error")]
    Project(#[from] ProjectError),
    /// The manifest document could not be serialized.
    #[error("invalid manifest update")]
    Manifest(#[from] ManifestDocumentError),
    /// An i/o operation failed.
    #[error("{operation} `{path}`")]
    Io {
        /// A short description of the failing operation.
        operation: &'static str,
        /// The path involved in the failure.
        path: PathBuf,
        /// The underlying i/o error.
        #[source]
        source: std::io::Error,
    },
    /// A project or journal path was not the expected kind of filesystem entry.
    #[error("project path `{path}` is not a regular {expected}")]
    InvalidPath {
        /// The offending path.
        path: PathBuf,
        /// The kind of entry that was expected.
        expected: &'static str,
    },
    /// The mutation failed and the rollback also failed.
    #[error(
        "rolling back the interrupted module project mutation after {source} also failed; \
         inspect manifest `{manifest_path}` and lockfile `{lockfile_path}` \
         for manual recovery; rollback: {rollback}"
    )]
    Rollback {
        /// The manifest path that may need manual recovery.
        manifest_path: PathBuf,
        /// The lockfile path that may need manual recovery.
        lockfile_path: PathBuf,
        /// The original mutation error.
        #[source]
        source: Box<ProjectMutationError>,
        /// The rollback error.
        rollback: Box<ProjectMutationError>,
    },
}

/// A module project held under its exclusive global mutation lock.
#[derive(Debug)]
pub struct LockedModuleProject {
    project: ModuleProject,
    journal_root: PathBuf,
    _lock: File,
}

impl LockedModuleProject {
    /// Acquires the project mutation lock, recovers any interrupted work,
    /// then reloads the manifest document under the lock.
    pub fn acquire(
        mut project: ModuleProject,
        state_root: &Path,
    ) -> Result<Self, ProjectMutationError> {
        let key = project_key(project.root())?;
        let journal_root = journal_root(state_root, &key);
        std::fs::create_dir_all(state_root).map_err(|source| ProjectMutationError::Io {
            operation: "creating mutation state directory",
            path: state_root.to_path_buf(),
            source,
        })?;
        let lock_root = state_root.join("locks");
        transaction::ensure_managed_directory(&lock_root)?;
        let lock_path = lock_root.join(format!("{key}.lock"));
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|source| ProjectMutationError::Io {
                operation: "opening project mutation lock",
                path: lock_path.clone(),
                source,
            })?;
        lock.lock().map_err(|source| ProjectMutationError::Io {
            operation: "acquiring project mutation lock",
            path: lock_path,
            source,
        })?;
        let journals_dir = state_root.join("journals");
        transaction::ensure_managed_directory(&journals_dir)?;
        transaction::ensure_managed_directory(&journal_root)?;
        transaction::remove_pending_directory(&journal_root)?;
        transaction::recover_active_mutation(&project, &journal_root)?;
        project.reload()?;
        Ok(Self {
            project,
            journal_root,
            _lock: lock,
        })
    }

    /// Returns the refreshed project snapshot held under this lock.
    pub fn project(&self) -> &ModuleProject {
        &self.project
    }

    /// Atomically applies the update to the project on disk.
    pub fn commit(&self, update: ProjectUpdate<'_>) -> Result<(), ProjectMutationError> {
        transaction::commit(&self.project, &self.journal_root, update)
    }
}

fn project_key(root: &Path) -> Result<String, ProjectMutationError> {
    let canonical = root.canonicalize().map_err(|source| ProjectMutationError::Io {
        operation: "canonicalizing module root",
        path: root.to_path_buf(),
        source,
    })?;
    Ok(hex::encode(Sha256::digest(
        canonical.as_os_str().as_encoded_bytes(),
    )))
}

fn journal_root(state_root: &Path, project_key: &str) -> PathBuf {
    state_root.join("journals").join(project_key)
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;
    use crate::project::ModuleProject;

    fn project(root: &Path) -> ModuleProject {
        std::fs::create_dir_all(root).unwrap();
        std::fs::write(
            root.join(crate::MANIFEST_FILENAME),
            br#"{"name":"test","license":"MIT"}"#,
        )
        .unwrap();
        ModuleProject::load(root.join(crate::MANIFEST_FILENAME)).unwrap()
    }

    #[test]
    fn paired_commit_writes_both_files_without_local_state() {
        let directory = tempfile::tempdir().unwrap();
        let project_root = directory.path().join("project");
        let state_root = directory.path().join("state");
        let project = project(&project_root);
        let mut document = project.document().clone();
        document
            .set_dependency(
                "dep",
                &serde_json::from_str(r#"{"path":"../dep"}"#).unwrap(),
            )
            .unwrap();
        let lockfile = crate::Lockfile::default();

        let locked = LockedModuleProject::acquire(project, &state_root).unwrap();
        locked
            .commit(ProjectUpdate::Both {
                manifest: &document,
                lockfile: &lockfile,
            })
            .unwrap();

        assert!(project_root.join(crate::LOCKFILE_FILENAME).is_file());
        assert!(!project_root.join(".sprocket").exists());
        assert!(state_root.join("locks").is_dir());
    }

    #[test]
    fn lock_serializes_concurrent_acquirers() {
        let directory = tempfile::tempdir().unwrap();
        let project = project(&directory.path().join("project"));
        let state_root = directory.path().join("state");
        let first = LockedModuleProject::acquire(project.clone(), &state_root).unwrap();
        let (sender, receiver) = mpsc::channel();
        let thread = std::thread::spawn({
            let state_root = state_root.clone();
            move || {
                sender
                    .send(LockedModuleProject::acquire(project, &state_root))
                    .unwrap();
            }
        });

        assert!(receiver.recv_timeout(Duration::from_millis(100)).is_err());
        drop(first);
        receiver.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        thread.join().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn path_aliases_share_a_lock() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        let project = project(&root);
        let alias = directory.path().join("alias");
        std::os::unix::fs::symlink(&root, &alias).unwrap();
        let alias_project =
            ModuleProject::load(alias.join(crate::MANIFEST_FILENAME)).unwrap();

        assert_eq!(
            project_key(project.root()).unwrap(),
            project_key(alias_project.root()).unwrap()
        );
    }

    #[test]
    fn rollback_error_renders_both_failures_inline() {
        let manifest_path = PathBuf::from("/worktree/module.json");
        let lockfile_path = PathBuf::from("/worktree/module-lock.json");
        let error = ProjectMutationError::Rollback {
            manifest_path: manifest_path.clone(),
            lockfile_path: lockfile_path.clone(),
            source: Box::new(ProjectMutationError::Io {
                operation: "writing",
                path: lockfile_path.clone(),
                source: std::io::Error::other("mutation write failed"),
            }),
            rollback: Box::new(ProjectMutationError::Io {
                operation: "restoring",
                path: manifest_path.clone(),
                source: std::io::Error::other("rollback restore failed"),
            }),
        };

        let rendered = error.to_string();

        assert!(rendered.contains(&format!("writing `{}`", lockfile_path.display())));
        assert!(rendered.contains(&format!("restoring `{}`", manifest_path.display())));
        assert!(rendered.contains(&manifest_path.display().to_string()));
        assert!(rendered.contains(&lockfile_path.display().to_string()));
        assert_eq!(
            error.source().unwrap().to_string(),
            format!("writing `{}`", lockfile_path.display())
        );
    }
}
