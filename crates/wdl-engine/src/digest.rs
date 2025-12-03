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
use futures::FutureExt;
use tokio::sync::OnceCell;
use tokio::task::spawn_blocking;
use tracing::debug;
use url::Url;

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

/// Calculates the digest of a local file.
async fn calculate_file_digest(path: &Path) -> Result<Digest> {
    let path = path.to_path_buf();
    spawn_blocking(move || {
        let mut hasher = Hasher::new();
        hasher.update_mmap_rayon(&path).with_context(|| {
            format!(
                "failed to calculate digest of `{path}`",
                path = path.display()
            )
        })?;

        anyhow::Ok(Digest::File(hasher.finalize()))
    })
    .await
    .context("file digest task panicked")?
}

/// Calculates the digest of a local directory.
///
/// This is a recursive operation where every file and directory recursively
/// contained in the directory will have their content digests calculated.
///
/// Returns a boxed future to break the type recursion.
fn calculate_directory_digest(path: &Path) -> impl Future<Output = Result<Digest>> + Send {
    let path = path.to_path_buf();
    async move {
        let mut dir = tokio::fs::read_dir(&path)
            .await
            .with_context(|| format!("failed to read directory `{path}`", path = path.display()))?;

        let mut entries = Vec::new();
        while let Some(entry) = dir
            .next_entry()
            .await
            .with_context(|| format!("failed to read directory `{path}`", path = path.display()))?
        {
            entries.push(entry);
        }

        // Sort the entries by name so that the digest order is consistent
        drop(dir);
        entries.sort_by_key(|e| e.file_name());

        let mut hasher = Hasher::new();
        for entry in &entries {
            let entry_path = entry.path();
            let metadata = entry.metadata().await.with_context(|| {
                format!(
                    "failed to read metadata for path `{path}`",
                    path = entry_path.display()
                )
            })?;

            // Hash the relative path to the entry
            let entry_rel_path = entry_path
                .strip_prefix(&path)
                .unwrap_or(&entry_path)
                .to_str()
                .with_context(|| {
                    format!("path `{path}` is not UTF-8", path = entry_path.display())
                })?;
            entry_rel_path.hash(&mut hasher);

            let kind = if metadata.is_file() {
                ContentKind::File
            } else {
                ContentKind::Directory
            };

            // Recursively calculate the entry's digest
            let digest = calculate_local_digest(&entry_path, kind).await?;
            digest.hash(&mut hasher);
        }

        hasher.update(&(entries.len() as u32).to_le_bytes());
        Ok(Digest::Directory(hasher.finalize()))
    }
    .boxed()
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
        .get_or_try_init(|| async move {
            let metadata = path.metadata().with_context(|| {
                format!("failed to read metadata of `{path}`", path = path.display())
            })?;

            debug!(
                "calculating content digest of `{path}`",
                path = path.display()
            );

            if kind == ContentKind::File {
                if !metadata.is_file() {
                    bail!("expected path `{path}` to be a file", path = path.display());
                }

                calculate_file_digest(path).await
            } else {
                if metadata.is_file() {
                    bail!(
                        "expected path `{path}` to be a directory",
                        path = path.display()
                    );
                }

                calculate_directory_digest(path).await
            }
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
            debug!("calculating content digest of `{url}`", url = url.display());

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

        // Calculate the digest of the `subdir`
        let mut hasher = Hasher::new();
        hasher.update(&1u32.to_le_bytes()); // Path length
        hasher.update("x".as_bytes()); // Path
        hasher.update(&[0]); // File digest tag
        hasher.update(&32u32.to_le_bytes()); // File digest length
        hasher.update(
            // Digest of `x` from https://emn178.github.io/online-tools/blake3/
            Hash::from_hex("3ae7d805f6789a6402acb70ad4096a85a56bf6804eaf25c0493ac697548d30b5")
                .unwrap()
                .as_bytes(),
        ); // File digest
        hasher.update(&1u32.to_le_bytes()); // Path length
        hasher.update("y".as_bytes()); // Path
        hasher.update(&[0]); // File digest tag
        hasher.update(&32u32.to_le_bytes()); // File digest length
        hasher.update(
            // Digest of `y` from https://emn178.github.io/online-tools/blake3/
            Hash::from_hex("08112a9e334ce73042b531c25668cf5cb12a1ee040a4326afeac065461079a06")
                .unwrap()
                .as_bytes(),
        ); // File digest
        hasher.update(&1u32.to_le_bytes()); // Path length
        hasher.update("z".as_bytes()); // Path
        hasher.update(&[0]); // File digest tag
        hasher.update(&32u32.to_le_bytes()); // File digest length
        hasher.update(
            // Digest of `z` from https://emn178.github.io/online-tools/blake3/
            Hash::from_hex("1104908ab930e671002c7cd7f3fc921570b1bf64ecfa12fe363585c630eaca6b")
                .unwrap()
                .as_bytes(),
        ); // File digest
        hasher.update(&3u32.to_le_bytes()); // Number of entries
        let subdir_digest = hasher.finalize();

        // Calculate the digest of the parent directory
        let mut hasher = Hasher::new();
        hasher.update(&1u32.to_le_bytes()); // Path length
        hasher.update("a".as_bytes()); // Path
        hasher.update(&[0]); // File digest tag
        hasher.update(&32u32.to_le_bytes()); // File digest length
        hasher.update(
            // Digest of `a` from https://emn178.github.io/online-tools/blake3/
            Hash::from_hex("17762fddd969a453925d65717ac3eea21320b66b54342fde15128d6caf21215f")
                .unwrap()
                .as_bytes(),
        ); // File digest
        hasher.update(&1u32.to_le_bytes()); // Path length
        hasher.update("b".as_bytes()); // Path
        hasher.update(&[0]); // File digest tag
        hasher.update(&32u32.to_le_bytes()); // File digest length
        hasher.update(
            // Digest of `b` from https://emn178.github.io/online-tools/blake3/
            Hash::from_hex("10e5cf3d3c8a4f9f3468c8cc58eea84892a22fdadbc1acb22410190044c1d553")
                .unwrap()
                .as_bytes(),
        ); // File digest
        hasher.update(&1u32.to_le_bytes()); // Path length
        hasher.update("c".as_bytes()); // Path
        hasher.update(&[0]); // File digest tag
        hasher.update(&32u32.to_le_bytes()); // File digest length
        hasher.update(
            // Digest of `c` from https://emn178.github.io/online-tools/blake3/
            Hash::from_hex("ea7aa1fc9efdbe106dbb70369a75e9671fa29d52bd55536711bf197477b8f021")
                .unwrap()
                .as_bytes(),
        ); // File digest
        hasher.update(&6u32.to_le_bytes()); // Path length
        hasher.update("subdir".as_bytes()); // Path
        hasher.update(&[1]); // Directory digest tag
        hasher.update(&32u32.to_le_bytes()); // Directory digest length
        hasher.update(subdir_digest.as_bytes()); // Directory digest
        hasher.update(&4u32.to_le_bytes()); // Number of entries
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
