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
use super::manifest::write_lockfile;
use super::manifest::write_manifest_value;
use super::project::Project;

const STATE_DIRECTORY: &str = ".sprocket";
const LEGACY_LOCK_FILENAME: &str = "module-mutation.lock";

/// Chooses the root directory under which global module mutation locks are
/// stored, preferring the Sprocket configuration directory, then the
/// platform cache directory, then the system temporary directory.
fn select_mutation_lock_root(config: Option<&Path>, cache: Option<&Path>, temp: &Path) -> PathBuf {
    if let Some(config) = config {
        return config.join("locks").join("module-mutations");
    }
    if let Some(cache) = cache {
        return cache
            .join("sprocket")
            .join("locks")
            .join("module-mutations");
    }
    temp.join("sprocket").join("locks").join("module-mutations")
}

/// Returns the global module mutation lock root for the current platform.
fn mutation_lock_root() -> PathBuf {
    select_mutation_lock_root(
        crate::config::config_root().as_deref(),
        dirs::cache_dir().as_deref(),
        &std::env::temp_dir(),
    )
}

/// Derives the deterministic global lock path for a canonicalized project
/// root, so path aliases (symlinks, relative paths) share one lock.
fn mutation_lock_path(project_root: &Path, lock_root: &Path) -> anyhow::Result<PathBuf> {
    let canonical = project_root
        .canonicalize()
        .with_context(|| format!("canonicalizing module root `{}`", project_root.display()))?;
    let digest = blake3::hash(canonical.as_os_str().as_encoded_bytes());
    Ok(lock_root.join(format!("{}.lock", digest.to_hex())))
}

/// A non-empty update applied atomically to a module project.
///
/// The variants are exhaustive and each carries at least one payload, so an
/// empty update can never be constructed.
#[derive(Clone, Copy, Debug)]
pub(super) enum ProjectUpdate<'a> {
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
pub(super) struct LockedProject {
    /// Project reloaded after the mutation lock is acquired.
    project: Project,
    /// Exclusive mutation guard held for this value's lifetime.
    mutation: ProjectMutation,
}

impl LockedProject {
    /// Acquires the project lock, recovers interrupted work, and reloads the
    /// manifest under the lock.
    pub(super) fn acquire(mut project: Project) -> anyhow::Result<Self> {
        let mutation = ProjectMutation::acquire(&project)?;
        project.reload()?;
        Ok(Self { project, mutation })
    }

    /// Returns the refreshed project snapshot protected by this lock.
    pub(super) fn project(&self) -> &Project {
        &self.project
    }

    /// Atomically applies a non-empty manifest and/or lockfile update.
    pub(super) fn commit(&self, update: ProjectUpdate<'_>) -> anyhow::Result<()> {
        self.mutation.commit(&self.project, update)
    }
}

/// An exclusive lock for mutations to one module project.
#[derive(Debug)]
struct ProjectMutation {
    _lock: File,
}

impl ProjectMutation {
    /// Acquires the global project mutation lock and recovers an
    /// interrupted mutation, migrating any legacy project-local lock.
    pub(super) fn acquire(project: &Project) -> anyhow::Result<Self> {
        Self::acquire_in(project, &mutation_lock_root())
    }

    /// Acquires the project mutation lock under `lock_root` and recovers an
    /// interrupted mutation, migrating any legacy project-local lock.
    fn acquire_in(project: &Project, lock_root: &Path) -> anyhow::Result<Self> {
        let lock_path = mutation_lock_path(&project.root, lock_root)?;
        std::fs::create_dir_all(lock_root).with_context(|| {
            format!(
                "creating module mutation lock directory `{}`",
                lock_root.display()
            )
        })?;
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("opening project mutation lock `{}`", lock_path.display()))?;
        lock_file(&lock, &lock_path)?;

        if let Some(state) = existing_state_directory(&project.root)? {
            let legacy = acquire_legacy_lock(&state)?;
            transaction::remove_pending_directory(&state)?;
            transaction::recover_active_mutation(project, &state)?;
            if let Some((legacy, legacy_path)) = legacy {
                drop(legacy);
                remove_legacy_lock(&legacy_path);
            }
            cleanup_state_directory(&state);
        }

