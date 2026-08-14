//! Advisory locking for `module-lock.json`.
//!
//! `module-lock.json` carries the lock that coordinates its readers and
//! writers. [`LockedLockfile::read`] opens an existing file under a shared lock
//! and never creates it. [`LockedLockfile::acquire`] opens or creates the file
//! under an exclusive lock so the caller can inspect its current contents and
//! decide whether to replace them. A replacement is written in place through
//! the held handle rather than through a rename, because a rename would install
//! a new inode and leave the lock guarding the old one.

use std::fs::File;
use std::fs::OpenOptions;
use std::fs::TryLockError;
use std::io::Read as _;
use std::io::Seek as _;
use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;

use super::ProjectError;
use crate::Lockfile;

/// A `module-lock.json` held under its exclusive advisory lock.
///
/// The lock is released when this value is dropped, or when
/// [`Self::write`] consumes it.
#[derive(Debug)]
pub struct LockedLockfile {
    /// The locked lockfile path.
    path: PathBuf,
    /// Open handle that holds the lock and receives the write.
    file: File,
}

impl LockedLockfile {
    /// Reads and parses the `module-lock.json` at `path` under a shared
    /// advisory lock.
    ///
    /// Returns `Ok(None)` when the file is absent or empty. The file is never
    /// created, so reading cannot make a project appear to have a lockfile.
    pub fn read(path: &Path) -> Result<Option<Lockfile>, ProjectError> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(ProjectError::Io {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        wait_for_lock(path, || file.try_lock_shared(), || file.lock_shared())?;
        parse(&file, path)
    }

    /// Acquires the exclusive advisory lock on the `module-lock.json` at
    /// `path`, creating the file when it is absent.
    ///
    /// This blocks until any other holder releases the lock.
    pub fn acquire(path: &Path) -> Result<Self, ProjectError> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .map_err(|source| ProjectError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        wait_for_lock(path, || file.try_lock(), || file.lock())?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
        })
    }

    /// Returns the lockfile as it stands on disk under the held lock.
    ///
    /// This is `None` when the file is empty, which is how a lockfile created
    /// solely to be locked reads.
    pub fn current(&self) -> Result<Option<Lockfile>, ProjectError> {
        parse(&self.file, &self.path)
    }

    /// Replaces the lockfile contents with `lockfile` and releases the lock.
    ///
    /// The lockfile is serialized into memory first, so a serialization failure
    /// never reaches the file.
    pub fn write(self, lockfile: &Lockfile) -> Result<(), ProjectError> {
        let mut bytes = Vec::new();
        lockfile
            .write(&mut bytes)
            .map_err(|source| ProjectError::Io {
                path: self.path.clone(),
                source,
            })?;
        let mut file = &self.file;
        file.rewind().map_err(|source| ProjectError::Io {
            path: self.path.clone(),
            source,
        })?;
        file.set_len(0).map_err(|source| ProjectError::Io {
            path: self.path.clone(),
            source,
        })?;
        file.write_all(&bytes).map_err(|source| ProjectError::Io {
            path: self.path.clone(),
            source,
        })
    }
}

