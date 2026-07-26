//! Journal, snapshot, recovery, and durable-write mechanics for module
//! project mutations.

use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;

use super::ManifestDocument;
use super::ModuleProject;
use super::ProjectMutationError;
use super::ProjectUpdate;
use crate::Lockfile;

/// Name of the active recovery journal directory.
pub(super) const ACTIVE_DIRECTORY: &str = "active";
/// Name used while a recovery journal is being created.
const PENDING_DIRECTORY: &str = "pending";

/// On-disk snapshots used to recover a project mutation.
#[derive(Debug)]
pub(super) struct ProjectTransaction {
    journal_root: PathBuf,
    active: PathBuf,
    manifest_path: PathBuf,
    lockfile_path: PathBuf,
}

impl ProjectTransaction {
    /// Snapshots both project files into a new recovery journal.
    pub(super) fn begin(
        project: &ModuleProject,
        journal_root: &Path,
    ) -> Result<Self, ProjectMutationError> {
        ensure_journal_root(journal_root)?;
        let pending = journal_root.join(PENDING_DIRECTORY);
        let active = journal_root.join(ACTIVE_DIRECTORY);
        remove_path_if_present(&pending)?;
        std::fs::create_dir(&pending).map_err(|source| ProjectMutationError::Io {
            operation: "creating mutation journal",
            path: pending.clone(),
            source,
        })?;
        snapshot_file(&pending, "manifest", project.manifest_path())?;
        snapshot_file(&pending, "lockfile", project.lockfile_path())?;
        sync_directory(&pending)?;
        std::fs::rename(&pending, &active).map_err(|source| ProjectMutationError::Io {
            operation: "activating module mutation journal",
            path: active.clone(),
            source,
        })?;
        sync_directory(journal_root)?;
        Ok(Self {
            journal_root: journal_root.to_path_buf(),
            active,
            manifest_path: project.manifest_path().to_path_buf(),
            lockfile_path: project.lockfile_path().to_path_buf(),
        })
    }

    /// Commits the mutation by removing the recovery journal.
    pub(super) fn finish(self) -> Result<(), ProjectMutationError> {
        remove_path_if_present(&self.active)?;
        sync_directory(&self.journal_root)?;
        cleanup_journal_if_empty(&self.journal_root);
        Ok(())
    }

    /// Restores both project files from the recovery journal.
    pub(super) fn rollback(self) -> Result<(), ProjectMutationError> {
        restore_snapshot(&self.active, "manifest", &self.manifest_path)?;
        restore_snapshot(&self.active, "lockfile", &self.lockfile_path)?;
        sync_project_files(&self.manifest_path, &self.lockfile_path)?;
        remove_path_if_present(&self.active)?;
        sync_directory(&self.journal_root)?;
        cleanup_journal_if_empty(&self.journal_root);
        Ok(())
    }
}

/// Validates and durably writes an update to the project files.
pub(super) fn commit(
    project: &ModuleProject,
    journal_root: &Path,
    update: ProjectUpdate<'_>,
) -> Result<(), ProjectMutationError> {
    validate_updates(update)?;
    let transaction = ProjectTransaction::begin(project, journal_root)?;
    let result = (|| {
        if let Some(document) = update.manifest() {
            write_manifest_atomically(project.manifest_path(), document)?;
        }
        if let Some(lockfile) = update.lockfile() {
            write_lockfile_atomically(project.lockfile_path(), lockfile)?;
        }
        sync_project_files(project.manifest_path(), project.lockfile_path())?;
        Ok(())
    })();

    match result {
        Ok(()) => transaction.finish(),
        Err(source) => match transaction.rollback() {
            Ok(()) => Err(source),
            Err(rollback) => Err(ProjectMutationError::Rollback {
                manifest_path: project.manifest_path().to_path_buf(),
                lockfile_path: project.lockfile_path().to_path_buf(),
                source: Box::new(source),
                rollback: Box::new(rollback),
            }),
        },
    }
}

/// Ensures `path` is a non-symlink directory owned by this process, creating
/// it if absent.
///
/// Accepts an existing real directory or a freshly created one. Rejects
/// symlinks and non-directory entries with `InvalidPath`. Handles creation
/// races by re-inspecting after `AlreadyExists`.
pub(super) fn ensure_journal_root(path: &Path) -> Result<(), ProjectMutationError> {
    loop {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(ProjectMutationError::InvalidPath {
                        path: path.to_path_buf(),
                        expected: "directory",
                    });
                }
                return Ok(());
            }
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                match std::fs::create_dir(path) {
                    Ok(()) => return Ok(()),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(source) => {
                        return Err(ProjectMutationError::Io {
                            operation: "creating mutation journal directory",
                            path: path.to_path_buf(),
                            source,
                        });
                    }
                }
            }
            Err(source) => {
                return Err(ProjectMutationError::Io {
                    operation: "inspecting mutation journal directory",
                    path: path.to_path_buf(),
                    source,
                });
            }
        }
    }
}

