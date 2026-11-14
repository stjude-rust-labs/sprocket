//! Implementation of advisory locks using files.

use std::fmt;
use std::fs;
use std::fs::File;
use std::fs::TryLockError;
use std::io;
use std::io::Read;
use std::io::Write;
use std::ops::Deref;
use std::ops::DerefMut;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use tokio::task::spawn_blocking;
use tracing::info;

use crate::slow::warn_if_slow;

/// The initial delay between attempts to acquire a contended lock.
const INITIAL_LOCK_RETRY_DELAY: Duration = Duration::from_millis(10);

/// The maximum delay between attempts to acquire a contended lock.
const MAX_LOCK_RETRY_DELAY: Duration = Duration::from_secs(1);

/// Represents the kind of lock to acquire.
#[derive(Debug, Clone, Copy)]
enum LockKind {
    /// A shared lock.
    Shared,
    /// An exclusive lock.
    Exclusive,
}

impl fmt::Display for LockKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shared => write!(f, "shared"),
            Self::Exclusive => write!(f, "exclusive"),
        }
    }
}

/// Represents a locked file.
///
/// All file system operations performed while acquiring the lock are run on
/// the blocking thread pool so that an unresponsive file system (e.g., a
/// network file system such as CephFS) cannot stall the async runtime.
///
/// Contended locks are acquired by polling rather than by blocking a thread
/// for the duration of the wait; this prevents many waiters from exhausting
/// the blocking thread pool.
pub struct LockedFile(File);

impl LockedFile {
    /// Acquires a shared file lock for the given path.
    ///
    /// If `create` is `true`, the file is created if it does not exist.
    ///
    /// If `create` is `false` and the file does not exist, `Ok(None)` is
    /// returned.
    pub async fn acquire_shared(path: impl AsRef<Path>, create: bool) -> Result<Option<Self>> {
        let path = path.as_ref().to_path_buf();
        let file = warn_if_slow(
            {
                let path = path.clone();
                async move {
                    spawn_blocking(move || Self::open_shared(&path, create))
                        .await
                        .context("failed to join lock task")?
                }
            },
            || format!("opening file `{path}`", path = path.display()),
        )
        .await?;

        let Some(file) = file else {
            return Ok(None);
        };

        Self::lock(file, path, LockKind::Shared).await.map(Some)
    }

    /// Acquires an exclusive file lock for the given path.
    ///
    /// If the file does not exist, it is created.
    pub async fn acquire_exclusive(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = warn_if_slow(
            {
                let path = path.clone();
                async move {
                    spawn_blocking(move || {
                        // Create or open the file, but do not truncate it if it
                        // exists before the lock is acquired
                        let mut options = fs::OpenOptions::new();
                        options.create(true).write(true);
                        options.open(&path).with_context(|| {
                            format!("failed to create file `{path}`", path = path.display())
                        })
                    })
                    .await
                    .context("failed to join lock task")?
                }
            },
            || format!("opening file `{path}`", path = path.display()),
        )
        .await?;

        Self::lock(file, path, LockKind::Exclusive).await
    }

    /// Opens a file for a shared lock.
    ///
    /// This performs blocking file system operations.
    fn open_shared(path: &Path, create: bool) -> Result<Option<File>> {
        match fs::File::open(path) {
            Ok(file) => Ok(Some(file)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if !create {
                    return Ok(None);
                }

                // Create the file, which requires writable access
                let mut options = fs::OpenOptions::new();
                options.create(true).write(true);
                options.open(path).with_context(|| {
                    format!("failed to create file `{path}`", path = path.display())
                })?;

                // Re-open the file as readable as the lock is shared and we
                // don't want the file to be writable
                fs::File::open(path)
                    .map(Some)
                    .with_context(|| format!("failed to open file `{path}`", path = path.display()))
            }
            Err(e) => Err(e)
                .with_context(|| format!("failed to open file `{path}`", path = path.display())),
        }
    }

