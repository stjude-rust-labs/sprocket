//! Implementation of remote file downloads over HTTP.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Context;
use anyhow::Error;
use anyhow::Result;
use anyhow::anyhow;
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
use tokio::sync::Semaphore;
use tokio::time::Duration;
use tokio::time::sleep;
use tracing::debug;
use tracing::error;
use tracing::info;
use url::Url;

use crate::config::Config;
use crate::config::DEFAULT_MAX_CONCURRENT_DOWNLOADS;

mod azure;
mod google;
mod s3;

/// The default cache subdirectory that is appended to the system cache
/// directory.
const DEFAULT_CACHE_SUBDIR: &str = "wdl";

/// Maximum number of download attempts.
const MAX_DOWNLOAD_ATTEMPTS: u32 = 3;
/// Initial delay before the first retry.
const INITIAL_RETRY_DELAY: Duration = Duration::from_secs(1);

/// Rewrites a given URL to a `http`, `https`, or `file` schemed URL.
///
/// Applies any cloud storage authentication to the URL.
pub fn rewrite_url<'a>(config: &Config, url: Cow<'a, Url>) -> Result<Cow<'a, Url>> {
    let url: Cow<'_, Url> = match url.scheme() {
        "file" => return Ok(url),
        "http" | "https" => url,
        "az" => Cow::Owned(azure::rewrite_url(&url)?),
        "s3" => Cow::Owned(s3::rewrite_url(&config.storage.s3, &url)?),
        "gs" => Cow::Owned(google::rewrite_url(&url)?),
        _ => bail!("unsupported URL `{url}`"),
    };

    // Attempt to apply auth for Azure storage
    let (matched, url) = azure::apply_auth(&config.storage.azure, url);
    if matched {
        return Ok(url);
    }

    // Attempt to apply auth for S3 storage
    let (matched, url) = s3::apply_auth(&config.storage.s3, url);
    if matched {
        return Ok(url);
    }

    // Finally, attempt to apply auth for Google Cloud Storage
    let (matched, url) = google::apply_auth(&config.storage.google, url);
    if matched {
        return Ok(url);
    }

    Ok(url)
}

/// A trait implemented by types responsible for downloading remote files over
/// HTTP for evaluation.
pub trait Downloader: Send + Sync {
    /// Downloads a file from a given URL.
    ///
    /// Returns the location of the downloaded file.
    fn download<'a, 'b, 'c>(
        &'a self,
        url: &'b Url,
    ) -> BoxFuture<'c, Result<Location<'static>, Arc<Error>>>
    where
        'a: 'c,
        'b: 'c,
        Self: 'c;

    /// Gets the size of a resource at a given URL.
    ///
    /// Returns `Ok(Some(_))` if the size is known.
    ///
    /// Returns `Ok(None)` if the URL is valid but the size cannot be
    /// determined.
    fn size<'a, 'b, 'c>(&'a self, url: &'b Url) -> BoxFuture<'c, Result<Option<u64>>>
    where
        'a: 'c,
        'b: 'c,
        Self: 'c;
}

/// Represents a location of a downloaded file.
#[derive(Debug, Clone)]
pub enum Location<'a> {
    /// The file exists as a temporary file.
    ///
    /// This is used whenever a response body cannot be cached.
    Temp(Arc<TempPath>),
    /// The location is a path to a non-temporary file.
    Path(Cow<'a, Path>),
}

impl Location<'_> {
    /// Converts the location into an owned representation.
    pub fn into_owned(self) -> Location<'static> {
        match self {
            Self::Temp(path) => Location::Temp(path),
            Self::Path(path) => Location::Path(Cow::Owned(path.into_owned())),
        }
    }
}

impl Deref for Location<'_> {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Temp(p) => p,
            Self::Path(p) => p,
        }
    }
}

