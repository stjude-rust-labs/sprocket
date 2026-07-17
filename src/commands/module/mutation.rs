//! Serialized and recoverable module project mutations.

use std::fs::File;
use std::fs::OpenOptions;
use std::fs::TryLockError;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use wdl_modules::Lockfile;

use super::Project;

const STATE_DIRECTORY: &str = ".sprocket";
const LOCK_FILENAME: &str = "module-mutation.lock";
const ACTIVE_DIRECTORY: &str = "module-mutation";
const PENDING_DIRECTORY: &str = "module-mutation.pending";

/// An exclusive lock for mutations to one module project.
#[derive(Debug)]
pub(crate) struct ProjectMutation {
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

        remove_pending_directory(&state)?;
        recover_active_mutation(project, &state)?;
        Ok(Self { _lock: lock })
    }

    /// Atomically applies an optional manifest and lockfile update.
    pub(crate) fn commit(
        &self,
        project: &Project,
        manifest: Option<&serde_json::Value>,
        lockfile: Option<&Lockfile>,
    ) -> anyhow::Result<()> {
        if manifest.is_none() && lockfile.is_none() {
            return Ok(());
        }

        validate_updates(manifest, lockfile)?;
        let transaction = ProjectTransaction::begin(project)?;
        let result = (|| {
            if let Some(manifest) = manifest {
                super::write_manifest_value(&project.manifest_path, manifest)?;
            }
            if let Some(lockfile) = lockfile {
                super::write_lockfile(project, lockfile)?;
            }
            sync_project_files(&project.manifest_path, &project.lockfile_path)?;
            Ok(())
        })();

        match result {
            Ok(()) => transaction.finish(),
            Err(source) => {
                transaction
                    .rollback()
                    .context("rolling back the interrupted module project mutation")?;
                Err(source)
            }
        }
    }
}

/// On-disk snapshots used to recover a project mutation.
#[derive(Debug)]
struct ProjectTransaction {
    state_root: PathBuf,
    active: PathBuf,
    manifest_path: PathBuf,
    lockfile_path: PathBuf,
}

impl ProjectTransaction {
    /// Snapshots both project files before a mutation begins.
    fn begin(project: &Project) -> anyhow::Result<Self> {
        let root = state_directory(&project.root)?;
        let pending = root.join(PENDING_DIRECTORY);
        let active = root.join(ACTIVE_DIRECTORY);
        remove_path_if_present(&pending)?;
        std::fs::create_dir(&pending)
            .with_context(|| format!("creating mutation journal `{}`", pending.display()))?;
        snapshot_file(&pending, "manifest", &project.manifest_path)?;
        snapshot_file(&pending, "lockfile", &project.lockfile_path)?;
        sync_directory(&pending)?;
        std::fs::rename(&pending, &active).with_context(|| {
            format!("activating module mutation journal `{}`", active.display())
        })?;
        sync_directory(&root)?;
        Ok(Self {
            state_root: root,
            active,
            manifest_path: project.manifest_path.clone(),
            lockfile_path: project.lockfile_path.clone(),
        })
    }

    /// Commits the mutation by removing its recovery journal.
    fn finish(self) -> anyhow::Result<()> {
        remove_path_if_present(&self.active)?;
        sync_directory(&self.state_root)
    }

    /// Restores both project files from the recovery journal.
    fn rollback(self) -> anyhow::Result<()> {
        restore_snapshot(&self.active, "manifest", &self.manifest_path)?;
        restore_snapshot(&self.active, "lockfile", &self.lockfile_path)?;
        sync_project_files(&self.manifest_path, &self.lockfile_path)?;
        remove_path_if_present(&self.active)?;
        sync_directory(&self.state_root)
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

/// Validates both serialized outputs before creating a recovery journal.
fn validate_updates(
    manifest: Option<&serde_json::Value>,
    lockfile: Option<&Lockfile>,
) -> anyhow::Result<()> {
    if let Some(manifest) = manifest {
        super::parse_manifest_value(manifest)?;
    }
    if let Some(lockfile) = lockfile {
        let mut bytes = Vec::new();
        lockfile
            .write(&mut bytes)
            .context("serializing updated `module-lock.json`")?;
    }
    Ok(())
}

/// Saves one file or records that it did not exist.
fn snapshot_file(journal: &Path, label: &str, path: &Path) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
            anyhow::bail!("project path `{}` is not a regular file", path.display());
        }
        Ok(_) => {
            let bytes =
                std::fs::read(path).with_context(|| format!("reading `{}`", path.display()))?;
            let snapshot = journal.join(format!("{label}.before"));
            let mut snapshot_file = File::create(&snapshot)
                .with_context(|| format!("writing mutation snapshot `{}`", snapshot.display()))?;
            std::io::Write::write_all(&mut snapshot_file, &bytes)
                .with_context(|| format!("writing mutation snapshot `{}`", snapshot.display()))?;
            snapshot_file
                .sync_all()
                .with_context(|| format!("syncing mutation snapshot `{}`", snapshot.display()))?;
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            let marker = journal.join(format!("{label}.absent"));
            let marker_file = File::create(&marker)
                .with_context(|| format!("writing mutation marker `{}`", marker.display()))?;
            marker_file
                .sync_all()
                .with_context(|| format!("syncing mutation marker `{}`", marker.display()))?;
        }
        Err(source) => {
            return Err(source).with_context(|| format!("inspecting `{}`", path.display()));
        }
    }
    Ok(())
}