    /// Locks the given file, waiting until the lock can be acquired.
    async fn lock(file: File, path: PathBuf, kind: LockKind) -> Result<Self> {
        let acquire = async {
            let mut file = Some(file);
            let mut delay = INITIAL_LOCK_RETRY_DELAY;
            let mut waiting = false;

            loop {
                let f = file.take().expect("file should be present");
                let (f, result) = spawn_blocking(move || {
                    let result = match kind {
                        LockKind::Shared => f.try_lock_shared(),
                        LockKind::Exclusive => f.try_lock(),
                    };
                    (f, result)
                })
                .await
                .context("failed to join lock task")?;

                match result {
                    Ok(()) => return Ok(Self(f)),
                    Err(TryLockError::WouldBlock) => {
                        if !waiting {
                            info!(
                                "waiting to acquire {kind} lock on file `{path}`",
                                path = path.display()
                            );
                            waiting = true;
                        }

                        file = Some(f);
                        tokio::time::sleep(delay).await;
                        delay = (delay * 2).min(MAX_LOCK_RETRY_DELAY);
                    }
                    Err(TryLockError::Error(e)) => {
                        return Err(e).with_context(|| {
                            format!(
                                "failed to acquire {kind} lock on file `{path}`",
                                path = path.display()
                            )
                        });
                    }
                }
            }
        };

        warn_if_slow(acquire, || {
            format!(
                "acquiring {kind} lock on file `{path}`",
                path = path.display()
            )
        })
        .await
    }
}

impl Deref for LockedFile {
    type Target = File;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for LockedFile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Read for LockedFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for LockedFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

#[cfg(test)]
mod tests {
    use tempfile::NamedTempFile;

    use super::*;

    #[tokio::test]
    async fn acquire_shared_no_create() {
        assert!(
            LockedFile::acquire_shared("does-not-exist", false)
                .await
                .unwrap()
                .is_none(),
            "should not file"
        );
    }

    #[tokio::test]
    async fn acquire_shared_existing_file() {
        let file = NamedTempFile::new().unwrap();
        let _first = LockedFile::acquire_shared(file.path(), true)
            .await
            .unwrap()
            .expect("should have locked file");
        let _second = LockedFile::acquire_shared(file.path(), true)
            .await
            .unwrap()
            .expect("should have locked file");
    }

    #[tokio::test]
    async fn acquire_shared_creates_file_returns_readable_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("needs_to_be_created");

        let mut file = LockedFile::acquire_shared(&path, true)
            .await
            .unwrap()
            .expect("should create and lock file");

        assert!(path.is_file());

        let mut buf = String::new();
        file.read_to_string(&mut buf)
            .expect("shared lock file should be readable");
    }

    #[tokio::test]
    async fn acquire_exclusive_waits_for_release() {
        let file = NamedTempFile::new().unwrap();
        let first = LockedFile::acquire_exclusive(file.path()).await.unwrap();

        let path = file.path().to_path_buf();
        let second = tokio::spawn(async move { LockedFile::acquire_exclusive(path).await });

        // The second lock should not be acquired while the first is held
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!second.is_finished());

        drop(first);
        let second = tokio::time::timeout(Duration::from_secs(10), second)
            .await
            .expect("lock should be acquired after release")
            .unwrap()
            .unwrap();

        // Ensure we can't acquire a shared lock while the second is held
        assert!(matches!(
            file.as_file().try_lock_shared(),
            Err(TryLockError::WouldBlock)
        ));
        drop(second);
    }

    #[tokio::test]
    async fn acquire_exclusive() {
        let file = NamedTempFile::new().unwrap();
        let _exclusive = LockedFile::acquire_exclusive(file.path()).await.unwrap();

        // Ensure we can't acquire a shared lock
        assert!(matches!(
            file.as_file().try_lock_shared(),
            Err(TryLockError::WouldBlock)
        ));
    }
}
