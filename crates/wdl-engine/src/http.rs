//! Implementation of remote file downloads and uploads over HTTP.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::ops::Deref;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread::available_parallelism;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use cloud_copy::HttpClient;
use cloud_copy::TransferEvent;
use cloud_copy::UrlExt;
use futures::FutureExt;
use futures::future::BoxFuture;
use tempfile::NamedTempFile;
use tempfile::TempPath;
use tokio::sync::OnceCell;
use tokio::sync::Semaphore;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::Level;
use tracing::debug;
use url::Url;

use crate::config::Config;

/// The default cache subdirectory that is appended to the system cache
/// directory.
const DEFAULT_CACHE_SUBDIR: &str = "wdl";

/// Represents a location of a downloaded file.
#[derive(Debug, Clone)]
pub enum Location {
    /// The location is a temporary file.
    Temp(Arc<TempPath>),
    /// The location is a path to a non-temporary file.
    Path(PathBuf),
}

impl Deref for Location {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Temp(p) => p,
            Self::Path(p) => p,
        }
    }
}

impl AsRef<Path> for Location {
    fn as_ref(&self) -> &Path {
        match self {
            Self::Temp(path) => path.as_ref(),
            Self::Path(cow) => cow.as_ref(),
        }
    }
}

/// Represents a file transferer.
pub trait Transferer: Send + Sync {
    /// Downloads a file or directory to a temporary path.
    fn download<'a>(&'a self, source: &'a Url) -> BoxFuture<'a, Result<Location>>;

    /// Uploads a local file or directory to a cloud storage URL.
    ///
    /// The destination URL is expected to be content-addressed (meaning
    /// specific to the content being uploaded).
    ///
    /// Returns the destination URL with any Azure authentication applied.
    fn upload<'a>(&'a self, source: &'a Path, destination: &'a Url) -> BoxFuture<'a, Result<()>>;

    /// Gets the size of a resource at a given URL.
    ///
    /// Returns `Ok(Some(_))` if the size is known.
    ///
    /// Returns `Ok(None)` if the URL is valid but the size cannot be
    /// determined.
    fn size<'a>(&'a self, url: &'a Url) -> BoxFuture<'a, Result<Option<u64>>>;

    /// Walks a given storage URL as if it were a directory.
    ///
    /// Returns a list of relative paths from the given URL.
    ///
    /// If the given storage URL is not a directory, an empty list is returned.
    fn walk<'a>(&'a self, url: &'a Url) -> BoxFuture<'a, Result<Arc<[String]>>>;

    /// Determines if the given URL exists.
    ///
    /// Returns `Ok(true)` if a HEAD request returns success or if a walk of the
    /// URL returns at least one contained URL.
    fn exists<'a>(&'a self, url: &'a Url) -> BoxFuture<'a, Result<bool>>;
}

/// Used to cache results of transferer operations.
#[derive(Default)]
struct Cache {
    /// Stores the results of downloading files.
    downloads: HashMap<Url, Arc<OnceCell<Location>>>,
    /// Stores the results of uploading files.
    uploads: HashMap<Url, Arc<OnceCell<()>>>,
    /// Stores the results of retrieving file sizes.
    sizes: HashMap<Url, Arc<OnceCell<Option<u64>>>>,
    /// Stores the results of walking a URL.
    walks: HashMap<Url, Arc<OnceCell<Arc<[String]>>>>,
    /// Stores the results of checking for URL existence.
    exists: HashMap<Url, Arc<OnceCell<bool>>>,
}

/// Represents the internal state of `HttpTransferer`.
struct HttpTransfererInner {
    /// The configuration for transferring files.
    copy_config: cloud_copy::Config,
    /// The HTTP client to use.
    client: HttpClient,
    /// The cached results of transferer operations.
    cache: Mutex<Cache>,
    /// The path to the temporary directory for links/copies.
    temp_dir: PathBuf,
    /// The cancellation token for canceling transfers.
    cancel: CancellationToken,
    /// The events sender to use for transfer events.
    events: Option<broadcast::Sender<TransferEvent>>,
    /// Limits the number of concurrent transfers.
    semaphore: Semaphore,
}

/// Implementation of a file transferer that uses HTTP.
#[derive(Clone)]
pub struct HttpTransferer(Arc<HttpTransfererInner>);

impl HttpTransferer {
    /// Constructs a new HTTP transferer with the given configuration.
    pub fn new(
        config: Arc<Config>,
        cancel: CancellationToken,
        events: Option<broadcast::Sender<TransferEvent>>,
    ) -> Result<Self> {
        let cache_dir: Cow<'_, Path> = match &config.http.cache_dir {
            Some(dir) => dir.into(),
            None => dirs::cache_dir()
                .context("failed to determine system cache directory")?
                .join(DEFAULT_CACHE_SUBDIR)
                .into(),
        };