/// Restores one file from a complete journal snapshot.
fn restore_snapshot(journal: &Path, label: &str, path: &Path) -> anyhow::Result<()> {
    let snapshot = journal.join(format!("{label}.before"));
    let absent = journal.join(format!("{label}.absent"));
    match std::fs::symlink_metadata(&snapshot) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            let bytes = std::fs::read(&snapshot)
                .with_context(|| format!("reading mutation snapshot `{}`", snapshot.display()))?;
            write_bytes_atomically(path, &bytes)?;
            return Ok(());
        }
        Ok(_) => {
            anyhow::bail!(
                "mutation snapshot `{}` is not a regular file",
                snapshot.display()
            );
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(source)
                .with_context(|| format!("inspecting mutation snapshot `{}`", snapshot.display()));
        }
    }
    match std::fs::symlink_metadata(&absent) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(source).with_context(|| format!("removing `{}`", path.display()));
                }
            }
            return Ok(());
        }
        Ok(_) => {
            anyhow::bail!(
                "mutation marker `{}` is not a regular file",
                absent.display()
            );
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(source)
                .with_context(|| format!("inspecting mutation marker `{}`", absent.display()));
        }
    }
    anyhow::bail!(
        "module mutation journal `{}` has no `{label}` snapshot",
        journal.display()
    )
}

/// Restores an interrupted transaction left by another process.
fn recover_active_mutation(project: &Project, state: &Path) -> anyhow::Result<()> {
    let active = state.join(ACTIVE_DIRECTORY);
    match std::fs::symlink_metadata(&active) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            anyhow::bail!(
                "module mutation journal `{}` is not a regular directory",
                active.display()
            );
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(source)
                .with_context(|| format!("inspecting mutation journal `{}`", active.display()));
        }
    }
    tracing::warn!(
        journal = %active.display(),
        "recovering an interrupted module project mutation"
    );
    restore_snapshot(&active, "manifest", &project.manifest_path)?;
    restore_snapshot(&active, "lockfile", &project.lockfile_path)?;
    remove_path_if_present(&active)
}

/// Replaces a file with recovery bytes using an atomic rename.
fn write_bytes_atomically(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(directory)
        .with_context(|| format!("creating a temporary file in `{}`", directory.display()))?;
    std::io::Write::write_all(&mut temp, bytes)
        .with_context(|| format!("writing `{}`", temp.path().display()))?;
    super::align_temp_permissions(&temp, path)?;
    temp.persist(path)
        .with_context(|| format!("restoring `{}`", path.display()))?;
    Ok(())
}

/// Makes both project files and their directory entries durable.
fn sync_project_files(manifest_path: &Path, lockfile_path: &Path) -> anyhow::Result<()> {
    for path in [manifest_path, lockfile_path] {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                OpenOptions::new()
                    .write(true)
                    .open(path)
                    .and_then(|file| file.sync_all())
                    .with_context(|| format!("syncing `{}`", path.display()))?;
            }
            Ok(_) => {
                anyhow::bail!("project path `{}` is not a regular file", path.display());
            }
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(source).with_context(|| format!("inspecting `{}`", path.display()));
            }
        }
    }
    let directory = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    sync_directory(directory)
}

/// Removes a journal path without following symbolic links.
fn remove_path_if_present(path: &Path) -> anyhow::Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(source).with_context(|| format!("inspecting `{}`", path.display()));
        }
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
    .with_context(|| format!("removing `{}`", path.display()))
}

/// Removes an incomplete pending journal.
fn remove_pending_directory(state: &Path) -> anyhow::Result<()> {
    remove_path_if_present(&state.join(PENDING_DIRECTORY))
}

/// Syncs journal directory entries on platforms that support directory fsync.
#[cfg(unix)]
fn sync_directory(path: &Path) -> anyhow::Result<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .with_context(|| format!("syncing mutation journal directory `{}`", path.display()))
}

/// Directory fsync is not portable to Windows.
#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn recovers_interrupted_pair_mutation() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let manifest_path = directory.path().join(wdl_modules::MANIFEST_FILENAME);
        let lockfile_path = directory.path().join(wdl_modules::LOCKFILE_FILENAME);
        let original = br#"{
  "name": "consumer",
  "license": "MIT"
}
"#;
        std::fs::write(&manifest_path, original)?;
        let manifest = wdl_modules::Manifest::parse(original)?;
        let project = Project {
            manifest_path: manifest_path.clone(),
            root: directory.path().to_path_buf(),
            manifest: Arc::new(manifest),
            lockfile_path: lockfile_path.clone(),
        };

        {
            let _mutation = ProjectMutation::acquire(&project)?;
            let _interrupted = ProjectTransaction::begin(&project)?;
            std::fs::write(&manifest_path, b"changed")?;
            std::fs::write(&lockfile_path, b"changed")?;
        }

        let _recovered = ProjectMutation::acquire(&project)?;
        assert_eq!(std::fs::read(&manifest_path)?, original);
        assert!(!lockfile_path.exists());
        Ok(())
    }
}
