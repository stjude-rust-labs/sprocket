//! Implements calculation of file and directory content digests.
//!
//! This is used by the call cache and for uploading inputs for remote backends.

use std::fs;
use std::io::Read;
use std::io::copy;
use std::num::NonZeroUsize;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use arrayvec::ArrayString;
use blake3::Hash;
use blake3::Hasher;
use cloud_copy::ContentDigest;
use cloud_copy::UrlExt;
use futures::FutureExt;
use futures::future::BoxFuture;
use tokio::task::spawn_blocking;
use tracing::debug;
use url::Url;

use crate::Cache;
use crate::CancellationContext;
use crate::ContentKind;
use crate::EvaluationHttpClient;
use crate::EvaluationPath;
use crate::EvaluationPathKind;
use crate::cache::Hashable;
use crate::config::ContentDigestMode;

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

/// The number of bytes from the start of a file that are hashed when
/// calculating a "strongish" digest.
///
/// This mirrors Cromwell's `fingerprint` call caching strategy, which hashes
/// only the first 10 MiB of a file's contents.
const STRONGISH_DIGEST_PREFIX_LEN: u64 = 10 * 1024 * 1024;

/// The inner state of [`DigestCalculator`].
struct DigestCalculatorInner {
    /// The evaluation HTTP client to use for digesting.
    client: EvaluationHttpClient,
    /// The cancellation context for the evaluation.
    cancellation: CancellationContext,
    /// The cache of digests for local files.
    local_digests: Cache<(ContentDigestMode, PathBuf), Digest>,
    /// The cache of digests for remote URLs.
    remote_digests: Cache<Url, Digest>,
}

/// Represents a calculator of Blake3 digests for files and directories.
///
/// This type is cheaply cloned.
#[derive(Clone)]
pub struct DigestCalculator(Arc<DigestCalculatorInner>);

impl DigestCalculator {
    /// Constructs a new [`DigestCalculator`].
    ///
    /// # Panics
    ///
    /// Panics if the provided cache capacity is zero.
    pub fn new(
        client: EvaluationHttpClient,
        cancellation: CancellationContext,
        capacity: usize,
    ) -> Self {
        let capacity = NonZeroUsize::new(capacity).expect("the cache capacity cannot be zero");

        Self(
            DigestCalculatorInner {
                client,
                cancellation,
                local_digests: Cache::new(capacity),
                remote_digests: Cache::new(capacity),
            }
            .into(),
        )
    }

    /// Calculates the content digest of the given evaluation path.
    pub async fn calculate_digest(
        &self,
        path: &EvaluationPath,
        kind: ContentKind,
        mode: ContentDigestMode,
    ) -> Result<Digest> {
        match path.kind() {
            EvaluationPathKind::Local(path) => self.calculate_local_digest(path, kind, mode).await,
            EvaluationPathKind::Remote(url) => self.calculate_remote_digest(url, kind).await,
        }
    }

    /// Calculates the content digest of a local path.
    ///
    /// If the path is a file, a [blake3](blake3) digest is calculated for the
    /// file's content.
    ///
    /// If the path is a directory, a consistent, recursive walk of the
    /// directory is performed and a digest calculated based on the
    /// directory's entries.
    ///
    /// The hash of a directory entry consist of:
    ///
    /// * The relative path to the entry.
    /// * Whether or not the entry is a file or a directory.
    /// * If the entry is a file, the hash of the file's contents as noted
    ///   above.
    ///
    /// [blake3]: https://github.com/BLAKE3-team/BLAKE3
    pub async fn calculate_local_digest(
        &self,
        path: &Path,
        kind: ContentKind,
        mode: ContentDigestMode,
    ) -> Result<Digest> {
        match self
            .0
            .local_digests
            .get(
                (mode, path.to_path_buf()),
                &self.0.cancellation,
                async || {
                    let metadata = path.metadata().with_context(|| {
                        format!("failed to read metadata of `{path}`", path = path.display())
                    })?;

                    debug!(
                        "calculating content digest of `{path}`",
                        path = path.display()
                    );

                    match kind {
                        ContentKind::File | ContentKind::TempFile => {
                            if !metadata.is_file() {
                                bail!("expected path `{path}` to be a file", path = path.display());
                            }

                            // Always use a strong digest mode for temporary
                            // files
                            // This will ensure that the file metadata is _not_
                            // considered for the
                            // digest
                            Self::calculate_file_digest(
                                path,
                                if kind == ContentKind::TempFile {
                                    ContentDigestMode::Strong
                                } else {
                                    mode
                                },
                            )
                            .await
                        }
                        ContentKind::Directory => {
                            if metadata.is_file() {
                                bail!(
                                    "expected path `{path}` to be a directory",
                                    path = path.display()
                                );
                            }

                            self.calculate_directory_digest(path, mode).await
                        }
                    }
                },
            )
            .await?
        {
            Some(digest) => Ok(digest),
            None => bail!(
                "failed to calculate digest of `{path}`: the operation was cancelled",
                path = path.display()
            ),
        }
    }