        let temp_dir = cache_dir.join("tmp");
        fs::create_dir_all(&temp_dir).with_context(|| {
            format!(
                "failed to create directory `{path}`",
                path = temp_dir.display()
            )
        })?;

        let azure_config = config
            .storage
            .azure
            .auth
            .as_ref()
            .map(|auth| {
                cloud_copy::AzureConfig::default()
                    .with_auth(auth.account_name.clone(), auth.access_key.inner().clone())
            })
            .unwrap_or_default();

        let s3_config = config
            .storage
            .s3
            .auth
            .as_ref()
            .map(|auth| {
                cloud_copy::S3Config::default().with_auth(
                    auth.access_key_id.clone(),
                    auth.secret_access_key.inner().clone(),
                )
            })
            .unwrap_or_default()
            .with_maybe_region(config.storage.s3.region.clone());

        let google_config = config
            .storage
            .google
            .auth
            .as_ref()
            .map(|auth| {
                cloud_copy::GoogleConfig::default()
                    .with_auth(auth.access_key.clone(), auth.secret.inner().clone())
            })
            .unwrap_or_default();

        let copy_config = cloud_copy::Config::builder()
            .with_link_to_cache(true)
            .with_overwrite(true)
            .with_maybe_retries(config.http.retries)
            .with_azure(azure_config)
            .with_s3(s3_config)
            .with_google(google_config)
            .build();

        let client = HttpClient::new_with_cache(copy_config.clone(), cache_dir);

        let semaphore = Semaphore::new(
            config
                .http
                .parallelism
                .unwrap_or_else(|| available_parallelism().map(Into::into).unwrap_or(1)),
        );

        Ok(Self(Arc::new(HttpTransfererInner {
            copy_config,
            client,
            cache: Default::default(),
            temp_dir,
            cancel,
            events,
            semaphore,
        })))
    }
}

impl Transferer for HttpTransferer {
    fn download<'a>(&'a self, source: &'a Url) -> BoxFuture<'a, Result<Location>> {
        async move {
            // File URLs don't need to be downloaded
            if source.scheme() == "file" {
                return Ok(Location::Path(
                    source
                        .to_file_path()
                        .map_err(|_| anyhow!("invalid file URL `{source}`"))?,
                ));
            }

            let download = {
                let mut cache = self.0.cache.lock().expect("failed to lock cache");
                cache.downloads.entry(source.clone()).or_default().clone()
            };

            // Get an existing result or initialize a new one exactly once
            Ok(download
                .get_or_try_init(|| async {
                    {
                        // Acquire a permit for the transfer
                        let _permit = self
                            .0
                            .semaphore
                            .acquire()
                            .await
                            .context("failed to acquire permit")?;

                        // Create a temporary path to where the download will go
                        let temp_path = NamedTempFile::new_in(&self.0.temp_dir)
                            .context("failed to create temporary file")?
                            .into_temp_path();

                        // Perform the download (always overwrite the local temp file)
                        cloud_copy::copy(
                            self.0.copy_config.clone(),
                            self.0.client.clone(),
                            source,
                            &*temp_path,
                            self.0.cancel.clone(),
                            self.0.events.clone(),
                        )
                        .await
                        .with_context(|| {
                            format!("failed to download `{source}`", source = source.display())
                        })
                        .map(|_| Location::Temp(Arc::new(temp_path)))
                    }
                })
                .await?
                .clone())
        }
        .boxed()
    }

