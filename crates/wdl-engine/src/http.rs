//! Implementation of remote file downloads over HTTP.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
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

use crate::config::Config;

mod azure;
mod google;
mod s3;

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
    /// The engine evaluation configuration.
    config: Arc<Config>,
    /// The underlying HTTP client.
    client: ClientWithMiddleware,
    /// The HTTP cache shared with the client.
    cache: Arc<Cache<DefaultCacheStorage>>,
    /// Stores the status of downloads by URL.
    downloads: Arc<Mutex<HashMap<Url, Status>>>,
}

impl HttpDownloader {
    /// Constructs a new HTTP downloader with the given configuration.
    pub fn new(config: Arc<Config>) -> Result<Self> {
        let cache_dir: Cow<'_, Path> = match &config.http.cache {
            Some(dir) => dir.into(),
            None => dirs::cache_dir()
                .context("failed to determine system cache directory")?
                .join(DEFAULT_CACHE_SUBDIR)
                .into(),
        };

        info!(
            "using HTTP download cache directory `{dir}`",
            dir = cache_dir.display()
        );

        let cache = Arc::new(Cache::new(DefaultCacheStorage::new(cache_dir)));

        Ok(Self {
            config,
            client: ClientBuilder::new(Client::new())
                .with_arc(cache.clone())
                .build(),
            cache,
            downloads: Default::default(),
        })
    }

    /// Gets the file at the given URL.
    ///
    /// Returns the file's local location upon success.
    async fn get(&self, url: &Url) -> Result<Location> {
        struct DisplayUrl<'a>(&'a Url);

        impl fmt::Display for DisplayUrl<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                // Only write the scheme, host, and path so that potential authentication
                // information doesn't end up in the log
                write!(
                    f,
                    "{scheme}://{host}{path}",
                    scheme = self.0.scheme(),
                    host = self.0.host_str().unwrap_or(""),
                    path = self.0.path()
                )
            }
        }

        // TODO: progress indicator?
        info!("downloading `{url}`", url = DisplayUrl(url));

        // Perform the download
        let response = self.client.get(url.as_str()).send().await?;

        let status = response.status();
        if !status.is_success() {
            if let Ok(text) = response.text().await {
                debug!(
                    "response from get of `{url}` was `{text}`",
                    url = DisplayUrl(url)
                );
            }

            bail!("server responded with status {status}");
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
                url = DisplayUrl(url),
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
            url = DisplayUrl(url),
            path = path.display()
        );

        let mut stream = response.bytes_stream();
        let mut writer = BufWriter::new(fs::File::from(file));

        while let Some(bytes) = stream.next().await {
            let bytes = bytes.with_context(|| {
                format!(
                    "failed to read response body from `{url}`",
                    url = DisplayUrl(url)
                )
            })?;
            writer.write_all(&bytes).await.with_context(|| {
                format!(
                    "failed to write to temporary file `{path}`",
                    path = path.display()
                )
            })?;
        }

        Ok(Location::Temp(path.into()))
    }

    /// Applies authentication to the given URL.
    fn apply_auth(&self, url: &mut Url) {
        // Attempt to apply auth for Azure storage
        if azure::apply_auth(&self.config.storage.azure, url) {
            return;
        }

        // Attempt to apply auth for S3 storage
        if s3::apply_auth(&self.config.storage.s3, url) {
            return;
        }

        // Finally, attempt to apply auth for Google Cloud Storage
        google::apply_auth(&self.config.storage.google, url);
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

            let mut url = match url.scheme() {
                "file" => {
                    // If it can be converted to a path, return the path; otherwise `None`
                    return Ok(match url.to_file_path() {
                        Ok(p) => Some(Location::Path(p)),
                        Err(_) => None,
                    });
                }
                "http" | "https" => url,
                "az" => azure::rewrite_url(&url)?,
                "s3" => s3::rewrite_url(&self.config.storage.s3, &url)?,
                "gs" => google::rewrite_url(&url)?,
                _ => return Ok(None),
            };

            // TODO: support downloading "directories" for cloud storage URLs

            // Apply any authentication to the URL based on configuration
            self.apply_auth(&mut url);

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
