//! Implementation of remote file downloads over HTTP.

use std::collections::HashMap;
use std::ops::Deref;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Context;
use anyhow::Error;
use anyhow::Result;
use anyhow::bail;
use futures::FutureExt;
use futures::StreamExt;
use futures::future::BoxFuture;
use http_cache_stream_reqwest::Cache;
use http_cache_stream_reqwest::CacheStorage;
use http_cache_stream_reqwest::storage::DefaultCacheStorage;
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use reqwest_middleware::ClientWithMiddleware;
use tempfile::NamedTempFile;
use tempfile::TempPath;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::io::BufWriter;
use tokio::sync::Notify;
use tracing::debug;
use tracing::info;
use url::Url;

/// The default cache subdirectory that is appended to the system cache
/// directory.
const DEFAULT_CACHE_SUBDIR: &str = "wdl";

/// A trait implemented by types responsible for downloading remote files over
/// HTTP for evaluation.
pub trait Downloader {
    /// Downloads a file from a given URL.
    ///
    /// Returns `Ok(None)` if the provided string is not a valid URL.
    ///
    /// Returns `Ok(Some)` if the file was downloaded successfully.
    fn download<'a, 'b, 'c>(
        &'a self,
        url: &'b str,
    ) -> BoxFuture<'c, Result<Option<Location>, Arc<Error>>>
    where
        'a: 'c,
        'b: 'c,
        Self: 'c;
}

/// Represents a location of a downloaded file.
#[derive(Debug, Clone)]
pub enum Location {
    /// The file exists as a temporary file.
    ///
    /// This is used whenever a response body cannot be cached.
    Temp(Arc<TempPath>),
    /// The location is a path to a non-temporary file.
    Path(PathBuf),
    /// The location is a shared path to a non-temporary file.
    SharedPath(Arc<PathBuf>),
}

impl Deref for Location {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Temp(p) => p,
            Self::Path(p) => p,
            Self::SharedPath(p) => p,
        }
    }
}

/// Represents the status of a download.
pub enum Status {
    /// The requested resource is currently being downloaded.
    ///
    /// The specified `Notify` will be notified when the download completes.
    Downloading(Arc<Notify>),
    /// The requested resource has already been downloaded.
    Downloaded(Result<Location, Arc<anyhow::Error>>),
}

/// Responsible for downloading and caching remote files using HTTP.
///
/// The downloader can be cheaply cloned.
#[derive(Clone)]
pub struct HttpDownloader {
    /// The underlying HTTP client.
    client: ClientWithMiddleware,
    /// The HTTP cache shared with the client.
    cache: Arc<Cache<DefaultCacheStorage>>,
    /// Stores the status of downloads by URL.
    downloads: Arc<Mutex<HashMap<Url, Status>>>,
}

impl HttpDownloader {
    /// Constructs a new HTTP downloader using the system cache directory.
    pub fn new() -> Result<Self> {
        Ok(Self::new_with_cache(
            dirs::cache_dir()
                .context("failed to determine system cache directory")?
                .join(DEFAULT_CACHE_SUBDIR),
        ))
    }

    /// Constructs a new downloader with the given cache directory.
    pub fn new_with_cache(cache_dir: impl Into<PathBuf>) -> Self {
        let cache_dir = cache_dir.into();

        info!(
            "using HTTP download cache directory `{dir}`",
            dir = cache_dir.display()
        );

        let cache = Arc::new(Cache::new(DefaultCacheStorage::new(cache_dir)));

        Self {
            client: ClientBuilder::new(Client::new())
                .with_arc(cache.clone())
                .build(),
            cache,
            downloads: Default::default(),
        }
    }