impl AsRef<Path> for Location<'_> {
    fn as_ref(&self) -> &Path {
        match self {
            Self::Temp(path) => path.as_ref(),
            Self::Path(cow) => cow.as_ref(),
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
    Downloaded(Result<Location<'static>, Arc<anyhow::Error>>),
}

/// Helper for displaying URLs in log messages.
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
    /// Limits the number of concurrent downloads.
    semaphore: Arc<Semaphore>,
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

        let max_downloads = config
            .http
            .max_concurrent_downloads
            .unwrap_or(DEFAULT_MAX_CONCURRENT_DOWNLOADS) as usize;

        debug!("maximum concurrent downloads set to {max_downloads}");

        Ok(Self {
            config,
            client: ClientBuilder::new(Client::new())
                .with_arc(cache.clone())
                .build(),
            cache,
            downloads: Default::default(),
            semaphore: Arc::new(Semaphore::new(max_downloads)),
        })
    }

    /// Gets the file at the given URL.
    ///
    /// Returns the file's local location upon success.
    async fn get(&self, url: &Url) -> Result<Location<'static>> {
        // TODO: progress indicator?
        debug!("sending GET for `{url}`", url = DisplayUrl(url));

        // Perform the download
        let response = self.client.get(url.as_str()).send().await?;

        let status = response.status();
        if !status.is_success() {
            if let Ok(text) = response.text().await {
                debug!(
                    "response from GET of `{url}` was `{text}`",
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
            return Ok(Location::Path(path.into()));
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
}

impl Downloader for HttpDownloader {
    fn download<'a, 'b, 'c>(
        &'a self,
        url: &'b Url,
    ) -> BoxFuture<'c, Result<Location<'static>, Arc<Error>>>
    where
        'a: 'c,
        'b: 'c,
        Self: 'c,
    {
        async move {
            let url = rewrite_url(&self.config, Cow::Borrowed(url))?;

            // File URLs don't need to be downloaded
            if url.scheme() == "file" {
                return Ok(Location::Path(
                    url.to_file_path()
                        .map_err(|_| anyhow!("invalid file URL `{url}`"))?
                        .into(),
                ));
            }

            // TODO: support downloading "directories" for cloud storage URLs

            // This loop exists so that all requests to download the same URL will block
            // waiting for a notification that the download has completed.
            // When the notification is received, the lookup into the downloads is retried
            let mut waited = false;
            loop {
                let status_check_result = {
                    let mut downloads = self.downloads.lock().expect("failed to lock downloads");
                    match downloads.get(&url) {
                        Some(Status::Downloading(notify)) => {
                            assert!(
                                !waited,
                                "file should not be downloading again after a notification"
                            );
                            Some(notify.clone())
                        }
                        Some(Status::Downloaded(r)) => {
                            return r.clone();
                        }
                        None => {
                            // not downloading, not downloaded. mark for retry/download
                            downloads.insert(
                                url.clone().into_owned(),
                                Status::Downloading(Arc::new(Notify::new())),
                            );
                            None
                        }
                    }
                };

                if let Some(notify) = status_check_result {
                    notify.notified().await;
                    waited = true;
                    continue;
                } else {
                    break;
                }
            }

            let mut attempt_counter = 0;
            let result = 'retry_loop: loop {
                let attempt = attempt_counter;

                let permit = self
                    .semaphore
                    .acquire()
                    .await
                    .expect("semaphore should not be closed");

                match self.get(&url).await {
                    Ok(location) => {
                        break 'retry_loop Ok(location);
                    }
                    Err(e) => {
                        let current_error = Arc::new(e.context(format!(
                            "download attempt {} failed for `{url}`",
                            attempt + 1
                        )));

                        // if it was the last attempt, return the error
                        if attempt == MAX_DOWNLOAD_ATTEMPTS - 1 {
                            error!(
                                "download failed after {} attempts for `{url}`: {}",
                                MAX_DOWNLOAD_ATTEMPTS, current_error
                            );
                            break 'retry_loop Err(current_error);
                        }

                        // backoff and retry
                        let delay_secs =
                            INITIAL_RETRY_DELAY.as_secs_f64() * 2.0f64.powi(attempt as i32);
                        let delay = Duration::from_secs_f64(delay_secs);
                        info!(
                            "backing off for {:.2}s before retry attempt {}/{} for `{url}`",
                            delay_secs,
                            attempt + 1,
                            MAX_DOWNLOAD_ATTEMPTS,
                            url = DisplayUrl(&url)
                        );

                        drop(permit);
                        sleep(delay).await;

                        attempt_counter += 1;
                        // permit will be re-acquired at the start of the next
                        // iteration
                    }
                }
                // permit is implicitly dropped here
            };

            let notify = {
                let mut downloads = self.downloads.lock().expect("failed to lock downloads");
                match downloads.insert(url.clone().into_owned(), Status::Downloaded(result.clone()))
                {
                    Some(Status::Downloading(notify)) => notify,
                    _ => panic!(
                        "expected to find a downloading status for `{url}`",
                        url = url
                    ),
                }
            };

            notify.notify_waiters();
            result
        }
        .boxed()
    }

    /// Gets the size of a resource at a given URL.
    ///
    /// Returns `Ok(Some(_))` if the size is known.
    ///
    /// Returns `Ok(None)` if the URL is valid but the size cannot be
    /// determined.
    fn size<'a, 'b, 'c>(&'a self, url: &'b Url) -> BoxFuture<'c, Result<Option<u64>>>
    where
        'a: 'c,
        'b: 'c,
        Self: 'c,
    {
        async move {
            let url = rewrite_url(&self.config, Cow::Borrowed(url))?;

            // Check for local file
            if url.scheme() == "file" {
                let path = url
                    .to_file_path()
                    .map_err(|_| anyhow!("invalid file URL `{url}`"))?;
                let metadata = path.metadata().with_context(|| {
                    format!(
                        "cannot retrieve metadata for file `{path}`",
                        path = path.display()
                    )
                })?;
                return Ok(Some(metadata.len()));
            }

            // Perform the HEAD request
            debug!("sending HEAD for `{url}`", url = DisplayUrl(&url));
            let response = self.client.head(url.as_str()).send().await?;

            let status = response.status();
            if !status.is_success() {
                if let Ok(text) = response.text().await {
                    debug!(
                        "response from HEAD of `{url}` was `{text}`",
                        url = DisplayUrl(&url)
                    );
                }

                bail!("server responded with status {status}");
            }

            Ok(response
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok().and_then(|v| v.parse().ok())))
        }
        .boxed()
    }
}