/// Removes an incomplete pending journal.
pub(super) fn remove_pending_directory(
    journal_root: &Path,
) -> Result<(), ProjectMutationError> {
    remove_path_if_present(&journal_root.join(PENDING_DIRECTORY))
}

/// Restores an interrupted transaction left by another process.
pub(super) fn recover_active_mutation(
    project: &ModuleProject,
    journal_root: &Path,
) -> Result<(), ProjectMutationError> {
    let active = journal_root.join(ACTIVE_DIRECTORY);
    match std::fs::symlink_metadata(&active) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(ProjectMutationError::InvalidPath {
                path: active,
                expected: "directory",
            });
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(ProjectMutationError::Io {
                operation: "inspecting mutation journal",
                path: active,
                source,
            });
        }
    }
    restore_snapshot(&active, "manifest", project.manifest_path())?;
    restore_snapshot(&active, "lockfile", project.lockfile_path())?;
    sync_project_files(project.manifest_path(), project.lockfile_path())?;
    remove_path_if_present(&active)?;
    sync_directory(journal_root)?;
    cleanup_journal_if_empty(journal_root);
    Ok(())
}

#[cfg(test)]
thread_local! {
    static FAIL_MANIFEST_RESTORE: std::cell::RefCell<Option<PathBuf>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
struct ManifestRestoreFailureGuard;

#[cfg(test)]
impl Drop for ManifestRestoreFailureGuard {
    fn drop(&mut self) {
        FAIL_MANIFEST_RESTORE.with(|path| {
            path.borrow_mut().take();
        });
    }
}

#[cfg(test)]
fn fail_manifest_restore_for_test(path: &Path) -> ManifestRestoreFailureGuard {
    FAIL_MANIFEST_RESTORE.with(|configured| {
        *configured.borrow_mut() = Some(path.to_path_buf());
    });
    ManifestRestoreFailureGuard
}

#[cfg(test)]
fn maybe_fail_manifest_restore(path: &Path) -> Result<(), ProjectMutationError> {
    let should_fail = FAIL_MANIFEST_RESTORE.with(|configured| {
        configured.borrow().as_deref() == Some(path)
    });
    if should_fail {
        return Err(ProjectMutationError::Io {
            operation: "restoring",
            path: path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::Other,
                "injected manifest restore failure",
            ),
        });
    }
    Ok(())
}

/// Makes both project files and their containing directory durable.
pub(super) fn sync_project_files(
    manifest_path: &Path,
    lockfile_path: &Path,
) -> Result<(), ProjectMutationError> {
    for path in [manifest_path, lockfile_path] {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                OpenOptions::new()
                    .write(true)
                    .open(path)
                    .and_then(|file| file.sync_all())
                    .map_err(|source| ProjectMutationError::Io {
                        operation: "syncing",
                        path: path.to_path_buf(),
                        source,
                    })?;
            }
            Ok(_) => {
                return Err(ProjectMutationError::InvalidPath {
                    path: path.to_path_buf(),
                    expected: "file",
                });
            }
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(ProjectMutationError::Io {
                    operation: "inspecting",
                    path: path.to_path_buf(),
                    source,
                });
            }
        }
    }
    let directory = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    sync_directory(directory)
}

/// Validates both serialized outputs before creating a recovery journal.
fn validate_updates(update: ProjectUpdate<'_>) -> Result<(), ProjectMutationError> {
    if let Some(document) = update.manifest() {
        document.to_bytes()?;
    }
    if let Some(lockfile) = update.lockfile() {
        lockfile
            .write(std::io::sink())
            .map_err(|source| ProjectMutationError::Io {
                operation: "serializing `module-lock.json`",
                path: PathBuf::from("module-lock.json"),
                source,
            })?;
    }
    Ok(())
}

/// Writes the manifest document to `path` atomically.
fn write_manifest_atomically(
    path: &Path,
    document: &ManifestDocument,
) -> Result<(), ProjectMutationError> {
    let bytes = document.to_bytes()?;
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp =
        tempfile::NamedTempFile::new_in(directory).map_err(|source| ProjectMutationError::Io {
            operation: "creating a temporary file in",
            path: directory.to_path_buf(),
            source,
        })?;
    temp.write_all(&bytes).map_err(|source| ProjectMutationError::Io {
        operation: "writing",
        path: temp.path().to_path_buf(),
        source,
    })?;
    align_temp_permissions(&temp, path)?;
    temp.persist(path).map_err(|e| ProjectMutationError::Io {
        operation: "replacing",
        path: path.to_path_buf(),
        source: e.error,
    })?;
    Ok(())
}