    /// Gets the file at the given URL.
    ///
    /// Returns the file's local location upon success.
    async fn get(&self, url: &Url) -> Result<Location> {
        // TODO: add auth tokens for s3/az/gs requests

        // Perform the download
        let response = self
            .client
            .get(url.as_str())
            .send()
            .await
            .with_context(|| format!("failed to download `{url}`"))?;

        let status = response.status();
        if !status.is_success() {
            if let Ok(text) = response.text().await {
                debug!("response from get of `{url}` was `{text}`");
            }

            bail!("failed to download `{url}`: server responded with status {status}");
        }

        if let Some(digest) = response
            .headers()
            .get(http_cache_stream_reqwest::X_CACHE_DIGEST)
        {
            let path = self
                .cache
                .storage()
                .body_path(digest.to_str().expect("key should be UTF-8"));

            debug!(
                "`{url}` was previously downloaded to `{path}`",
                path = path.display()
            );

            // The file is in the cache
            return Ok(Location::SharedPath(path.into()));
        }

        // The file is not in the cache, we need to download it to a temporary path
        let (file, path) = NamedTempFile::new()
            .context("failed to create temporary file")?
            .into_parts();

        debug!(
            "response body for `{url}` was not present in cache: downloading to temporary file \
             `{path}`",
            path = path.display()
        );

        let mut stream = response.bytes_stream();
        let mut writer = BufWriter::new(fs::File::from(file));

        while let Some(bytes) = stream.next().await {
            let bytes =
                bytes.with_context(|| format!("failed to read response body from `{url}`"))?;
            writer.write_all(&bytes).await.with_context(|| {
                format!(
                    "failed to write to temporary file `{path}`",
                    path = path.display()
                )
            })?;
        }

        Ok(Location::Temp(path.into()))
    }
}

impl Downloader for HttpDownloader {
    fn download<'a, 'b, 'c>(
        &'a self,
        url: &'b str,
    ) -> BoxFuture<'c, Result<Option<Location>, Arc<Error>>>
    where
        'a: 'c,
        'b: 'c,
        Self: 'c,
    {
        async move {
            // If the given string is not a URL, return `None`
            let url: Url = match url.parse() {
                Ok(url) => url,
                Err(_) => return Ok(None),
            };

            match url.scheme() {
                "file" => {
                    // If it can be converted to a path, return the path; otherwise `None`
                    return Ok(match url.to_file_path() {
                        Ok(p) => Some(Location::Path(p)),
                        Err(_) => None,
                    });
                }
                "http" | "https" => {}
                // TODO: support s3/az/gs URI here, including downloading of blobs under a shared
                // prefix
                _ => return Ok(None),
            }

            // This loop exists so that all requests to download the same URL will block
            // waiting for a notification that the download has completed.
            // When the notification is received, the lookup into the downloads is retried
            let mut retried = false;
            loop {
                // Scope to ensure the mutex guard is not visible to the await point
                let notify = {
                    let mut downloads = self.downloads.lock().expect("failed to lock downloads");
                    match downloads.get(&url) {
                        Some(Status::Downloading(notify)) => {
                            assert!(
                                !retried,
                                "file should not be downloading again after a notification"
                            );

                            notify.clone()
                        }
                        Some(Status::Downloaded(r)) => {
                            return r.clone().map(Into::into);
                        }
                        None => {
                            assert!(
                                !retried,
                                "file should not be downloaded again after a notification"
                            );

                            // Insert an entry and break out of the loop to download
                            downloads.insert(url.clone(), Status::Downloading(Arc::default()));
                            break;
                        }
                    }
                };

                notify.notified().await;
                retried = true;
                continue;
            }

            // TODO: progress indicator?
            info!("downloading `{url}` to the cache");

            // Perform the download
            let res = self.get(&url).await.map_err(Arc::from);
            let notify = {
                let mut downloads = self.downloads.lock().expect("failed to lock downloads");
                match std::mem::replace(
                    downloads.get_mut(&url).expect("should have status"),
                    Status::Downloaded(res.clone()),
                ) {
                    Status::Downloading(notify) => notify,
                    _ => panic!("file should be downloading"),
                }
            };

            notify.notify_waiters();
            res.map(Some)
        }
        .boxed()
    }
}
