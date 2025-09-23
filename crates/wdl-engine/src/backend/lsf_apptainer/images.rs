//! Management of Apptainer images.
//!
//! This module populates and maintains explictly-converted `.sif` files and
//! reuses them without inducing additional requests to the container registry.
//!
//! Apptainer/Singularity has its own image format, `.sif`, but can convert from
//! Docker-style OCI images more-or-less transparently when images are specified
//! with a `docker://` prefix. It even caches these conversions so that repeated
//! invocations of `apptainer exec` with the same image specifier will not
//! trigger a full fetch and rebuild of the image.
//!
//! However, even though the `.sif` images are cached, Apptainer still sends a
//! request to the container registry when reusing the images in order to ensure
//! the image is up to date. Depending on the shape of a workflow execution and
//! the configuration of the container registry, this traffic is enough to cause
//! sporadic failures in workflow execution, particularly when large numbers of
//! tasks are invoked with the same WDL `container` requirement.
//!
//! By explicitly converting to `.sif` and using those files in `apptainer exec`
//! invocations instead of `docker://` specifiers, we avoid this additional
//! container registry traffic. To sidestep the issue of staleness between the
//! locally-converted `.sif` images and the contents of the container
//! registry, the `.sif` files are only used for a single workflow execution. We
//! still benefit from the Apptainer cache when building new `.sif` files, so
//! this is not much of a slowdown, but it does increase disk space consumption
//! depending on where the images directory is created.
//!
//! NOTE ACF 2025-09-22: This is currently a ⚠️ Hack Zone ⚠️ and is not meant to
//! reflect final behavior.
//!
//! We don't currently have a notion of a top-level directory for an entire
//! workflow execution; the `root` path for each workflow and task evaluator is
//! specific to _that_ workflow or task, but the point of keeping our own cache
//! of Apptainer images is to avoid pushing our luck with spotty
//! container registries by inducing repeated requests for the same image.
//!
//! For expedience, this implementation makes the simplifying assumption that we
//! have one top-level workflow execution per process, and keeps the images
//! directory in a global variable. This should be replaced with something more
//! robust, but currently fits the execution model of the `sprocket`
//! CLI well enough to proceed.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;

use anyhow::Context as _;
use anyhow::anyhow;
use tempfile::TempDir;
use tokio::io::AsyncBufReadExt as _;
use tokio::io::BufReader;
use tokio::process::Command;
use tokio::sync::OnceCell;
use tokio_retry2::Retry;
use tokio_retry2::RetryError;
use tokio_retry2::strategy::ExponentialBackoff;
use tracing::Level;
use tracing::trace;

use super::LsfApptainerBackendConfig;

static APPTAINER_IMAGES_DIR: OnceCell<PathBuf> = OnceCell::const_new();
static APPTAINER_IMAGES: LazyLock<Mutex<HashMap<String, Arc<OnceCell<PathBuf>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) async fn global_apptainer_images_dir(
    config: &LsfApptainerBackendConfig,
) -> Result<&'static Path, anyhow::Error> {
    APPTAINER_IMAGES_DIR
        .get_or_try_init(|| async {
            // Create a new temp directory to hold the images for this run. This approach
            // leaks space, but when using the default tmpdir or a
            // user-controlled destination, the system or user is hopefully able
            // to manage consumption appropriately enough for this interim
            // solution.
            let path = if let Some(path) = &config.apptainer_images_dir {
                tokio::fs::create_dir_all(&path).await?;
                TempDir::with_prefix_in("sprocket-apptainer-images-", path)?.keep()
            } else {
                TempDir::with_prefix("sprocket-apptainer-images-")?.keep()
            };
            Ok::<PathBuf, anyhow::Error>(path)
        })
        .await
        .context("initializing Apptainer images directory")
        .map(|buf| buf.as_path())
}

pub(crate) async fn sif_for_container(
    config: &LsfApptainerBackendConfig,
    container: &str,
) -> Result<PathBuf, anyhow::Error> {
    let once = {
        let mut map = APPTAINER_IMAGES.lock().unwrap();
        map.entry(container.to_owned())
            .or_insert_with(|| Arc::new(OnceCell::new()))
            .clone()
    };
    let container = container.to_owned();
    once.get_or_try_init(|| async move {
        let sif_filename = container.replace("/", "_2f_").replace(":", "_3a_");
        let mut sif_path = global_apptainer_images_dir(config)
            .await?
            .join(sif_filename)
            .to_path_buf();
        sif_path.set_extension("sif");

        Retry::spawn(
            // TODO ACF 2025-09-22: configure the retry behavior based on actual experience with
            // flakiness of the container registries. This is a finger-in-the-wind guess at some
            // reasonable parameters that shouldn't lead to us making our own problems worse by
            // overwhelming registries with repeated retries.
            ExponentialBackoff::from_millis(50)
                .max_delay_millis(60_000)
                .take(5),
            || async {
                try_pull(&sif_path, &container)
                    .await
                    .map_err(RetryError::transient)
            },
        )
        .await?;

        Ok(sif_path)
    })
    .await
    .cloned()
}

async fn try_pull(sif_path: &Path, container: &str) -> Result<(), anyhow::Error> {
    let mut apptainer_pull_command = Command::new("apptainer");
    if tracing::enabled!(Level::TRACE) {
        apptainer_pull_command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    } else {
        apptainer_pull_command
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    }
    let mut apptainer_pull_child = apptainer_pull_command
        .arg("pull")
        .arg(&sif_path)
        .arg(format!("docker://{container}"))
        .spawn()?;

    if tracing::enabled!(Level::TRACE) {
        // Take the stdio pipes from the child process and consume them for tracing
        // purposes.
        let child_stdout = apptainer_pull_child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("apptainer pull child stdout missing"))?;
        let stdout_container = container.to_owned();
        tokio::spawn(async move {
            let mut lines = BufReader::new(child_stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                trace!(stdout = line, container = stdout_container);
            }
        });
        let child_stderr = apptainer_pull_child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("apptainer pull child stderr missing"))?;
        let stderr_container = container.to_owned();
        tokio::spawn(async move {
            let mut lines = BufReader::new(child_stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                trace!(stderr = line, container = stderr_container);
            }
        });
    }

    let child_result = apptainer_pull_child.wait().await?;
    if !child_result.success() {
        Err(anyhow!(
            "`apptainer pull` failed with exit code {:?}",
            child_result.code()
        ))?
    } else {
        Ok(())
    }
}
