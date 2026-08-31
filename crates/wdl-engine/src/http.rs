//! Implementation of a HTTP client.
//!
//! The `DefaultHttpClient` type implements the `HttpClient` trait.
//!
//! The `HttpClient` trait can be used to replace the HTTP implementation for
//! testing.

use std::fs;
use std::ops::Deref;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use cloud_copy::ContentDigest;
use cloud_copy::TransferEvent;
use cloud_copy::UrlExt;
use futures::FutureExt;
use futures::future::BoxFuture;
use tempfile::NamedTempFile;
use tempfile::TempPath;
use tokio::sync::Semaphore;
use tokio::sync::broadcast;
use tracing::debug;
use url::Url;

use crate::Cache;
use crate::CancellationContext;
use crate::config::Config;

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

/// A trait implemented by HTTP clients.
pub trait HttpClient: Send + Sync {
    /// Downloads a file or directory to a temporary path.
    fn download<'a>(
        &'a self,
        source: &'a Url,
        events: Option<broadcast::Sender<TransferEvent>>,
        cancellation: &'a CancellationContext,
        cache: &'a Cache<Url, Location>,
    ) -> BoxFuture<'a, Result<Location>>;

    /// Uploads a local file or directory to a cloud storage URL.
    ///
    /// The destination URL is expected to be content-addressed (meaning
    /// specific to the content being uploaded).
    fn upload<'a>(
        &'a self,
        source: &'a Path,
        destination: &'a Url,
        events: Option<broadcast::Sender<TransferEvent>>,
        cancellation: &'a CancellationContext,
        cache: &'a Cache<Url, ()>,
    ) -> BoxFuture<'a, Result<()>>;

    /// Gets the size of a resource at a given URL.
    ///
    /// Returns `Ok(Some(_))` if the size is known.
    ///
    /// Returns `Ok(None)` if the URL is valid but the size cannot be
    /// determined.
    fn size<'a>(
        &'a self,
        url: &'a Url,
        cancellation: &'a CancellationContext,
        cache: &'a Cache<Url, Option<u64>>,
    ) -> BoxFuture<'a, Result<Option<u64>>>;

    /// Walks a given storage URL as if it were a directory.
    ///
    /// Returns a list of relative paths from the given URL that are in
    /// lexicographical order.
    ///
    /// If the given storage URL is not a directory, an empty list is returned.
    fn walk<'a>(
        &'a self,
        url: &'a Url,
        cancellation: &'a CancellationContext,
        cache: &'a Cache<Url, Arc<[String]>>,
    ) -> BoxFuture<'a, Result<Arc<[String]>>>;

    /// Determines if the given URL exists.
    ///
    /// Returns `Ok(true)` if a HEAD request returns success or if a walk of the
    /// URL returns at least one contained URL.
    fn exists<'a>(
        &'a self,
        url: &'a Url,
        cancellation: &'a CancellationContext,
        cache: &'a Cache<Url, bool>,
    ) -> BoxFuture<'a, Result<bool>>;

    /// Gets the content digest of the resource identified by the given URL.
    ///
    /// Returns `Ok(None)` if the resource has no associated content digest.
    fn digest<'a>(
        &'a self,
        url: &'a Url,
        cancellation: &'a CancellationContext,
        cache: &'a Cache<Url, Option<Arc<ContentDigest>>>,
    ) -> BoxFuture<'a, Result<Option<Arc<ContentDigest>>>>;
}

/// The internal state of the default HTTP client.
struct State {
    /// The configuration for transferring files.
    config: cloud_copy::Config,
    /// The internal HTTP client.
    client: cloud_copy::HttpClient,
    /// The path to the temporary directory for links/copies.
    temp_dir: PathBuf,
    /// Limits the number of concurrent transfers.
    semaphore: Semaphore,
}

/// Implementation of the default HTTP client.
#[derive(Clone)]
pub struct DefaultHttpClient(Arc<State>);