    fn upload<'a>(&'a self, source: &'a Path, destination: &'a Url) -> BoxFuture<'a, Result<()>> {
        async move {
            let upload = {
                let mut cache = self.0.cache.lock().expect("failed to lock cache");
                cache
                    .uploads
                    .entry(destination.clone())
                    .or_default()
                    .clone()
            };

            // Get an existing result or initialize a new one exactly once
            upload
                .get_or_try_init(|| async {
                    {
                        // Acquire a permit for the transfer
                        let _permit = self
                            .0
                            .semaphore
                            .acquire()
                            .await
                            .context("failed to acquire permit")?;

                        // Perform the upload (do not overwrite)
                        let mut config = self.0.copy_config.clone();
                        config.set_overwrite(false);
                        match cloud_copy::copy(
                            config,
                            self.0.client.clone(),
                            source,
                            destination,
                            self.0.cancel.clone(),
                            self.0.events.clone(),
                        )
                        .await
                        {
                            Ok(_) | Err(cloud_copy::Error::RemoteDestinationExists(_)) => {
                                anyhow::Ok(())
                            }
                            Err(e) => Err(e.into()),
                        }
                    }
                })
                .await?;

            Ok(())
        }
        .boxed()
    }

    fn size<'a>(&'a self, url: &'a Url) -> BoxFuture<'a, Result<Option<u64>>> {
        async move {
            // Check for local file
            if url.scheme() == "file" {
                let path = url
                    .to_file_path()
                    .map_err(|_| anyhow!("invalid file URL `{url}`"))?;
                let metadata = path.metadata().with_context(|| {
                    format!(
                        "failed to retrieve metadata for file `{path}`",
                        path = path.display()
                    )
                })?;
                return Ok(Some(metadata.len()));
            }

            let size = {
                let mut cache = self.0.cache.lock().expect("failed to lock cache");
                cache.sizes.entry(url.clone()).or_default().clone()
            };

            // Get an existing result or initialize a new one exactly once
            Ok(*size
                .get_or_try_init(|| async {
                    let permit = self
                        .0
                        .semaphore
                        .acquire()
                        .await
                        .context("failed to acquire permit")?;

                    // Perform the HEAD request
                    debug!("sending HEAD for `{url}`", url = url.display());
                    let response =
                        self.0
                            .client
                            .head(url.as_str())
                            .send()
                            .await
                            .with_context(|| {
                                format!("failed to retrieve size of `{url}`", url = url.display())
                            })?;

                    drop(permit);

                    let status = response.status();
                    if !status.is_success() {
                        if tracing::enabled!(Level::DEBUG)
                            && let Ok(text) = response.text().await
                        {
                            debug!(
                                "response from HEAD of `{url}` was `{text}`",
                                url = url.display()
                            );
                        }

                        bail!(
                            "failed to retrieve size of `{url}`: server responded with status \
                             {status}",
                            url = url.display()
                        );
                    }

                    Ok(response
                        .headers()
                        .get("content-length")
                        .and_then(|v| v.to_str().ok().and_then(|v| v.parse().ok())))
                })
                .await?)
        }
        .boxed()
    }

    fn walk<'a>(&'a self, url: &'a Url) -> BoxFuture<'a, Result<Arc<[String]>>> {
        async move {
            let walk = {
                let mut cache = self.0.cache.lock().expect("failed to lock cache");
                cache.walks.entry(url.clone()).or_default().clone()
            };

            // Get an existing result or initialize a new one exactly once
            Ok(walk
                .get_or_try_init(|| async {
                    let _permit = self
                        .0
                        .semaphore
                        .acquire()
                        .await
                        .context("failed to acquire permit")?;

                    anyhow::Ok(
                        cloud_copy::walk(
                            self.0.copy_config.clone(),
                            self.0.client.clone(),
                            url.clone(),
                        )
                        .await
                        .with_context(|| format!("failed to walk URL `{url}`"))?
                        .into(),
                    )
                })
                .await?
                .clone())
        }
        .boxed()
    }

    fn exists<'a>(&'a self, url: &'a Url) -> BoxFuture<'a, Result<bool>> {
        async move {
            // Check for local file
            if url.scheme() == "file" {
                let path = url
                    .to_file_path()
                    .map_err(|_| anyhow!("invalid file URL `{url}`"))?;
                return Ok(path.exists());
            }

            let exists = {
                let mut cache = self.0.cache.lock().expect("failed to lock cache");
                cache.exists.entry(url.clone()).or_default().clone()
            };

            // Get an existing result or initialize a new one exactly once
            Ok(*exists
                .get_or_try_init(|| async {
                    let permit = self
                        .0
                        .semaphore
                        .acquire()
                        .await
                        .context("failed to acquire permit")?;

                    // Perform the HEAD request
                    debug!("sending HEAD for `{url}`", url = url.display());
                    let response =
                        self.0
                            .client
                            .head(url.as_str())
                            .send()
                            .await
                            .with_context(|| {
                                format!(
                                    "failed to determine existence of `{url}`",
                                    url = url.display()
                                )
                            })?;

                    drop(permit);

                    let status = response.status();
                    if !status.is_success() {
                        // The URL might be a "directory"; check to see if a walk produces at least
                        // one URL
                        if status.as_u16() == 404 {
                            return Ok(!self.walk(url).await?.is_empty());
                        }

                        bail!(
                            "failed to check existence of `{url}`: server responded with status \
                             {status}",
                            url = url.display()
                        );
                    }

                    Ok(true)
                })
                .await?)
        }
        .boxed()
    }
}