/// Writes the lockfile to `path` atomically.
fn write_lockfile_atomically(
    path: &Path,
    lockfile: &Lockfile,
) -> Result<(), ProjectMutationError> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp =
        tempfile::NamedTempFile::new_in(directory).map_err(|source| ProjectMutationError::Io {
            operation: "creating a temporary file in",
            path: directory.to_path_buf(),
            source,
        })?;
    lockfile.write(&mut temp).map_err(|source| ProjectMutationError::Io {
        operation: "writing",
        path: temp.path().to_path_buf(),
        source,
    })?;
    align_temp_permissions(&temp, path)?;
    temp.persist(path).map_err(|e| ProjectMutationError::Io {
        operation: "replacing",
        path: path.to_path_buf(),
        source: e.error,
    })?;
    Ok(())
}

/// Aligns a temporary file's permissions with the destination before rename.
fn align_temp_permissions(
    temp: &tempfile::NamedTempFile,
    path: &Path,
) -> Result<(), ProjectMutationError> {
    if let Ok(metadata) = std::fs::metadata(path) {
        temp.as_file()
            .set_permissions(metadata.permissions())
            .map_err(|source| ProjectMutationError::Io {
                operation: "setting permissions on",
                path: temp.path().to_path_buf(),
                source,
            })?;
        return Ok(());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        temp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o644))
            .map_err(|source| ProjectMutationError::Io {
                operation: "setting permissions on",
                path: temp.path().to_path_buf(),
                source,
            })?;
    }

    Ok(())
}

/// Saves one file as `<label>.before` or records its absence as `<label>.absent`.
fn snapshot_file(
    journal: &Path,
    label: &str,
    path: &Path,
) -> Result<(), ProjectMutationError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
            return Err(ProjectMutationError::InvalidPath {
                path: path.to_path_buf(),
                expected: "file",
            });
        }
        Ok(_) => {
            let bytes = std::fs::read(path).map_err(|source| ProjectMutationError::Io {
                operation: "reading",
                path: path.to_path_buf(),
                source,
            })?;
            let snapshot = journal.join(format!("{label}.before"));
            let mut snapshot_file =
                File::create(&snapshot).map_err(|source| ProjectMutationError::Io {
                    operation: "writing mutation snapshot",
                    path: snapshot.clone(),
                    source,
                })?;
            snapshot_file.write_all(&bytes).map_err(|source| ProjectMutationError::Io {
                operation: "writing mutation snapshot",
                path: snapshot.clone(),
                source,
            })?;
            snapshot_file.sync_all().map_err(|source| ProjectMutationError::Io {
                operation: "syncing mutation snapshot",
                path: snapshot,
                source,
            })?;
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            let marker = journal.join(format!("{label}.absent"));
            let marker_file =
                File::create(&marker).map_err(|source| ProjectMutationError::Io {
                    operation: "writing mutation marker",
                    path: marker.clone(),
                    source,
                })?;
            marker_file.sync_all().map_err(|source| ProjectMutationError::Io {
                operation: "syncing mutation marker",
                path: marker,
                source,
            })?;
        }
        Err(source) => {
            return Err(ProjectMutationError::Io {
                operation: "inspecting",
                path: path.to_path_buf(),
                source,
            });
        }
    }
    Ok(())
}

/// Restores one file from the journal snapshot at `<label>.before` or removes
/// it when the journal records `<label>.absent`.
fn restore_snapshot(
    journal: &Path,
    label: &str,
    path: &Path,
) -> Result<(), ProjectMutationError> {
    let snapshot = journal.join(format!("{label}.before"));
    let absent = journal.join(format!("{label}.absent"));

    match std::fs::symlink_metadata(&snapshot) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            let bytes =
                std::fs::read(&snapshot).map_err(|source| ProjectMutationError::Io {
                    operation: "reading mutation snapshot",
                    path: snapshot.clone(),
                    source,
                })?;
            return write_bytes_atomically(path, &bytes);
        }
        Ok(_) => {
            return Err(ProjectMutationError::InvalidPath {
                path: snapshot,
                expected: "file",
            });
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(ProjectMutationError::Io {
                operation: "inspecting mutation snapshot",
                path: snapshot,
                source,
            });
        }
    }

    match std::fs::symlink_metadata(&absent) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(ProjectMutationError::Io {
                        operation: "removing",
                        path: path.to_path_buf(),
                        source,
                    });
                }
            }
            return Ok(());
        }
        Ok(_) => {
            return Err(ProjectMutationError::InvalidPath {
                path: absent,
                expected: "file",
            });
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(ProjectMutationError::Io {
                operation: "inspecting mutation marker",
                path: absent,
                source,
            });
        }
    }

    Err(ProjectMutationError::Io {
        operation: "module mutation journal has no snapshot",
        path: journal.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no `{label}` snapshot"),
        ),
    })
}