/// Takes a lock, logging once when the lock is contended.
fn wait_for_lock(
    path: &Path,
    try_lock: impl FnOnce() -> Result<(), TryLockError>,
    lock: impl FnOnce() -> std::io::Result<()>,
) -> Result<(), ProjectError> {
    match try_lock() {
        Ok(()) => Ok(()),
        Err(TryLockError::WouldBlock) => {
            #[cfg(feature = "git-resolver")]
            tracing::info!(
                lockfile = %path.display(),
                "waiting to acquire the module lockfile lock"
            );
            lock().map_err(|source| ProjectError::Io {
                path: path.to_path_buf(),
                source,
            })
        }
        Err(TryLockError::Error(source)) => Err(ProjectError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Reads and parses a locked lockfile handle from its start.
fn parse(file: &File, path: &Path) -> Result<Option<Lockfile>, ProjectError> {
    let mut handle = file;
    handle.rewind().map_err(|source| ProjectError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut bytes = Vec::new();
    handle
        .read_to_end(&mut bytes)
        .map_err(|source| ProjectError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.is_empty() {
        return Ok(None);
    }
    Lockfile::parse(&bytes)
        .map(Some)
        .map_err(|source| ProjectError::Lockfile {
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    /// A minimal valid `module-lock.json`.
    const LOCKFILE: &[u8] = br#"{"version":1,"dependencies":{}}"#;

    /// Any error a test can propagate.
    type Result = std::result::Result<(), Box<dyn std::error::Error>>;

    /// Returns the lockfile path inside `root`.
    fn lockfile_path(root: &Path) -> std::path::PathBuf {
        root.join(crate::LOCKFILE_FILENAME)
    }

    #[test]
    fn read_reports_an_absent_lockfile_as_none() -> Result {
        let directory = tempfile::tempdir()?;
        let path = lockfile_path(directory.path());

        assert!(LockedLockfile::read(&path)?.is_none());
        assert!(
            !path.exists(),
            "reading must never create `module-lock.json`"
        );
        Ok(())
    }

    #[test]
    fn read_parses_a_present_lockfile() -> Result {
        let directory = tempfile::tempdir()?;
        let path = lockfile_path(directory.path());
        std::fs::write(&path, LOCKFILE)?;

        assert_eq!(
            LockedLockfile::read(&path)?.map(|lockfile| lockfile.version),
            Some(crate::lockfile::LOCKFILE_VERSION)
        );
        Ok(())
    }

    #[test]
    fn read_reports_an_empty_lockfile_as_none() -> Result {
        let directory = tempfile::tempdir()?;
        let path = lockfile_path(directory.path());
        std::fs::write(&path, b"")?;

        assert!(LockedLockfile::read(&path)?.is_none());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn write_keeps_the_locked_inode() -> Result {
        use std::os::unix::fs::MetadataExt as _;

        let directory = tempfile::tempdir()?;
        let path = lockfile_path(directory.path());
        std::fs::write(&path, LOCKFILE)?;
        let before = std::fs::metadata(&path)?.ino();

        LockedLockfile::acquire(&path)?.write(&Lockfile::default())?;

        assert_eq!(
            std::fs::metadata(&path)?.ino(),
            before,
            "writing through the held handle must not replace the inode"
        );
        Ok(())
    }

    #[test]
    fn write_replaces_longer_previous_contents() -> Result {
        let directory = tempfile::tempdir()?;
        let path = lockfile_path(directory.path());
        std::fs::write(&path, [LOCKFILE, b"                    "].concat())?;

        LockedLockfile::acquire(&path)?.write(&Lockfile::default())?;

        assert_eq!(
            LockedLockfile::read(&path)?.map(|lockfile| lockfile.version),
            Some(crate::lockfile::LOCKFILE_VERSION)
        );
        Ok(())
    }

    #[test]
    fn acquire_serializes_concurrent_writers() -> Result {
        let directory = tempfile::tempdir()?;
        let path = lockfile_path(directory.path());
        let first = LockedLockfile::acquire(&path)?;
        let (sender, receiver) = mpsc::channel();
        let thread = std::thread::spawn({
            let path = path.clone();
            move || {
                // SAFETY: the receiver lives until this thread is joined, so
                // the channel cannot be disconnected before the send.
                sender.send(LockedLockfile::acquire(&path).is_ok()).unwrap();
            }
        });

        assert!(receiver.recv_timeout(Duration::from_millis(100)).is_err());
        drop(first);
        assert!(receiver.recv_timeout(Duration::from_secs(5))?);
        // SAFETY: the spawned closure only sends on a channel, so it cannot
        // panic and the join cannot observe a panicked thread.
        thread.join().unwrap();
        Ok(())
    }

    #[test]
    fn current_sees_what_is_on_disk_under_the_lock() -> Result {
        let directory = tempfile::tempdir()?;
        let path = lockfile_path(directory.path());
        std::fs::write(&path, LOCKFILE)?;

        let guard = LockedLockfile::acquire(&path)?;

        assert_eq!(
            guard.current()?.map(|lockfile| lockfile.version),
            Some(crate::lockfile::LOCKFILE_VERSION)
        );
        Ok(())
    }
}
