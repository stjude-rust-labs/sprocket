//! Cross-platform advisory file lock on a directory.

use std::fs;
use std::fs::File;
use std::fs::TryLockError;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use tracing::info;

/// The name of the lock file created within the locked directory.
const LOCK_FILE_NAME: &str = ".lock";

/// An exclusive advisory lock on a directory.
///
/// The lock is implemented by creating and holding an exclusive advisory lock
/// on a `.lock` file within the target directory. This ensures that multiple
/// processes operating on the same directory (e.g., concurrent `sprocket run`
/// invocations sharing an output directory) serialize their access to shared
/// resources like the database and directory structure.
///
/// The lock is released when the value is dropped.
pub struct FileSystemLock {
    /// The held file (the lock is released on drop).
    _file: File,

    /// The path to the lock file (retained for diagnostics).
    path: PathBuf,
}

impl FileSystemLock {
    /// Acquires an exclusive lock on the given directory.
    ///
    /// Creates the directory and the `.lock` file within it if they do not
    /// already exist. If another process holds the lock, this call blocks
    /// until the lock becomes available.
    pub fn acquire(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();

        fs::create_dir_all(dir).with_context(|| {
            format!("failed to create directory `{path}`", path = dir.display())
        })?;

        let path = dir.join(LOCK_FILE_NAME);
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .with_context(|| format!("failed to open lock file `{path}`", path = path.display()))?;

        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                info!("waiting to acquire lock on `{path}`", path = path.display());
                file.lock().with_context(|| {
                    format!("failed to acquire lock on `{path}`", path = path.display())
                })?;
            }
            Err(TryLockError::Error(e)) => {
                return Err(e).with_context(|| {
                    format!("failed to acquire lock on `{path}`", path = path.display())
                });
            }
        }

        Ok(Self { _file: file, path })
    }

    /// Returns the path to the lock file.
    #[cfg(test)]
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl std::fmt::Debug for FileSystemLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSystemLock")
            .field("path", &self.path)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn acquires_lock() {
        let dir = tempdir().unwrap();
        let lock = FileSystemLock::acquire(dir.path()).unwrap();
        assert!(lock.path().exists());
    }

    #[test]
    fn creates_directory_if_missing() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("a").join("b").join("c");
        let lock = FileSystemLock::acquire(&nested).unwrap();
        assert!(nested.exists());
        assert!(lock.path().exists());
    }

    #[test]
    fn lock_released_on_drop() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(LOCK_FILE_NAME);

        {
            let _lock = FileSystemLock::acquire(dir.path()).unwrap();
            // While held, another exclusive lock should block (we test
            // with try_lock instead).
            let file = File::open(&path).unwrap();
            assert!(matches!(file.try_lock(), Err(TryLockError::WouldBlock)));
        }

        // After drop, we should be able to acquire the lock.
        let file = File::open(&path).unwrap();
        file.try_lock()
            .expect("lock should be available after drop");
    }
}