        Ok(Self { _lock: lock })
    }

    /// Atomically applies a non-empty manifest and/or lockfile update.
    pub(super) fn commit(
        &self,
        project: &Project,
        update: ProjectUpdate<'_>,
    ) -> anyhow::Result<()> {
        transaction::validate_updates(update)?;
        let transaction = ProjectTransaction::begin(project)?;
        let result = (|| {
            if let Some(manifest) = update.manifest() {
                write_manifest_value(&project.manifest_path, manifest)?;
            }
            if let Some(lockfile) = update.lockfile() {
                write_lockfile(project, lockfile)?;
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

/// Locks `file`, waiting for another process to release it if necessary.
fn lock_file(file: &File, path: &Path) -> anyhow::Result<()> {
    match file.try_lock() {
        Ok(()) => Ok(()),
        Err(TryLockError::WouldBlock) => {
            tracing::info!(
                lock = %path.display(),
                "waiting for another module command to finish"
            );
            file.lock()
                .with_context(|| format!("acquiring project mutation lock `{}`", path.display()))
        }
        Err(TryLockError::Error(source)) => Err(source)
            .with_context(|| format!("acquiring project mutation lock `{}`", path.display())),
    }
}

/// Returns the project's private state directory if it already exists,
/// failing if the path exists but is not a regular directory.
fn existing_state_directory(root: &Path) -> anyhow::Result<Option<PathBuf>> {
    let state = root.join(STATE_DIRECTORY);
    match std::fs::symlink_metadata(&state) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            anyhow::bail!(
                "module state path `{}` is not a regular directory",
                state.display()
            );
        }
        Ok(_) => Ok(Some(state)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(source)
            .with_context(|| format!("inspecting module state path `{}`", state.display())),
    }
}

/// Returns the project's private state directory, creating it if it does
/// not already exist.
fn create_state_directory(root: &Path) -> anyhow::Result<PathBuf> {
    if let Some(state) = existing_state_directory(root)? {
        return Ok(state);
    }
    let state = root.join(STATE_DIRECTORY);
    std::fs::create_dir(&state)
        .with_context(|| format!("creating module state directory `{}`", state.display()))?;
    Ok(state)
}

/// Acquires the legacy project-local mutation lock if one is present,
/// without following a symbolic link.
fn acquire_legacy_lock(state: &Path) -> anyhow::Result<Option<(File, PathBuf)>> {
    let path = state.join(LEGACY_LOCK_FILENAME);
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            anyhow::bail!(
                "legacy module mutation lock `{}` is not a regular file",
                path.display()
            );
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(source)
                .with_context(|| format!("inspecting legacy mutation lock `{}`", path.display()));
        }
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("opening legacy mutation lock `{}`", path.display()))?;
    lock_file(&file, &path)?;
    Ok(Some((file, path)))
}

/// Removes the legacy project-local mutation lock after it has been
/// migrated, logging a warning if removal fails.
fn remove_legacy_lock(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            tracing::warn!(
                path = %path.display(),
                error = %source,
                "failed to remove legacy module mutation lock"
            );
        }
    }
}

/// Removes the project's private state directory once it is empty,
/// warning rather than failing if removal is not possible.
fn cleanup_state_directory(state: &Path) {
    cleanup_state_directory_with(state, |state| std::fs::remove_dir(state));
}

