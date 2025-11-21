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

                    // Ignore the root
                    if entry.path() == path {
                        continue;
                    }

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
                        // If entry is a file, hash the tag, length, and content
                        ContentKind::File.hash(&mut hasher);
                        hasher.update(&metadata.len().to_le_bytes());
                        hasher.update_mmap_rayon(entry_path).with_context(|| {
                            format!(
                                "failed to calculate digest of file `{path}`",
                                path = entry_path.display()
                            )
                        })?;
                    } else {
                        // Otherwise for a directory, just hash the tag
                        ContentKind::Directory.hash(&mut hasher);
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

#[cfg(test)]
pub(crate) mod test {
    use std::fs;
    use std::io::Write;
    use std::path::MAIN_SEPARATOR;

    use anyhow::anyhow;
    use futures::FutureExt;
    use futures::future::BoxFuture;
    use pretty_assertions::assert_eq;
    use tempfile::NamedTempFile;
    use tempfile::tempdir;

    use super::*;
    use crate::http::Location;

    /// Helper for clearing the cached digests for tests
    pub fn clear_digest_cache() {
        DIGESTS.lock().expect("failed to lock digests").clear();
    }

    pub struct DigestTransferer(HashMap<&'static str, Option<Arc<ContentDigest>>>);

    impl DigestTransferer {
        pub fn new<C>(c: C) -> Self
        where
            C: IntoIterator<Item = (&'static str, Option<ContentDigest>)>,
        {
            Self(HashMap::from_iter(
                c.into_iter().map(|(k, v)| (k, v.map(Into::into))),
            ))
        }
    }

    impl Transferer for DigestTransferer {
        fn download<'a>(&'a self, _source: &'a Url) -> BoxFuture<'a, Result<Location>> {
            unimplemented!()
        }

        fn upload<'a>(
            &'a self,
            _source: &'a Path,
            _destination: &'a Url,
        ) -> BoxFuture<'a, Result<()>> {
            unimplemented!()
        }

        fn size<'a>(&'a self, _url: &'a Url) -> BoxFuture<'a, Result<Option<u64>>> {
            unimplemented!()
        }

        fn walk<'a>(&'a self, url: &'a Url) -> BoxFuture<'a, Result<Arc<[String]>>> {
            async {
                let mut entries = Vec::new();
                for k in self.0.keys() {
                    if let Some(path) = k.strip_prefix(url.as_str()) {
                        let path = path.strip_prefix("/").unwrap_or(path);
                        entries.push(path.to_string());
                    }
                }

                entries.sort();
                Ok(entries.into())
            }
            .boxed()
        }

        fn exists<'a>(&'a self, _url: &'a Url) -> BoxFuture<'a, Result<bool>> {
            unimplemented!()
        }

        fn digest<'a>(&'a self, url: &'a Url) -> BoxFuture<'a, Result<Option<Arc<ContentDigest>>>> {
            async {
                Ok(self
                    .0
                    .get(url.as_str())
                    .ok_or_else(|| anyhow!("does not exist"))?
                    .clone())
            }
            .boxed()
        }
    }

    #[tokio::test]
    async fn local_file_digest() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world!").unwrap();

        let digest = calculate_local_digest(file.path(), ContentKind::File)
            .await
            .unwrap();
        // Digest of `hello world!` from https://emn178.github.io/online-tools/blake3/
        assert_eq!(
            *digest.to_hex(),
            *"3aa61c409fd7717c9d9c639202af2fae470c0ef669be7ba2caea5779cb534e9d"
        );
    }

    #[tokio::test]
    async fn local_directory_digest() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a"), b"a").unwrap();
        fs::write(dir.path().join("b"), b"b").unwrap();
        fs::write(dir.path().join("c"), b"c").unwrap();

        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("z"), b"z").unwrap();
        fs::write(subdir.join("y"), b"y").unwrap();
        fs::write(subdir.join("x"), b"x").unwrap();

        let digest = calculate_local_digest(dir.path(), ContentKind::Directory)
            .await
            .unwrap();

        let mut hasher = Hasher::new();
        hasher.update(&1u32.to_le_bytes()); // Path length
        hasher.update("a".as_bytes()); // Path
        hasher.update(&[0]); // File tag
        hasher.update(&1u64.to_le_bytes()); // File length
        hasher.update(b"a"); // File contents
        hasher.update(&1u32.to_le_bytes()); // Path length
        hasher.update("b".as_bytes()); // Path
        hasher.update(&[0]); // File tag
        hasher.update(&1u64.to_le_bytes()); // File length
        hasher.update(b"b"); // File contents
        hasher.update(&1u32.to_le_bytes()); // Path length
        hasher.update("c".as_bytes()); // Path
        hasher.update(&[0]); // File tag
        hasher.update(&1u64.to_le_bytes()); // File length
        hasher.update(b"c"); // File contents
        hasher.update(&6u32.to_le_bytes()); // Path length
        hasher.update("subdir".as_bytes()); // Path
        hasher.update(&[1]); // Directory tag
        hasher.update(&8u32.to_le_bytes()); // Path length
        hasher.update(format!("subdir{}x", MAIN_SEPARATOR).as_bytes()); // Path
        hasher.update(&[0]); // File tag
        hasher.update(&1u64.to_le_bytes()); // File length
        hasher.update(b"x"); // File contents
        hasher.update(&8u32.to_le_bytes()); // Path length
        hasher.update(format!("subdir{}y", MAIN_SEPARATOR).as_bytes()); // Path
        hasher.update(&[0]); // File tag
        hasher.update(&1u64.to_le_bytes()); // File length
        hasher.update(b"y"); // File contents
        hasher.update(&8u32.to_le_bytes()); // Path length
        hasher.update(format!("subdir{}z", MAIN_SEPARATOR).as_bytes()); // Path
        hasher.update(&[0]); // File tag
        hasher.update(&1u64.to_le_bytes()); // File length
        hasher.update(b"z"); // File contents
        hasher.update(&7u32.to_le_bytes()); // Number of entries
        assert_eq!(digest.to_hex(), hasher.finalize().to_hex());
    }

    #[tokio::test]
    async fn remote_file_digest() {
        // SHA-256 of `hello world!`
        let content_digest =
            Hash::from_hex("7509e5bda0c762d2bac7f90d758b5b2263fa01ccbc542ab5e3df163be08e6ca9")
                .unwrap();

        let transferer = DigestTransferer::new([
            (
                "http://example.com/foo",
                Some(ContentDigest::Hash {
                    algorithm: "sha256".to_string(),
                    digest: content_digest.as_bytes().into(),
                }),
            ),
            (
                "http://example.com/bar",
                Some(ContentDigest::ETag("etag".into())),
            ),
            ("http://example.com/baz", None),
        ]);

        // URL with Content-Digest header
        let digest = calculate_remote_digest(
            &transferer,
            &"http://example.com/foo".parse().unwrap(),
            ContentKind::File,
        )
        .await
        .unwrap();

        let mut hasher = Hasher::new();
        hasher.update(&[0]); // Hash tag
        hasher.update(&6u32.to_le_bytes()); // Algorithm length
        hasher.update("sha256".as_bytes()); // Algorithm
        hasher.update(&32u32.to_le_bytes()); // Digest length
        hasher.update(content_digest.as_bytes()); // Digest bytes
        assert_eq!(digest.to_hex(), hasher.finalize().to_hex());

        // URL with ETag header
        let digest = calculate_remote_digest(
            &transferer,
            &"http://example.com/bar".parse().unwrap(),
            ContentKind::File,
        )
        .await
        .unwrap();

        let mut hasher = Hasher::new();
        hasher.update(&[1]); // ETag tag
        hasher.update(&4u32.to_le_bytes()); // ETag length
        hasher.update("etag".as_bytes()); // ETag
        assert_eq!(digest.to_hex(), hasher.finalize().to_hex());

        // URL with no digest
        assert_eq!(
            calculate_remote_digest(
                &transferer,
                &"http://example.com/baz".parse().unwrap(),
                ContentKind::File,
            )
            .await
            .unwrap_err()
            .to_string(),
            "URL `http://example.com/baz` does not have a known content digest"
        );

        // 404
        assert_eq!(
            format!(
                "{:#}",
                calculate_remote_digest(
                    &transferer,
                    &"http://example.com/nope".parse().unwrap(),
                    ContentKind::File,
                )
                .await
                .unwrap_err()
            ),
            "failed to get content digest of URL `http://example.com/nope`: does not exist"
        );
    }

    #[tokio::test]
    async fn remote_directory_digest() {
        // SHA-256 of `hello world!`
        let content_digest =
            Hash::from_hex("7509e5bda0c762d2bac7f90d758b5b2263fa01ccbc542ab5e3df163be08e6ca9")
                .unwrap();

        let transferer = DigestTransferer::new([
            (
                "http://example.com/dir/foo",
                Some(ContentDigest::Hash {
                    algorithm: "sha256".to_string(),
                    digest: content_digest.as_bytes().into(),
                }),
            ),
            (
                "http://example.com/dir/bar/baz",
                Some(ContentDigest::ETag("etag".into())),
            ),
            ("http://example.com/missing/baz", None),
        ]);

        // Digest of a remote "directory"
        let digest = calculate_remote_digest(
            &transferer,
            &"http://example.com/dir".parse().unwrap(),
            ContentKind::Directory,
        )
        .await
        .unwrap();

        let mut hasher = Hasher::new();
        hasher.update(&7u32.to_le_bytes()); // Path length
        hasher.update("bar/baz".as_bytes()); // Path
        hasher.update(&[1]); // ETag tag
        hasher.update(&4u32.to_le_bytes()); // ETag length
        hasher.update("etag".as_bytes()); // ETag
        hasher.update(&3u32.to_le_bytes()); // Path length
        hasher.update("foo".as_bytes()); // Path
        hasher.update(&[0]); // Hash tag
        hasher.update(&6u32.to_le_bytes()); // Algorithm length
        hasher.update("sha256".as_bytes()); // Algorithm
        hasher.update(&32u32.to_le_bytes()); // Digest length
        hasher.update(content_digest.as_bytes()); // Digest bytes
        hasher.update(&2u32.to_le_bytes()); // Number of entries
        assert_eq!(digest.to_hex(), hasher.finalize().to_hex());

        // Digest of a remote "directory" that is "empty"
        // We can't distinguish between a non-existent directory and an empty one
        let digest = calculate_remote_digest(
            &transferer,
            &"http://example.com/empty".parse().unwrap(),
            ContentKind::Directory,
        )
        .await
        .unwrap();

        let mut hasher = Hasher::new();
        hasher.update(&0u32.to_le_bytes()); // Number of entries
        assert_eq!(digest.to_hex(), hasher.finalize().to_hex());

        // Digest of a remote "directory" containing a file with a missing content
        // digest
        assert_eq!(
            format!(
                "{:#}",
                calculate_remote_digest(
                    &transferer,
                    &"http://example.com/missing".parse().unwrap(),
                    ContentKind::Directory,
                )
                .await
                .unwrap_err()
            ),
            "URL `http://example.com/missing/baz` does not have a known content digest"
        );
    }
}