    /// Calculates the content digest of a remote URL.
    ///
    /// If the URL is to a remote file, a `HEAD` request is made and the
    /// response must have an associated content digest header; the header's
    /// value is hashed to produce the content digest of the file.
    ///
    /// If the URL is a "directory", a consistent, recursive walk of the
    /// directory is performed and a digest calculated based on the
    /// directory's entries.
    ///
    /// The hash of a directory entry consist of:
    ///
    /// * The relative path to the entry.
    /// * The content digest of the entry.
    pub async fn calculate_remote_digest(&self, url: &Url, kind: ContentKind) -> Result<Digest> {
        match self
            .0
            .remote_digests
            .get_by_ref(url, &self.0.cancellation, async || {
                debug!("calculating content digest of `{url}`", url = url.display());

                // If there were no entries, treat the URL as a file
                if kind == ContentKind::File {
                    let digest = self.get_content_digest(url).await?;
                    let mut hasher = Hasher::new();
                    digest.hash(&mut hasher);
                    return anyhow::Ok(Digest::File(hasher.finalize()));
                }

                assert_eq!(
                    kind,
                    ContentKind::Directory,
                    "expected a directory for the content kind"
                );

                // Walk the URL; the returned entries are in lexicographical
                // order
                let entries =
                    self.0.client.walk(url).await.with_context(|| {
                        format!("failed to walk URL `{url}`", url = url.display())
                    })?;

                let mut hasher = Hasher::new();
                for entry in entries.iter() {
                    let mut url = url.clone();

                    {
                        // Append the entry to the url; we must pop the last
                        // segment if it is empty
                        // as otherwise `push` will append another empty
                        // segment
                        let mut segments = url.path_segments_mut().expect("URL should have a path");
                        segments.pop_if_empty();
                        for segment in entry.split('/') {
                            segments.push(segment);
                        }
                    }

                    let digest = self.get_content_digest(&url).await?;
                    entry.hash(&mut hasher);
                    digest.hash(&mut hasher);
                }

                hasher.update(&(entries.len() as u32).to_le_bytes());
                Ok(Digest::Directory(hasher.finalize()))
            })
            .await?
        {
            Some(digest) => Ok(digest),
            None => bail!(
                "failed to calculate digest of `{url}`: the operation was cancelled",
                url = url.display()
            ),
        }
    }

    /// Clears the digest caches.
    #[allow(unused)]
    pub fn clear(&self) {
        self.0.local_digests.clear();
        self.0.remote_digests.clear();
    }

