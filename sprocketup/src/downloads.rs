//! Utilities for handling file downloads.

use std::io::Read;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::Context;
use anyhow::bail;
use bytes::Bytes;
use futures::StreamExt;
use futures::future::TryFutureExt;
use indicatif::MultiProgress;
use indicatif::ProgressBar;
use indicatif::ProgressBarIter;
use indicatif::ProgressDrawTarget;
use indicatif::ProgressStyle;
use reqwest::Client;
use reqwest::Response;
use sha2::Digest;
use sha2::Sha256;
use sha2::digest::FixedOutput;
use url::Url;

/// A remote file downloader.
pub struct Downloader {
    /// A collection of active progress bars.
    progress: MultiProgress,
}

impl Downloader {
    /// Create a new `Downloader`.
    pub fn new() -> Self {
        Self {
            progress: MultiProgress::with_draw_target(ProgressDrawTarget::stderr()),
        }
    }

    /// Create a new [`Download`] associated with this `Downloader`.
    pub fn download(&self, url: Url) -> Download<'_, Bytes> {
        Download::new(self, url)
    }
}

/// A remote file download.
pub struct Download<'a, Out = Bytes> {
    /// The downloader this originated from.
    downloader: &'a Downloader,
    /// The URL of the resource to download.
    url: Url,
    /// Optional identifier to use in progress bars.
    identifier: Option<String>,
    /// Optional path to write the received file to.
    destination: Option<PathBuf>,
    /// Optional hash to verify against the received file.
    hash: Option<String>,
    /// Marker for the output.
    _phantom: PhantomData<Out>,
}

impl<'a> Download<'a, Bytes> {
    /// Create a new `Download` for the given URL.
    fn new(downloader: &'a Downloader, url: Url) -> Self {
        Self {
            downloader,
            url,
            identifier: None,
            destination: None,
            hash: None,
            _phantom: PhantomData,
        }
    }

    /// Set a destination path to write the file to.
    pub fn destination(self, destination: impl Into<PathBuf>) -> Download<'a, ()> {
        Download {
            downloader: self.downloader,
            url: self.url,
            identifier: self.identifier,
            destination: Some(destination.into()),
            hash: self.hash,
            _phantom: PhantomData,
        }
    }
}

impl<Out> Download<'_, Out> {
    /// Set a name for the download, used in the progress bar.
    pub fn identifier(mut self, identifier: impl Into<String>) -> Self {
        self.identifier = Some(identifier.into());
        self
    }

    /// Set an expected SHA-256 hash.
    ///
    /// The download will fail if the hash does not match.
    pub fn hash(mut self, hash: impl Into<String>) -> Self {
        self.hash = Some(hash.into());
        self
    }

    // TODO: Retries on failure.
    // TODO: Resuming downloads.
    /// The actual download logic.
    async fn download_impl(&mut self) -> anyhow::Result<Bytes> {
        let status = DownloadStatus::new(self.downloader, self.identifier.take());

        let client = client()?;
        let response = client.get(self.url.clone()).send().await?;

        if let Some(content_length) = response.content_length() {
            status.received_length(content_length);
        }

        let mut hasher = if self.hash.is_some() {
            Some(Sha256::new())
        } else {
            None
        };

        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            status.received_data(chunk.len());

            if let Some(s) = hasher.as_mut() {
                s.update(&chunk)
            }
        }

        status.finished();

        if let Some(hash) = self.hash.as_deref()
            && !hash.eq_ignore_ascii_case(&faster_hex::hex_string(
                &hasher.expect("should exist").finalize_fixed(),
            ))
        {
            bail!("hash mismatch");
        }

        match reqwest::get(self.url.clone())
            .and_then(Response::bytes)
            .await
        {
            Ok(data) => {
                status.finished();

                if let Some(hash) = self.hash.as_deref() {
                    let mut hasher = Sha256::new();
                    hasher.update(&data);
                    if hash != faster_hex::hex_string(&hasher.finalize_fixed()) {
                        bail!("hash mismatch");
                    }
                }

                Ok(data)
            }
            Err(e) => {
                status.failed();
                Err(e.into())
            }
        }
    }
}