/// Replaces a file with recovery bytes using an atomic rename.
fn write_bytes_atomically(path: &Path, bytes: &[u8]) -> Result<(), ProjectMutationError> {
    #[cfg(test)]
    maybe_fail_manifest_restore(path)?;

    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp =
        tempfile::NamedTempFile::new_in(directory).map_err(|source| ProjectMutationError::Io {
            operation: "creating a temporary file in",
            path: directory.to_path_buf(),
            source,
        })?;
    temp.write_all(bytes).map_err(|source| ProjectMutationError::Io {
        operation: "writing",
        path: temp.path().to_path_buf(),
        source,
    })?;
    align_temp_permissions(&temp, path)?;
    temp.persist(path).map_err(|e| ProjectMutationError::Io {
        operation: "restoring",
        path: path.to_path_buf(),
        source: e.error,
    })?;
    Ok(())
}

/// Removes a journal path without following symbolic links.
fn remove_path_if_present(path: &Path) -> Result<(), ProjectMutationError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(ProjectMutationError::Io {
                operation: "inspecting",
                path: path.to_path_buf(),
                source,
            });
        }
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
    .map_err(|source| ProjectMutationError::Io {
        operation: "removing",
        path: path.to_path_buf(),
        source,
    })
}

/// Attempts to remove the journal directory when it is empty; silently
/// ignores errors so that unrelated journal contents are not affected.
fn cleanup_journal_if_empty(path: &Path) {
    let _ = std::fs::remove_dir(path);
}

/// Syncs journal directory entries on platforms that support directory fsync.
#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), ProjectMutationError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| ProjectMutationError::Io {
            operation: "syncing mutation journal directory",
            path: path.to_path_buf(),
            source,
        })
}