impl DefaultHttpClient {
    /// Constructs a new [`DefaultHttpClient`] with the given configuration.
    pub fn new(config: &Config) -> Result<Self> {
        let cache_dir = config.http.cache_dir()?;

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
            .with_hash_algorithm(config.http.hash_algorithm)
            .with_retries(
                config
                    .http
                    .retries
                    .try_into()
                    .context("invalid HTTP retries")?,
            )
            .with_azure(azure_config)
            .with_s3(s3_config)
            .with_google(google_config)
            .build();

        let client = cloud_copy::HttpClient::new_with_cache(copy_config.clone(), cache_dir);

        Ok(Self(Arc::new(State {
            config: copy_config,
            client,
            temp_dir,
            semaphore: Semaphore::new(config.http.parallelism.into()),
        })))
    }
}

impl HttpClient for DefaultHttpClient {
    fn download<'a>(
        &'a self,
        source: &'a Url,
        events: Option<broadcast::Sender<TransferEvent>>,
        cancellation: &'a CancellationContext,
        cache: &'a Cache<Url, Location>,
    ) -> BoxFuture<'a, Result<Location>> {
        async move {
            // File URLs don't need to be downloaded
            if source.scheme() == "file" {
                return Ok(Location::Path(
                    source
                        .to_file_path()
                        .map_err(|_| anyhow!("invalid file URL `{source}`"))?,
                ));
            }

            let x = cache
                .get_by_ref(source, cancellation, async || {
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

                    // Perform the download (always overwrite the local temp
                    // file)
                    cloud_copy::copy(
                        self.0.config.clone(),
                        self.0.client.clone(),
                        source,
                        &*temp_path,
                        cancellation.first(),
                        events,
                    )
                    .await
                    .with_context(|| {
                        format!("failed to download `{source}`", source = source.display())
                    })
                    .map(|_| Location::Temp(Arc::new(temp_path)))
                })
                .await?;

            match x {
                Some(location) => Ok(location),
                None => bail!(
                    "failed to download `{source}`: the operation was cancelled",
                    source = source.display()
                ),
            }
        }
        .boxed()
    }

    fn upload<'a>(
        &'a self,
        source: &'a Path,
        destination: &'a Url,
        events: Option<broadcast::Sender<TransferEvent>>,
        cancellation: &'a CancellationContext,
        cache: &'a Cache<Url, ()>,
    ) -> BoxFuture<'a, Result<()>> {
        async move {
            match cache
                .get_by_ref(destination, cancellation, async || {
                    // Acquire a permit for the transfer
                    let _permit = self
                        .0
                        .semaphore
                        .acquire()
                        .await
                        .context("failed to acquire permit")?;

                    // Perform the upload (do not overwrite)
                    let mut config = self.0.config.clone();
                    config.set_overwrite(false);
                    match cloud_copy::copy(
                        config,
                        self.0.client.clone(),
                        source,
                        destination,
                        cancellation.first(),
                        events,
                    )
                    .await
                    {
                        Ok(_) | Err(cloud_copy::Error::RemoteDestinationExists(_)) => Ok(()),
                        Err(e) => Err(e).with_context(|| {
                            format!(
                                "failed to upload `{destination}`",
                                destination = destination.display()
                            )
                        }),
                    }
                })
                .await?
            {
                Some(_) => Ok(()),
                None => bail!(
                    "failed to upload `{destination}`: the operation was cancelled",
                    destination = destination.display()
                ),
            }
        }
        .boxed()
    }

    fn size<'a>(
        &'a self,
        url: &'a Url,
        cancellation: &'a CancellationContext,
        cache: &'a Cache<Url, Option<u64>>,
    ) -> BoxFuture<'a, Result<Option<u64>>> {
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

            match cache
                .get_by_ref(url, cancellation, async || {
                    let _permit = self
                        .0
                        .semaphore
                        .acquire()
                        .await
                        .context("failed to acquire permit")?;

                    // Get the size
                    cloud_copy::size(self.0.config.clone(), self.0.client.clone(), url.clone())
                        .await
                        .with_context(|| {
                            format!("failed to retrieve size of `{url}`", url = url.display())
                        })
                })
                .await?
            {
                Some(size) => Ok(size),
                None => bail!(
                    "failed to retrieve size of `{url}`: the operation was cancelled",
                    url = url.display()
                ),
            }
        }
        .boxed()
    }

    fn walk<'a>(
        &'a self,
        url: &'a Url,
        cancellation: &'a CancellationContext,
        cache: &'a Cache<Url, Arc<[String]>>,
    ) -> BoxFuture<'a, Result<Arc<[String]>>> {
        async move {
            match cache
                .get_by_ref(url, cancellation, async || {
                    let _permit = self
                        .0
                        .semaphore
                        .acquire()
                        .await
                        .context("failed to acquire permit")?;

                    let mut entries =
                        cloud_copy::walk(self.0.config.clone(), self.0.client.clone(), url.clone())
                            .await
                            .with_context(|| {
                                format!("failed to walk URL `{url}`", url = url.display())
                            })?;

                    // We return the entries in lexicographical order
                    entries.sort();

                    anyhow::Ok(entries.into())
                })
                .await?
            {
                Some(entries) => Ok(entries),
                None => bail!(
                    "failed to walk URL `{url}`: the operation was cancelled",
                    url = url.display()
                ),
            }
        }
        .boxed()
    }

    fn exists<'a>(
        &'a self,
        url: &'a Url,
        cancellation: &'a CancellationContext,
        cache: &'a Cache<Url, bool>,
    ) -> BoxFuture<'a, Result<bool>> {
        async move {
            // Check for local file
            if url.scheme() == "file" {
                let path = url
                    .to_file_path()
                    .map_err(|_| anyhow!("invalid file URL `{url}`"))?;
                return Ok(path.exists());
            }

            match cache
                .get_by_ref(url, cancellation, async || {
                    let _permit = self
                        .0
                        .semaphore
                        .acquire()
                        .await
                        .context("failed to acquire permit")?;

                    // Determine if the URL exists
                    cloud_copy::exists(self.0.config.clone(), self.0.client.clone(), url.clone())
                        .await
                        .with_context(|| {
                            format!(
                                "failed to determine existence of `{url}`",
                                url = url.display()
                            )
                        })
                })
                .await?
            {
                Some(exists) => Ok(exists),
                None => bail!(
                    "failed to determine existence of `{url}`: the operation was cancelled",
                    url = url.display()
                ),
            }
        }
        .boxed()
    }

    fn digest<'a>(
        &'a self,
        url: &'a Url,
        cancellation: &'a CancellationContext,
        cache: &'a Cache<Url, Option<Arc<ContentDigest>>>,
    ) -> BoxFuture<'a, Result<Option<Arc<ContentDigest>>>> {
        async move {
            match cache
                .get_by_ref(url, cancellation, async || {
                    let permit = self
                        .0
                        .semaphore
                        .acquire()
                        .await
                        .context("failed to acquire permit")?;

                    debug!("retrieving content digest for `{url}`", url = url.display());
                    let digest = cloud_copy::get_content_digest(
                        self.0.config.clone(),
                        self.0.client.clone(),
                        url.clone(),
                    )
                    .await
                    .with_context(|| {
                        format!(
                            "failed to retrieve content digest of `{url}`",
                            url = url.display()
                        )
                    })?;
                    drop(permit);
                    anyhow::Ok(digest.map(Into::into))
                })
                .await?
            {
                Some(digest) => Ok(digest),
                None => bail!(
                    "failed to retrieve content digest of `{url}`: the operation was cancelled",
                    url = url.display()
                ),
            }
        }
        .boxed()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub struct NotImplementedHttpClient;

    impl HttpClient for NotImplementedHttpClient {
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
        ) -> BoxFuture<'a, anyhow::Result<Option<u64>>> {
            unimplemented!()
        }

        fn walk<'a>(
            &'a self,
            _: &'a Url,
            _: &'a CancellationContext,
            _: &'a Cache<Url, Arc<[String]>>,
        ) -> BoxFuture<'a, Result<Arc<[String]>>> {
            unimplemented!()
        }

        fn exists<'a>(
            &'a self,
            _: &'a Url,
            _: &'a CancellationContext,
            _: &'a Cache<Url, bool>,
        ) -> BoxFuture<'a, Result<bool>> {
            unimplemented!()
        }

        fn digest<'a>(
            &'a self,
            _: &'a Url,
            _: &'a CancellationContext,
            _: &'a Cache<Url, Option<Arc<ContentDigest>>>,
        ) -> BoxFuture<'a, Result<Option<Arc<ContentDigest>>>> {
            unimplemented!()
        }
    }
}
