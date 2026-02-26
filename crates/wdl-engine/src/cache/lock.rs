//! Implementation of a file lock used by the call cache.

use std::fs;
use std::fs::File;
use std::fs::TryLockError;
use std::io;
use std::io::Read;
use std::io::Write;
use std::ops::Deref;
use std::ops::DerefMut;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use tokio::task::spawn_blocking;
use tracing::info;

/// Represents a locked file.
pub struct LockedFile(File);

impl LockedFile {
    /// Acquires a shared file lock for the given path.
    ///
    /// If `create` is `true`, the file is created if it does not exist.
    ///
    /// If `create` is `false` and the file does not exist, `Ok(None)` is
    /// returned.
    pub async fn acquire_shared(path: impl AsRef<Path>, create: bool) -> Result<Option<Self>> {
        let path = path.as_ref();
        let file = if create {
            // Create or open the file, but do not truncate it if it exists
            let mut options = fs::OpenOptions::new();
            options.create(true).write(true);
            options.open(path).with_context(|| {
                format!(
                    "failed to create call cache entry file `{path}`",
                    path = path.display()
                )
            })?
        } else {
            match fs::File::open(path)
                .map(Some)
                .or_else(|e| {
                    if e.kind() == io::ErrorKind::NotFound {
                        Ok(None)
                    } else {
                        Err(e)
                    }
                })
                .with_context(|| {
                    format!(
                        "failed to open call cache entry file `{path}`",
                        path = path.display()
                    )
                })? {
                Some(file) => file,
                None => return Ok(None),
            }
        };

        match file.try_lock_shared() {
            Ok(_) => Ok(Some(Self(file))),
            Err(TryLockError::WouldBlock) => {
                let path = path.to_path_buf();
                spawn_blocking(move || {
                    info!(
                        "waiting to acquire shared lock on cache entry file `{path}`",
                        path = path.display()
                    );

                    file.lock_shared().with_context(|| {
                        format!(
                            "failed to acquire shared lock on cache entry file `{path}`",
                            path = path.display()
                        )
                    })?;
                    Ok(Some(Self(file)))
                })
                .await
                .context("failed to join lock task")?
            }
            Err(TryLockError::Error(e)) => Err(e).with_context(|| {
                format!(
                    "failed to acquire shared lock on cache entry file `{path}`",
                    path = path.display()
                )
            }),
        }
    }

    /// Acquires an exclusive file lock for the given path.
    ///
    /// If the file does not exist, it is created.
    ///
    /// If the file exists, it is truncated once the lock is acquired.
    pub async fn acquire_exclusive_truncated(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        // Create or open the file, but do not truncate it if it exists before the lock
        // is acquired
        let mut options = fs::OpenOptions::new();
        options.create(true).write(true);
        let file = options.open(path).with_context(|| {
            format!(
                "failed to create call cache entry file `{path}`",
                path = path.display()
            )
        })?;

        let file = match file.try_lock() {
            Ok(_) => Ok(Self(file)),
            Err(TryLockError::WouldBlock) => {
                let path = path.to_path_buf();
                spawn_blocking(move || {
                    info!(
                        "waiting to acquire exclusive lock on cache entry file `{path}`",
                        path = path.display()
                    );

                    file.lock().with_context(|| {
                        format!(
                            "failed to acquire exclusive lock on cache entry file `{path}`",
                            path = path.display()
                        )
                    })?;
                    Ok(Self(file))
                })
                .await
                .context("failed to join lock task")?
            }
            Err(TryLockError::Error(e)) => Err(e).with_context(|| {
                format!(
                    "failed to acquire exclusive lock on cache entry file `{path}`",
                    path = path.display()
                )
            }),
        }?;

        file.set_len(0).with_context(|| {
            format!(
                "failed to truncate cache entry file `{path}`",
                path = path.display()
            )
        })?;

        Ok(file)
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
mod test {
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
    async fn acquire_shared() {
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
    async fn acquire_exclusive() {
        let file = NamedTempFile::new().unwrap();
        let _exclusive = LockedFile::acquire_exclusive_truncated(file.path())
            .await
            .unwrap();

        // Ensure we can't acquire a shared lock
        assert!(matches!(
            file.as_file().try_lock_shared(),
            Err(TryLockError::WouldBlock)
        ));
    }
}