    /// Calculates the digest of a local directory.
    ///
    /// This is a recursive operation where every file and directory recursively
    /// contained in the directory will have their content digests calculated.
    ///
    /// Returns a boxed future to break the type recursion.
    fn calculate_directory_digest<'a>(
        &'a self,
        path: &'a Path,
        mode: ContentDigestMode,
    ) -> BoxFuture<'a, Result<Digest>> {
        async move {
            let mut dir = tokio::fs::read_dir(&path).await.with_context(|| {
                format!("failed to read directory `{path}`", path = path.display())
            })?;

            let mut entries = Vec::new();
            while let Some(entry) = dir.next_entry().await.with_context(|| {
                format!("failed to read directory `{path}`", path = path.display())
            })? {
                entries.push(entry);
            }

            // Sort the entries by name so that the digest order is consistent
            drop(dir);
            entries.sort_by_key(|e| e.file_name());

            let mut count: u32 = 0;
            let mut hasher = Hasher::new();
            for entry in &entries {
                let entry_path = entry.path();
                let mut metadata = entry.metadata().await.with_context(|| {
                    format!(
                        "failed to read metadata for path `{path}`",
                        path = entry_path.display()
                    )
                })?;

                // For symlink entries, ensure the link isn't broken by
                // retrieving the target's metadata; if it is
                // broken, ignore it by not including it
                if metadata.is_symlink() {
                    match fs::metadata(&entry_path) {
                        Ok(m) => metadata = m,
                        Err(_) => continue,
                    }
                }

                let kind = if metadata.is_file() {
                    ContentKind::File
                } else {
                    ContentKind::Directory
                };

                // Hash the relative path to the entry
                let entry_rel_path = entry_path
                    .strip_prefix(path)
                    .expect("entry path should be relative")
                    .to_str()
                    .with_context(|| {
                        format!("path `{path}` is not UTF-8", path = entry_path.display())
                    })?;
                entry_rel_path.hash(&mut hasher);

                // Recursively calculate the entry's digest
                let digest = self.calculate_local_digest(&entry_path, kind, mode).await?;
                digest.hash(&mut hasher);
                count += 1;
            }

            hasher.update(&count.to_le_bytes());
            Ok(Digest::Directory(hasher.finalize()))
        }
        .boxed()
    }

    /// Helper for retrieving the content digest of a URL.
    async fn get_content_digest(&self, url: &Url) -> Result<Arc<ContentDigest>> {
        match self.0.client.digest(url).await.with_context(|| {
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
    async fn calculate_file_digest(path: &Path, mode: ContentDigestMode) -> Result<Digest> {
        match mode {
            ContentDigestMode::Strong => {
                // Calculate a Blake3 digest for the file's contents
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
            ContentDigestMode::Strongish => {
                // Calculate a digest off of file metadata and a hash of only
                // the first `STRONGISH_DIGEST_PREFIX_LEN` bytes
                // of the file's contents
                let path = path.to_path_buf();
                spawn_blocking(move || {
                    let mut hasher = Hasher::new();
                    Self::hash_file_metadata(&path, &mut hasher)?;

                    let file = fs::File::open(&path).with_context(|| {
                        format!("failed to open `{path}`", path = path.display())
                    })?;

                    copy(&mut file.take(STRONGISH_DIGEST_PREFIX_LEN), &mut hasher).with_context(
                        || format!("failed to read contents of `{path}`", path = path.display()),
                    )?;

                    anyhow::Ok(Digest::File(hasher.finalize()))
                })
                .await
                .context("file digest task panicked")?
            }
            ContentDigestMode::Weak => {
                // Calculate a digest solely off of file metadata
                let mut hasher = Hasher::new();
                Self::hash_file_metadata(path, &mut hasher)?;
                Ok(Digest::File(hasher.finalize()))
            }
        }
    }

    /// Hashes a file's metadata (size and last modified time) into the given
    /// hasher.
    ///
    /// This is shared between the `weak` and `strongish` content digest modes.
    fn hash_file_metadata(path: &Path, hasher: &mut Hasher) -> Result<()> {
        let metadata = path.metadata().with_context(|| {
            format!("failed to read metadata of `{path}`", path = path.display())
        })?;
        let mtime = metadata
            .modified()
            .with_context(|| {
                format!(
                    "failed to determine last modified time of `{path}`",
                    path = path.display()
                )
            })?
            .duration_since(UNIX_EPOCH)
            .with_context(|| {
                format!(
                    "last modified time of `{path}` occurs is before UNIX epoch",
                    path = path.display()
                )
            })?;

        hasher.update(&metadata.len().to_le_bytes());
        hasher.update(&mtime.as_secs().to_le_bytes());
        hasher.update(&mtime.as_millis().to_le_bytes());
        hasher.update(&mtime.as_micros().to_le_bytes());
        hasher.update(&mtime.as_nanos().to_le_bytes());
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::io::Write;
    use std::time::Duration;
    use std::time::SystemTime;

    use anyhow::anyhow;
    use cloud_copy::TransferEvent;
    use futures::FutureExt;
    use futures::future::BoxFuture;
    use pretty_assertions::assert_eq;
    use tempfile::NamedTempFile;
    use tempfile::tempdir;
    use tokio::sync::broadcast;

    use super::*;
    use crate::Cache;
    use crate::CancellationContext;
    use crate::Config;
    use crate::ContentKind;
    use crate::Engine;
    use crate::Events;
    use crate::http::HttpClient;
    use crate::http::Location;

    #[derive(Default)]
    pub struct DigestHttpClient(HashMap<&'static str, Option<Arc<ContentDigest>>>);

    impl DigestHttpClient {
        pub fn new<C>(c: C) -> Self
        where
            C: IntoIterator<Item = (&'static str, Option<ContentDigest>)>,
        {
            Self(HashMap::from_iter(
                c.into_iter().map(|(k, v)| (k, v.map(Into::into))),
            ))
        }
    }

    impl HttpClient for DigestHttpClient {
        fn download<'a>(
            &'a self,
            _: &'a Url,
            _: Option<broadcast::Sender<TransferEvent>>,
            _: &'a CancellationContext,
            _: &'a Cache<Url, Location>,
        ) -> BoxFuture<'a, Result<Location>> {
            unimplemented!()
        }

        fn upload<'a>(
            &'a self,
            _: &'a Path,
            _: &'a Url,
            _: Option<broadcast::Sender<TransferEvent>>,
            _: &'a CancellationContext,
            _: &'a Cache<Url, ()>,
        ) -> BoxFuture<'a, Result<()>> {
            unimplemented!()
        }

        fn size<'a>(
            &'a self,
            _: &'a Url,
            _: &'a CancellationContext,
            _: &'a Cache<Url, Option<u64>>,
        ) -> BoxFuture<'a, Result<Option<u64>>> {
            unimplemented!()
        }

        fn walk<'a>(
            &'a self,
            url: &'a Url,
            _: &'a CancellationContext,
            _: &'a Cache<Url, Arc<[String]>>,
        ) -> BoxFuture<'a, Result<Arc<[String]>>> {
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

        fn exists<'a>(
            &'a self,
            _url: &'a Url,
            _: &'a CancellationContext,
            _: &'a Cache<Url, bool>,
        ) -> BoxFuture<'a, Result<bool>> {
            unimplemented!()
        }

        fn digest<'a>(
            &'a self,
            url: &'a Url,
            _: &'a CancellationContext,
            _: &'a Cache<Url, Option<Arc<ContentDigest>>>,
        ) -> BoxFuture<'a, Result<Option<Arc<ContentDigest>>>> {
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

    /// Creates a digest calculator for the tests.
    pub async fn digests(client: DigestHttpClient) -> DigestCalculator {
        let engine = Engine::new_with_http_client(Config::local(), client)
            .await
            .unwrap();

        let cancellation = CancellationContext::default();
        let client = EvaluationHttpClient::new(&engine, &Events::disabled(), cancellation.clone());
        DigestCalculator::new(client, cancellation, 1000)
    }

    #[tokio::test]
    async fn local_file_digest_strong() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world!").unwrap();

        let digests = digests(Default::default()).await;

        let digest = digests
            .calculate_local_digest(file.path(), ContentKind::File, ContentDigestMode::Strong)
            .await
            .unwrap();

        // Digest of `hello world!` from https://emn178.github.io/online-tools/blake3/
        assert_eq!(
            *digest.to_hex(),
            *"3aa61c409fd7717c9d9c639202af2fae470c0ef669be7ba2caea5779cb534e9d"
        );
    }

    #[tokio::test]
    async fn local_file_digest_weak() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world!").unwrap();

        let digests = digests(Default::default()).await;

        let digest = digests
            .calculate_local_digest(file.path(), ContentKind::File, ContentDigestMode::Weak)
            .await
            .unwrap();

        // It should match the digest returned by `calculate_file_digest`
        assert_eq!(
            digest,
            DigestCalculator::calculate_file_digest(file.path(), ContentDigestMode::Weak)
                .await
                .unwrap()
        );

        // The digest should change if we modify its size
        file.write_all(b"!").unwrap();
        file.flush().unwrap();

        digests.clear();

        let changed = digests
            .calculate_local_digest(file.path(), ContentKind::File, ContentDigestMode::Weak)
            .await
            .unwrap();

        assert!(digest != changed, "expected digest to change");

        let digest = changed;

        // The digest should change if we modify the mtime
        file.as_file()
            .set_modified(
                SystemTime::now()
                    .checked_sub(Duration::from_hours(1))
                    .unwrap(),
            )
            .unwrap();

        digests.clear();

        let changed = digests
            .calculate_local_digest(file.path(), ContentKind::File, ContentDigestMode::Weak)
            .await
            .unwrap();

        assert!(digest != changed, "expected digest to change");
    }

    #[tokio::test]
    async fn local_file_digest_strongish() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world!").unwrap();

        let digests = digests(Default::default()).await;

        let digest = digests
            .calculate_local_digest(file.path(), ContentKind::File, ContentDigestMode::Strongish)
            .await
            .unwrap();

        // It should match the digest returned by `calculate_file_digest`
        assert_eq!(
            digest,
            DigestCalculator::calculate_file_digest(file.path(), ContentDigestMode::Strongish)
                .await
                .unwrap()
        );

        // The digest should change if we modify its size
        file.write_all(b"!").unwrap();
        file.flush().unwrap();

        digests.clear();

        let changed = digests
            .calculate_local_digest(file.path(), ContentKind::File, ContentDigestMode::Strongish)
            .await
            .unwrap();

        assert!(digest != changed, "expected digest to change");

        let digest = changed;

        // The digest should change if we modify the mtime
        file.as_file()
            .set_modified(
                SystemTime::now()
                    .checked_sub(Duration::from_hours(1))
                    .unwrap(),
            )
            .unwrap();

        digests.clear();

        let changed = digests
            .calculate_local_digest(file.path(), ContentKind::File, ContentDigestMode::Strongish)
            .await
            .unwrap();

        assert!(digest != changed, "expected digest to change");
    }

    #[tokio::test]
    async fn local_file_digest_strongish_ignores_content_past_prefix() {
        // Create two files that are identical for the first
        // `STRONGISH_DIGEST_PREFIX_LEN` bytes, but differ afterward; a
        // `strongish` digest should not be able to tell them apart so long as
        // their size and mtime also match
        let mut a = NamedTempFile::new().unwrap();
        let mut b = NamedTempFile::new().unwrap();

        let prefix = vec![0u8; STRONGISH_DIGEST_PREFIX_LEN as usize];
        a.write_all(&prefix).unwrap();
        b.write_all(&prefix).unwrap();
        a.write_all(b"a").unwrap();
        b.write_all(b"b").unwrap();
        a.flush().unwrap();
        b.flush().unwrap();

        // Force both files to have the same size and mtime
        let mtime = SystemTime::now();
        a.as_file().set_modified(mtime).unwrap();
        b.as_file().set_modified(mtime).unwrap();

        let digest_a =
            DigestCalculator::calculate_file_digest(a.path(), ContentDigestMode::Strongish)
                .await
                .unwrap();
        let digest_b =
            DigestCalculator::calculate_file_digest(b.path(), ContentDigestMode::Strongish)
                .await
                .unwrap();

        assert_eq!(
            digest_a, digest_b,
            "expected digests to match since they differ only past the strongish prefix length"
        );

        // A strong digest, on the other hand, should be able to tell the
        // difference
        let digest_a = DigestCalculator::calculate_file_digest(a.path(), ContentDigestMode::Strong)
            .await
            .unwrap();
        let digest_b = DigestCalculator::calculate_file_digest(b.path(), ContentDigestMode::Strong)
            .await
            .unwrap();

        assert!(digest_a != digest_b, "expected strong digests to differ");
    }

    #[tokio::test]
    async fn temp_files_always_use_strong_digests() {
        // Create two temporary files with the same content
        let mut a = NamedTempFile::new().unwrap();
        let mut b = NamedTempFile::new().unwrap();

        a.write_all(b"samesies!!!").unwrap();
        b.write_all(b"samesies!!!").unwrap();
        a.flush().unwrap();
        b.flush().unwrap();

        let digests = digests(Default::default()).await;

        // Regardless of the content digest mode, temporary files should
        // _always_ use a strong digest
        let digest_a = digests
            .calculate_local_digest(a.path(), ContentKind::TempFile, ContentDigestMode::Weak)
            .await
            .unwrap();
        let digest_b = digests
            .calculate_local_digest(
                b.path(),
                ContentKind::TempFile,
                ContentDigestMode::Strongish,
            )
            .await
            .unwrap();
        let digest_c = digests
            .calculate_local_digest(b.path(), ContentKind::TempFile, ContentDigestMode::Strong)
            .await
            .unwrap();

        assert_eq!(
            digest_a, digest_b,
            "expected digests to match since temporary files always use strong digests"
        );
        assert_eq!(
            digest_a, digest_c,
            "expected digests to match since temporary files always use strong digests"
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

        let digests = digests(Default::default()).await;

        let digest = digests
            .calculate_local_digest(
                dir.path(),
                ContentKind::Directory,
                ContentDigestMode::Strong,
            )
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

        let digests = digests(DigestHttpClient::new([
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
        ]))
        .await;

        // URL with Content-Digest header
        let digest = digests
            .calculate_remote_digest(
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
        let digest = digests
            .calculate_remote_digest(
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
            digests
                .calculate_remote_digest(
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
                digests
                    .calculate_remote_digest(
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

        let digests = digests(DigestHttpClient::new([
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
        ]))
        .await;

        // Digest of a remote "directory"
        let digest = digests
            .calculate_remote_digest(
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

        // Digest of a remote "directory" with a trailing slash
        let trailing_digest = digests
            .calculate_remote_digest(
                &"http://example.com/dir/".parse().unwrap(),
                ContentKind::Directory,
            )
            .await
            .unwrap();
        assert_eq!(digest, trailing_digest);

        // Digest of a remote "directory" that is "empty"
        // We can't distinguish between a non-existent directory and an empty
        // one
        let digest = digests
            .calculate_remote_digest(
                &"http://example.com/empty".parse().unwrap(),
                ContentKind::Directory,
            )
            .await
            .unwrap();

        let mut hasher = Hasher::new();
        hasher.update(&0u32.to_le_bytes()); // Number of entries
        assert_eq!(digest.to_hex(), hasher.finalize().to_hex());

        // Digest of a remote "directory" containing a file with a missing
        // content digest
        assert_eq!(
            format!(
                "{:#}",
                digests
                    .calculate_remote_digest(
                        &"http://example.com/missing".parse().unwrap(),
                        ContentKind::Directory,
                    )
                    .await
                    .unwrap_err()
            ),
            "URL `http://example.com/missing/baz` does not have a known content digest"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn ignore_broken_symlink() {
        use std::os::unix::fs::symlink;

        // Create a temp file as the target of the symlink
        let target = NamedTempFile::new()
            .expect("failed to create temporary file")
            .into_temp_path();
        fs::write(&target, b"hello world!").expect("failed to write temporary file");

        // Symlink the file
        let dir = tempdir().expect("failed to create temp directory");
        let link = dir.path().join("b");
        symlink(&target, &link).expect("failed to create symlink");

        let digests = digests(Default::default()).await;

        // Digest the directory with the file
        let digest = digests
            .calculate_directory_digest(dir.path(), ContentDigestMode::Strong)
            .await
            .expect("failed to calculate digest");

        // Delete the file to break the link
        fs::remove_file(&target).expect("failed to delete file");

        // Digest again; the link should be ignored and the digest changed
        let modified = digests
            .calculate_directory_digest(dir.path(), ContentDigestMode::Strong)
            .await
            .expect("failed to calculate digest");
        assert_ne!(digest, modified);

        // Restore the file
        fs::write(&target, b"hello world!").expect("failed to create temporary file");

        // Digest again; the digest should match the original
        let modified = digests
            .calculate_directory_digest(dir.path(), ContentDigestMode::Strong)
            .await
            .expect("failed to calculate digest");
        assert_eq!(digest, modified);
    }
}
