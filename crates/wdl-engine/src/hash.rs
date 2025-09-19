//! Utility functions for cryptographically hashing files and directories.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;

use anyhow::Context;
use anyhow::Result;
use blake3::Hash;
use blake3::Hasher;
use tokio::sync::OnceCell;
use tokio::task::spawn_blocking;
use url::Url;
use walkdir::WalkDir;

/// Represents a calculated [Blake3](https://github.com/BLAKE3-team/BLAKE3) digest of a file or directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Digest {
    /// The digest is for a file.
    File(Hash),
    /// The digest is for a directory.
    Directory(Hash),
}

/// Keeps track of previously calculated digests.
///
/// As WDL evaluation cannot write to existing files, it is assumed that files
/// and directories are not modified during evaluation.
///
/// We check for changes to files and directories when we get a cache hit and
/// error if the source has been modified.
static DIGESTS: LazyLock<Mutex<HashMap<PathBuf, Arc<OnceCell<Digest>>>>> =
    LazyLock::new(Mutex::default);

/// An extension trait for joining a digest to a URL.
pub trait UrlDigestExt: Sized {
    /// Joins the given digest to the URL.
    ///
    /// If the digest is for a file, a `file` path segment is pushed first.
    ///
    /// If the digest is for a directory, a `directory` path segment is pushed
    /// first.
    ///
    /// A path segment is then pushed for the digest as a hex string.
    fn join_digest(&self, digest: Digest) -> Self;
}

impl UrlDigestExt for Url {
    fn join_digest(&self, digest: Digest) -> Self {
        assert!(
            !self.cannot_be_a_base(),
            "invalid URL: URL is required to be a base"
        );

        let mut url = self.clone();

        {
            // SAFETY: this will always return `Ok` if the above assert passed
            let mut segments = url.path_segments_mut().unwrap();
            segments.pop_if_empty();

            let digest = match digest {
                Digest::File(digest) => {
                    segments.push("file");
                    digest
                }
                Digest::Directory(digest) => {
                    segments.push("directory");
                    digest
                }
            };

            let hex = digest.to_hex();
            segments.push(hex.as_str());
        }

        url
    }
}

/// Calculates the digest of a path.
///
/// If the path is a single file, a [blake3](blake3) digest is calculated for
/// the file.
///
/// If the path is a directory, a consistent, recursive walk of the directory is
/// performed and a digest of each directory entry is calculated.
///
/// A directory entry's digest consists of:
///
/// * The relative path to the entry.
/// * Whether or not the entry is a file or a directory.
/// * If the entry is a file, the [blake3](blake3) digest of the file's
///   contents.
/// * The total number of entries in the directory.
///
/// [blake3]: https://github.com/BLAKE3-team/BLAKE3
pub async fn calculate_path_digest(path: impl AsRef<Path>) -> Result<Digest> {
    let path = path.as_ref();

    let digest = {
        let mut digests = DIGESTS.lock().expect("failed to lock digests");
        digests.entry(path.to_path_buf()).or_default().clone()
    };

    // Get an existing result or initialize a new one exactly once
    Ok(*digest
        .get_or_try_init(|| async {
            let path = path.to_path_buf();
            spawn_blocking(move || {
                let metadata = path.metadata().with_context(|| {
                    format!(
                        "failed to read metadata for path `{path}`",
                        path = path.display()
                    )
                })?;

                // If the path is a file, hash just the file.
                if metadata.is_file() {
                    let mut hasher = Hasher::new();
                    hasher.update_mmap(&path).with_context(|| {
                        format!(
                            "failed to calculate digest of file `{path}`",
                            path = path.display()
                        )
                    })?;
                    return anyhow::Ok(Digest::File(hasher.finalize()));
                }

                // Otherwise, walk the directory and calculate a directory digest.
                let mut entries: usize = 0;
                let mut hasher = Hasher::new();
                for entry in WalkDir::new(&path).sort_by_file_name() {
                    let entry = entry.with_context(|| {
                        format!(
                            "failed to walk directory contents of `{path}`",
                            path = path.display()
                        )
                    })?;

                    let entry_path = entry.path();
                    let metadata = entry.metadata().with_context(|| {
                        format!(
                            "failed to read metadata for path `{path}`",
                            path = entry_path.display()
                        )
                    })?;

                    // Hash the relative path to the entry
                    hasher.update(
                        entry_path
                            .strip_prefix(&path)
                            .unwrap_or(entry_path)
                            .to_str()
                            .with_context(|| {
                                format!("path `{path}` is not UTF-8", path = entry_path.display())
                            })?
                            .as_bytes(),
                    );

                    // If entry is a file, hash its contents
                    if metadata.is_file() {
                        hasher
                            .update(&[1])
                            .update_mmap(entry_path)
                            .with_context(|| {
                                format!(
                                    "failed to calculate digest of file `{path}`",
                                    path = entry_path.display()
                                )
                            })?;
                    } else {
                        hasher.update(&[0]);
                    }

                    entries += 1;
                }

                hasher.update(&entries.to_le_bytes());
                Ok(Digest::Directory(hasher.finalize()))
            })
            .await
            .expect("digest task failed")
        })
        .await?)
}