/// Directory fsync is not portable to Windows.
#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), ProjectMutationError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::Lockfile;
    use crate::project::ModuleProject;

    use super::ACTIVE_DIRECTORY;
    use super::ProjectTransaction;
    use super::super::{LockedModuleProject, ProjectUpdate, journal_root, project_key};

    fn test_project(root: &Path) -> ModuleProject {
        std::fs::create_dir_all(root).unwrap();
        std::fs::write(
            root.join(crate::MANIFEST_FILENAME),
            br#"{"name":"test","license":"MIT"}"#,
        )
        .unwrap();
        ModuleProject::load(root.join(crate::MANIFEST_FILENAME)).unwrap()
    }

    fn updated_document(project: &ModuleProject) -> crate::project::ManifestDocument {
        let mut document = project.document().clone();
        document
            .set_dependency(
                "dep",
                &serde_json::from_str(r#"{"path":"../dep"}"#).unwrap(),
            )
            .unwrap();
        document
    }

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt as _;

        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn recovers_interrupted_pair_mutation_from_global_journal() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        let state = directory.path().join("state");
        let project = test_project(&root);
        let original = std::fs::read(project.manifest_path()).unwrap();
        let journal = journal_root(&state, &project_key(project.root()).unwrap());

        {
            let _locked = LockedModuleProject::acquire(project.clone(), &state).unwrap();
            let _transaction = ProjectTransaction::begin(&project, &journal).unwrap();
            std::fs::write(project.manifest_path(), b"changed").unwrap();
            std::fs::write(project.lockfile_path(), b"changed").unwrap();
        }

        let recovered = LockedModuleProject::acquire(project, &state).unwrap();
        assert_eq!(
            std::fs::read(recovered.project().manifest_path()).unwrap(),
            original
        );
        assert!(!recovered.project().lockfile_path().exists());
        assert!(!journal.join(ACTIVE_DIRECTORY).exists());
    }

    #[test]
    fn second_write_failure_rolls_back_manifest() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        let state = directory.path().join("state");
        let project = test_project(&root)
            .with_test_lockfile_path(root.join("missing").join(crate::LOCKFILE_FILENAME));
        let original = std::fs::read(project.manifest_path()).unwrap();
        let document = updated_document(&project);

        let locked = LockedModuleProject::acquire(project, &state).unwrap();
        let error = locked
            .commit(ProjectUpdate::Both {
                manifest: &document,
                lockfile: &Lockfile::default(),
            })
            .unwrap_err();

        assert!(error.to_string().contains("temporary file"));
        assert_eq!(
            std::fs::read(locked.project().manifest_path()).unwrap(),
            original
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_manifest_atomically_gives_new_files_mode_0644() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(crate::MANIFEST_FILENAME);
        let document =
            crate::project::ManifestDocument::parse(br#"{"name":"test","license":"MIT"}"#)
                .unwrap();

        super::write_manifest_atomically(&path, &document).unwrap();

        assert_eq!(mode_of(&path), 0o644);
    }

    #[cfg(unix)]
    #[test]
    fn manifest_commit_preserves_existing_mode() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        let state = directory.path().join("state");
        let project = test_project(&root);
        let manifest_path = project.manifest_path().to_path_buf();
        std::fs::set_permissions(&manifest_path, std::fs::Permissions::from_mode(0o600))
            .unwrap();
        let document = updated_document(&project);

        let locked = LockedModuleProject::acquire(project, &state).unwrap();
        locked.commit(ProjectUpdate::Manifest(&document)).unwrap();

        assert_eq!(mode_of(&manifest_path), 0o600);
    }

    #[test]
    fn successful_commit_keeps_unrelated_sibling_journal_state() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        let state = directory.path().join("state");
        let project = test_project(&root);
        let journal = journal_root(&state, &project_key(project.root()).unwrap());
        let sibling = state.join("journals").join("sibling-project");
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(sibling.join("keep"), b"keep").unwrap();
        let document = updated_document(&project);

        let locked = LockedModuleProject::acquire(project, &state).unwrap();
        locked.commit(ProjectUpdate::Manifest(&document)).unwrap();

        assert_eq!(std::fs::read(sibling.join("keep")).unwrap(), b"keep");
        assert!(sibling.is_dir());
        assert!(!journal.exists());
    }

    #[test]
    fn rollback_failure_leaves_active_journal_for_manual_recovery() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        let state = directory.path().join("state");
        let project = test_project(&root)
            .with_test_lockfile_path(root.join("missing").join(crate::LOCKFILE_FILENAME));
        let journal = journal_root(&state, &project_key(project.root()).unwrap());
        let document = updated_document(&project);
        let locked = LockedModuleProject::acquire(project, &state).unwrap();
        let _guard = super::fail_manifest_restore_for_test(locked.project().manifest_path());

        let error = locked
            .commit(ProjectUpdate::Both {
                manifest: &document,
                lockfile: &Lockfile::default(),
            })
            .unwrap_err();

        assert!(matches!(error, super::super::ProjectMutationError::Rollback { .. }));
        assert!(journal.join(ACTIVE_DIRECTORY).is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_active_journal() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        let state = directory.path().join("state");
        let project = test_project(&root);
        let journal = journal_root(&state, &project_key(project.root()).unwrap());
        let outside = directory.path().join("outside");
        std::fs::create_dir_all(&journal).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, journal.join(ACTIVE_DIRECTORY)).unwrap();

        let error = LockedModuleProject::acquire(project, &state)
            .expect_err("a symlinked active journal should fail");

        assert!(error.to_string().contains("is not a regular"));
        assert!(outside.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_journal_root() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        let state = directory.path().join("state");
        let project = test_project(&root);
        let outside = directory.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        let journals_dir = state.join("journals");
        std::fs::create_dir_all(&journals_dir).unwrap();
        std::os::unix::fs::symlink(
            &outside,
            journals_dir.join(project_key(project.root()).unwrap()),
        )
        .unwrap();

        let error = LockedModuleProject::acquire(project, &state)
            .expect_err("a symlinked journal root should fail");

        assert!(
            error.to_string().contains("is not a regular directory"),
            "unexpected error: {error}"
        );
        assert_eq!(
            std::fs::read_dir(&outside).unwrap().count(),
            0,
            "outside directory should be untouched"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_recovery_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        let state = directory.path().join("state");
        let project = test_project(&root);
        let journal = journal_root(&state, &project_key(project.root()).unwrap());
        let active = journal.join(ACTIVE_DIRECTORY);
        std::fs::create_dir_all(&active).unwrap();
        std::os::unix::fs::symlink(project.manifest_path(), active.join("manifest.before"))
            .unwrap();
        std::fs::write(active.join("lockfile.absent"), b"").unwrap();

        let error = LockedModuleProject::acquire(project, &state)
            .expect_err("a symlinked recovery snapshot should fail");

        assert!(error.to_string().contains("is not a regular file"));
    }
}
