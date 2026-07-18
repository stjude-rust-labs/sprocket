//! Journal, snapshot, recovery, and durable-write mechanics for module
//! project mutations.

use std::fs::File;
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;

use super::Project;
use super::ProjectUpdate;
use super::state_directory;

const ACTIVE_DIRECTORY: &str = "module-mutation";
const PENDING_DIRECTORY: &str = "module-mutation.pending";

/// On-disk snapshots used to recover a project mutation.
#[derive(Debug)]
pub(super) struct ProjectTransaction {
    state_root: PathBuf,
    active: PathBuf,
    manifest_path: PathBuf,
    lockfile_path: PathBuf,
}

impl ProjectTransaction {
    /// Snapshots both project files before a mutation begins.
    pub(super) fn begin(project: &Project) -> anyhow::Result<Self> {
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
    pub(super) fn finish(self) -> anyhow::Result<()> {
        remove_path_if_present(&self.active)?;
        sync_directory(&self.state_root)
    }

    /// Restores both project files from the recovery journal.
    pub(super) fn rollback(self) -> anyhow::Result<()> {
        restore_snapshot(&self.active, "manifest", &self.manifest_path)?;
        restore_snapshot(&self.active, "lockfile", &self.lockfile_path)?;
        sync_project_files(&self.manifest_path, &self.lockfile_path)?;
        remove_path_if_present(&self.active)?;
        sync_directory(&self.state_root)
    }
}

/// Combines an original mutation failure with a failed rollback.
pub(super) fn rollback_error(
    source: anyhow::Error,
    rollback: anyhow::Error,
    manifest_path: &Path,
    lockfile_path: &Path,
) -> anyhow::Error {
    source.context(format!(
        "rolling back the interrupted module project mutation also failed; inspect manifest `{}` \
         and lockfile `{}` for manual recovery; {rollback:#}",
        manifest_path.display(),
        lockfile_path.display(),
    ))
}

/// Validates both serialized outputs before creating a recovery journal.
pub(super) fn validate_updates(update: ProjectUpdate<'_>) -> anyhow::Result<()> {
    if let Some(manifest) = update.manifest() {
        super::super::parse_manifest_value(manifest)?;
    }
    if let Some(lockfile) = update.lockfile() {
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
pub(super) fn recover_active_mutation(project: &Project, state: &Path) -> anyhow::Result<()> {
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
    super::super::align_temp_permissions(&temp, path)?;
    temp.persist(path)
        .with_context(|| format!("restoring `{}`", path.display()))?;
    Ok(())
}

/// Makes both project files and their directory entries durable.
pub(super) fn sync_project_files(manifest_path: &Path, lockfile_path: &Path) -> anyhow::Result<()> {
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
pub(super) fn remove_pending_directory(state: &Path) -> anyhow::Result<()> {
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

    use wdl_modules::Lockfile;

    use super::super::ProjectMutation;
    use super::super::STATE_DIRECTORY;
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

    #[test]
    fn rolls_back_manifest_when_lockfile_write_fails() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let lockfile_path = directory.path().join("missing").join("module-lock.json");
        let project = test_project(directory.path(), lockfile_path)?;
        let original_manifest = std::fs::read(&project.manifest_path)?;
        let manifest = serde_json::json!({"name": "updated", "license": "MIT"});
        let lockfile = Lockfile::default();
        let mutation = ProjectMutation::acquire(&project)?;

        let error = mutation
            .commit(
                &project,
                ProjectUpdate::Both {
                    manifest: &manifest,
                    lockfile: &lockfile,
                },
            )
            .expect_err("writing into a missing directory should fail");

        assert!(error.to_string().contains("temporary file"));
        assert_eq!(std::fs::read(&project.manifest_path)?, original_manifest);
        let state = directory.path().join(STATE_DIRECTORY);
        assert!(!state.join(PENDING_DIRECTORY).exists());
        assert!(!state.join(ACTIVE_DIRECTORY).exists());
        Ok(())
    }

    #[test]
    fn rollback_failure_preserves_original_mutation_failure() {
        let manifest_path = Path::new("/worktree/module.json");
        let lockfile_path = Path::new("/worktree/module-lock.json");
        let error = rollback_error(
            anyhow::anyhow!("writing `module-lock.json` failed"),
            anyhow::anyhow!("restoring `module.json` failed"),
            manifest_path,
            lockfile_path,
        );
        let rendered = format!("{error:#}");

        assert!(rendered.contains("writing `module-lock.json` failed"));
        assert!(rendered.contains("restoring `module.json` failed"));
        assert!(rendered.contains("/worktree/module.json"));
        assert!(rendered.contains("/worktree/module-lock.json"));
    }
}