/// Get the HTTP client.
fn client() -> anyhow::Result<Client> {
    static CLIENT: OnceLock<Client> = OnceLock::new();

    if CLIENT.get().is_none() {
        let client = Client::builder()
            .read_timeout(Duration::from_secs(30))
            .build()?;
        let _ = CLIENT.set(client);
    }

    Ok(CLIENT.get().cloned().unwrap())
}

impl Download<'_, ()> {
    /// Start the download, writing the response to the specified file on
    /// success.
    ///
    /// The [`InstallStatus`] can optionally be used if an installation step
    /// follows this download.
    pub async fn start(mut self) -> anyhow::Result<InstallStatus> {
        let destination = self.destination.take().expect("should have destination");
        match self.download_impl().await {
            Ok(data) => {
                std::fs::write(destination, data).context("failed to write to destination file")?;
                Ok(InstallStatus::new(self.downloader))
            }
            Err(e) => Err(e),
        }
    }
}

impl Download<'_, Bytes> {
    /// Start the download, returning the bytes and an [`InstallStatus`].
    ///
    /// The [`InstallStatus`] can optionally be used if an installation step
    /// follows this download.
    pub async fn start(mut self) -> anyhow::Result<(InstallStatus, Bytes)> {
        self.download_impl()
            .await
            .map(|bytes| (InstallStatus::new(self.downloader), bytes))
    }
}

/// The status of an active [`Download`].
struct DownloadStatus {
    /// The progress bar shown to the user.
    progress: ProgressBar,
}

impl DownloadStatus {
    /// Create a new `DownloadStatus`.
    fn new(downloader: &Downloader, mut identifier: Option<String>) -> Self {
        let progress = ProgressBar::hidden().with_style(
            ProgressStyle::with_template(
                "{msg:>13.bold} downloading [{bar:15}] {total_bytes:>11} ({bytes_per_sec}, ETA: \
                 {eta})",
            )
            .unwrap()
            .progress_chars("## "),
        );
        progress.set_message(
            identifier
                .take()
                .unwrap_or_else(|| String::from("component")),
        );

        downloader.progress.add(progress.clone());
        Self { progress }
    }

    /// Set the content length of the pending download.
    fn received_length(&self, len: u64) {
        self.progress.reset();
        self.progress.set_length(len);
    }

    /// A new chunk of data was received.
    fn received_data(&self, len: usize) {
        self.progress.inc(len as u64);
        self.progress.set_style(
            ProgressStyle::with_template(
                "{msg:>13.bold} downloading [{bar:15}] {total_bytes:>11} ({bytes_per_sec}, ETA: \
                 {eta})",
            )
            .unwrap()
            .progress_chars("## "),
        );
    }

    /// The download finished successfully.
    fn finished(&self) {
        self.progress.set_style(
            ProgressStyle::with_template("{msg:>13.bold} pending installation {total_bytes:>20}")
                .unwrap(),
        );
        self.progress.tick(); // A tick is needed for the new style to appear, as it is static.
    }

    /// The download failed.
    fn failed(&self) {
        self.progress.set_style(
            ProgressStyle::with_template("{msg:>13.bold} download failed after {elapsed}").unwrap(),
        );
        self.progress.finish();
    }
}

/// An installation status tracker.
pub struct InstallStatus {
    /// The progress bar shown to the user.
    progress: ProgressBar,
}

impl InstallStatus {
    /// Create a new `InstallStatus`.
    fn new(downloader: &Downloader) -> Self {
        let progress = ProgressBar::hidden();
        downloader.progress.add(progress.clone());

        Self { progress }
    }

    /// Entered the unpacking stage.
    pub(crate) fn unpack<T: Read>(&self, inner: T) -> ProgressBarIter<T> {
        self.progress.reset();
        self.progress.set_style(
            ProgressStyle::with_template(
                "{msg:>13.bold} unpacking   [{bar:15}] {total_bytes:>11} ({bytes_per_sec}, ETA: \
                 {eta})",
            )
            .unwrap()
            .progress_chars("## "),
        );
        self.progress.wrap_read(inner)
    }

    /// The installation finished successfully.
    pub(crate) fn installed(&self) {
        self.progress.set_style(
            ProgressStyle::with_template("{msg:>13.bold} installed {total_bytes:>31}").unwrap(),
        );
        self.progress.finish();
    }
}