/// Removes `state` with an injectable removal function so the warning
/// branch can be tested without platform-specific permission tricks.
fn cleanup_state_directory_with(state: &Path, remove: impl FnOnce(&Path) -> std::io::Result<()>) {
    match remove(state) {
        Ok(()) => {}
        Err(source)
            if matches!(
                source.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            ) => {}
        Err(source) => {
            tracing::warn!(
                path = %state.display(),
                error = %source,
                "failed to remove empty module state directory"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tracing_test::traced_test;

    use super::super::manifest::read_manifest_value;
    use super::*;

    /// Builds a minimal project rooted in a test directory.
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
        let mutation =
            ProjectMutation::acquire_in(&project, &directory.path().join("global-locks"))?;

        let manifest = serde_json::json!({"name": "updated", "license": "MIT"});
        let lockfile = Lockfile::default();
        mutation.commit(
            &project,
            ProjectUpdate::Both {
                manifest: &manifest,
                lockfile: &lockfile,
            },
        )?;

        assert_eq!(read_manifest_value(&project.manifest_path)?, manifest);
        assert!(lockfile_path.is_file());
        assert!(!directory.path().join(STATE_DIRECTORY).exists());
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

        let error = ProjectMutation::acquire_in(&project, &directory.path().join("global-locks"))
            .expect_err("a non-directory transaction state should fail");

        assert!(error.to_string().contains("is not a regular directory"));
        Ok(())
    }

    #[test]
    fn selects_mutation_lock_root_by_available_base() {
        let config = Path::new("/config");
        let cache = Path::new("/cache");
        let temp = Path::new("/tmp");

        assert_eq!(
            select_mutation_lock_root(Some(config), Some(cache), temp),
            config.join("locks").join("module-mutations")
        );
        assert_eq!(
            select_mutation_lock_root(None, Some(cache), temp),
            cache
                .join("sprocket")
                .join("locks")
                .join("module-mutations")
        );
        assert_eq!(
            select_mutation_lock_root(None, None, temp),
            temp.join("sprocket").join("locks").join("module-mutations")
        );
    }

    #[test]
    fn lock_only_acquisition_creates_no_project_state() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let project_root = directory.path().join("project");
        std::fs::create_dir(&project_root)?;
        let project = test_project(
            &project_root,
            project_root.join(wdl_modules::LOCKFILE_FILENAME),
        )?;
        let mutation = ProjectMutation::acquire_in(&project, &directory.path().join("locks"))?;

        assert!(!project_root.join(STATE_DIRECTORY).exists());
        drop(mutation);
        Ok(())
    }

    #[test]
    fn global_lock_serializes_concurrent_acquirers() -> anyhow::Result<()> {
        use std::sync::mpsc;
        use std::time::Duration;

        let directory = tempfile::tempdir()?;
        let project_root = directory.path().join("project");
        std::fs::create_dir(&project_root)?;
        let project = test_project(
            &project_root,
            project_root.join(wdl_modules::LOCKFILE_FILENAME),
        )?;
        let lock_root = directory.path().join("locks");
        let first = ProjectMutation::acquire_in(&project, &lock_root)?;
        let (sender, receiver) = mpsc::channel();
        let thread = std::thread::spawn({
            let project = project.clone();
            let lock_root = lock_root.clone();
            move || {
                let second = ProjectMutation::acquire_in(&project, &lock_root);
                sender.send(second).expect("receiver should remain open");
            }
        });

        assert!(receiver.recv_timeout(Duration::from_millis(100)).is_err());
        drop(first);
        let second = receiver.recv_timeout(Duration::from_secs(5))??;
        drop(second);
        thread.join().expect("lock thread should not panic");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn path_aliases_share_the_same_global_lock() -> anyhow::Result<()> {
        use std::sync::mpsc;
        use std::time::Duration;

        let directory = tempfile::tempdir()?;
        let project_root = directory.path().join("project");
        let alias_root = directory.path().join("project-alias");
        std::fs::create_dir(&project_root)?;
        std::os::unix::fs::symlink(&project_root, &alias_root)?;
        let project = test_project(
            &project_root,
            project_root.join(wdl_modules::LOCKFILE_FILENAME),
        )?;
        let alias = Project {
            manifest_path: alias_root.join(wdl_modules::MANIFEST_FILENAME),
            root: alias_root,
            manifest: project.manifest.clone(),
            lockfile_path: project.lockfile_path.clone(),
        };
        let lock_root = directory.path().join("locks");

        assert_eq!(
            mutation_lock_path(&project.root, &lock_root)?,
            mutation_lock_path(&alias.root, &lock_root)?
        );
        let first = ProjectMutation::acquire_in(&project, &lock_root)?;
        let (sender, receiver) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let second = ProjectMutation::acquire_in(&alias, &lock_root);
            sender.send(second).expect("receiver should remain open");
        });

        assert!(receiver.recv_timeout(Duration::from_millis(100)).is_err());
        drop(first);
        let second = receiver.recv_timeout(Duration::from_secs(5))??;
        drop(second);
        thread.join().expect("lock thread should not panic");
        Ok(())
    }

    #[test]
    fn migrates_legacy_local_lock_without_a_journal() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let project_root = directory.path().join("project");
        let state = project_root.join(STATE_DIRECTORY);
        std::fs::create_dir_all(&state)?;
        std::fs::write(state.join(LEGACY_LOCK_FILENAME), b"")?;
        let project = test_project(
            &project_root,
            project_root.join(wdl_modules::LOCKFILE_FILENAME),
        )?;

        let mutation = ProjectMutation::acquire_in(&project, &directory.path().join("locks"))?;

        assert!(!state.exists());
        drop(mutation);
        Ok(())
    }

    #[traced_test]
    #[test]
    fn state_cleanup_failure_warns_without_returning_an_error() {
        cleanup_state_directory_with(Path::new("/project/.sprocket"), |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "permission denied",
            ))
        });

        assert!(logs_contain(
            "failed to remove empty module state directory"
        ));
    }
}
