//! Implements calculation of file and directory content digests.
//!
//! This is used by the call cache and for uploading inputs for remote backends.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use arrayvec::ArrayString;
use blake3::Hash;
use blake3::Hasher;
use cloud_copy::ContentDigest;
use cloud_copy::UrlExt;
use tokio::sync::OnceCell;
use tokio::task::spawn_blocking;
use url::Url;
use walkdir::WalkDir;

use crate::ContentKind;
use crate::cache::Hashable;
use crate::http::Transferer;
use crate::path::EvaluationPath;

/// The variant tag for files.
const FILE_VARIANT_TAG: u8 = 0;
/// The variant tag for directories.
const DIRECTORY_VARIANT_TAG: u8 = 1;

/// Represents a calculated [Blake3](https://github.com/BLAKE3-team/BLAKE3) digest of a file or directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Digest {
    /// The digest is for a file.
    File(Hash),
    /// The digest is for a directory.
    Directory(Hash),
}

impl Digest {
    /// Converts the digest to a hex string.
    pub fn to_hex(self) -> ArrayString<64> {
        match self {
            Self::File(hash) => hash.to_hex(),
            Self::Directory(hash) => hash.to_hex(),
        }
    }
}

/// Keeps track of previously calculated digests.
///
/// As WDL evaluation cannot write to existing files, it is assumed that files
/// and directories are not modified during evaluation.
///
/// We check for changes to files and directories when we get a cache hit and
/// error if the source has been modified.
static DIGESTS: LazyLock<Mutex<HashMap<EvaluationPath, Arc<OnceCell<Digest>>>>> =
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

/// Helper for retrieving the content digest of a URL.
async fn get_content_digest(transferer: &dyn Transferer, url: &Url) -> Result<Arc<ContentDigest>> {
    match transferer.digest(url).await.with_context(|| {
        format!(
            "failed to get content digest of URL `{url}`",
            url = url.display()
        )
    })? {
        Some(digest) => Ok(digest),
        None => bail!("URL `{url}` does not have a known content digest"),
    }
}

/// Calculates the content digest of a local path.
///
/// If the path is a file, a [blake3](blake3) digest is calculated for the
/// file's content.
///
/// If the path is a directory, a consistent, recursive walk of the directory is
/// performed and a digest calculated based on the directory's entries.
///
/// The hash of a directory entry consist of:
///
/// * The relative path to the entry.
/// * Whether or not the entry is a file or a directory.
/// * If the entry is a file, the hash of the file's contents as noted above.
///
/// [blake3]: https://github.com/BLAKE3-team/BLAKE3
pub async fn calculate_local_digest(path: &Path, kind: ContentKind) -> Result<Digest> {
    let digest = {
        let mut digests = DIGESTS.lock().expect("failed to lock digests");
        digests
            .entry(EvaluationPath::Local(path.to_path_buf()))
            .or_default()
            .clone()
    };

    // Get an existing result or initialize a new one exactly once
    Ok(*digest
        .get_or_try_init(|| async {
            let path = path.to_path_buf();
            spawn_blocking(move || {
                let metadata = path.metadata().with_context(|| {
                    format!("failed to read metadata of `{path}`", path = path.display())
                })?;

                // If the path is a file, hash just the file.
                if kind == ContentKind::File {
                    if !metadata.is_file() {
                        bail!("expected path `{path}` to be a file", path = path.display());
                    }

                    let mut hasher = Hasher::new();
                    hasher.update_mmap_rayon(&path).with_context(|| {
                        format!(
                            "failed to calculate digest of `{path}`",
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
                    entry_path
                        .strip_prefix(&path)
                        .unwrap_or(entry_path)
                        .to_str()
                        .with_context(|| {
                            format!("path `{path}` is not UTF-8", path = entry_path.display())
                        })?
                        .hash(&mut hasher);

                    if metadata.is_file() {
                        // If entry is a file, hash its tag and content
                        hasher
                            .update(&[FILE_VARIANT_TAG])
                            .update_mmap_rayon(entry_path)
                            .with_context(|| {
                                format!(
                                    "failed to calculate digest of file `{path}`",
                                    path = entry_path.display()
                                )
                            })?;
                    } else {
                        // Otherwise for a directory, just hash the tag
                        hasher.update(&[DIRECTORY_VARIANT_TAG]);
                    }

                    entries += 1;
                }

                hasher.update(&(entries as u32).to_le_bytes());
                Ok(Digest::Directory(hasher.finalize()))
            })
            .await
            .expect("digest task failed")
        })
        .await?)
}

/// Calculates the content digest of a remote URL.
///
/// If the URL is to a remote file, a `HEAD` request is made and the response
/// must have an associated content digest header; the header's value is hashed
/// to produce the content digest of the file.
///
/// If the URL is a "directory", a consistent, recursive walk of the directory
/// is performed and a digest calculated based on the directory's entries.
///
/// The hash of a directory entry consist of:
///
/// * The relative path to the entry.
/// * The content digest of the entry.
pub async fn calculate_remote_digest(
    transferer: &dyn Transferer,
    url: &Url,
    kind: ContentKind,
) -> Result<Digest> {
    let digest = {
        let mut digests = DIGESTS.lock().expect("failed to lock digests");
        digests
            .entry(EvaluationPath::Remote(url.clone()))
            .or_default()
            .clone()
    };

    // Get an existing result or initialize a new one exactly once
    Ok(*digest
        .get_or_try_init(|| async {
            // If there were no entries, treat the URL as a file
            if kind == ContentKind::File {
                let digest = get_content_digest(transferer, url).await?;
                let mut hasher = Hasher::new();
                digest.hash(&mut hasher);
                return anyhow::Ok(Digest::File(hasher.finalize()));
            }

            // Walk the URL; the returned entries are in lexicographical order
            let entries = transferer
                .walk(url)
                .await
                .with_context(|| format!("failed to walk URL `{url}`", url = url.display()))?;

            let mut hasher = Hasher::new();
            for entry in entries.iter() {
                let mut url = url.clone();

                {
                    let mut segments = url.path_segments_mut().expect("URL should have a path");
                    for segment in entry.split('/') {
                        segments.push(segment);
                    }
                }

                let digest = get_content_digest(transferer, &url).await?;
                entry.hash(&mut hasher);
                digest.hash(&mut hasher);
            }

            hasher.update(&(entries.len() as u32).to_le_bytes());
            Ok(Digest::Directory(hasher.finalize()))
        })
        .await?)
}
