//! Management of Apptainer images.
//!
//! This module populates and maintains explicitly-converted `.sif` files and
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
//!
//! Since an ordinary `cargo test` runs each test executable once, and each
//! executable can contain many targets, this hack has a particularly distorting
//! effect on tests of this backend. Consider using `cargo nextest` to run each
//! target in a separate process in order to eliminate cross-test target
//! interference.

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
use tokio_util::sync::CancellationToken;
use tracing::error;
use tracing::info;
use tracing::trace;
use tracing::warn;

use super::ApptainerConfig;

/// The path to the global cache of `.sif`-format container images.
static APPTAINER_IMAGES_DIR: OnceCell<PathBuf> = OnceCell::const_new();
/// A global map from container strings to paths pointing to the `.sif` version
/// of that container.
static APPTAINER_IMAGES: LazyLock<Mutex<HashMap<String, Arc<OnceCell<PathBuf>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Get the directory containing converted `.sif`-format container images.
///
/// See the module-level documentation for details about the current behavior,
/// which unfortunately depends on global variables.
pub(crate) async fn global_apptainer_images_dir(
    config: &ApptainerConfig,
) -> Result<&'static Path, anyhow::Error> {
    APPTAINER_IMAGES_DIR
        .get_or_try_init(|| async {
            // Create a new temp directory to hold the images for this run. This approach
            // leaks space, but when using the default tmpdir or a
            // user-controlled destination, the system or user is hopefully able
            // to manage consumption appropriately enough for this interim
            // solution.
            let path = {
                let expanded =
                    PathBuf::from(shellexpand::full(&config.apptainer_images_dir)?.into_owned());
                tokio::fs::create_dir_all(&expanded).await?;
                TempDir::with_prefix_in("sprocket-apptainer-images-", &expanded)?.keep()
            };
            Ok::<PathBuf, anyhow::Error>(path)
        })
        .await
        .context("initializing Apptainer images directory")
        .map(|buf| buf.as_path())
}

/// Get the path to the container image in `.sif` format, potentially performing
/// an `apptainer pull` if the image cache has not already been populated.
pub(crate) async fn sif_for_container(
    config: &ApptainerConfig,
    container: &str,
    cancellation_token: CancellationToken,
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
        let sif_path = global_apptainer_images_dir(config)
            .await?
            // Append `.sif` to the filename. It would be nice to use a method like
            // [`with_added_extension()`](https://doc.rust-lang.org/std/path/struct.Path.html#method.with_added_extension)
            // instead, but it's not stable yet.
            .join(format!("{sif_filename}.sif"));

        let retry = Retry::spawn_notify(
            // TODO ACF 2025-09-22: configure the retry behavior based on actual experience with
            // flakiness of the container registries. This is a finger-in-the-wind guess at some
            // reasonable parameters that shouldn't lead to us making our own problems worse by
            // overwhelming registries with repeated retries.
            ExponentialBackoff::from_millis(50)
                .max_delay_millis(60_000)
                .take(10),
            || try_pull(&sif_path, &container),
            |e, _| {
                warn!(e = %e, "`apptainer pull` failed");
            },
        );

        tokio::select! {
            _ = cancellation_token.cancelled() => return Err(anyhow!("task execution cancelled")),
            res = retry => res?,
        };

        info!(sif_path = %sif_path.display(), container, "image pulled successfully");
        Ok(sif_path)
    })
    .await
    .cloned()
}

/// Try once to use `apptainer pull` to build the `.sif` file.
///
/// The tricky thing about this function is determining whether a failure is
/// transient or permanent. When in doubt, choose transient; the downside is a
/// permanent failure may take longer to finally bring down an execution, but
/// this is better for a long-running task than letting a transient failure
/// bring it down before a retry.
///
/// `apptainer pull` doesn't have a well-defined interface for us to tell
/// whether a failure is transient, but as we gain experience recognizing its
/// output patterns, we can enhance the fidelity of the error handling.
async fn try_pull(sif_path: &Path, container: &str) -> Result<(), RetryError<anyhow::Error>> {
    info!(container, "pulling image");
    let mut apptainer_pull_child = Command::new("apptainer")
        // Pipe the stdio handles, both for tracing and to inspect for telltale signs of permanent
        // errors
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("pull")
        .arg(sif_path)
        .arg(format!("docker://{container}"))
        .spawn()
        // If the system can't handle spawning a process, we're better off failing quickly
        .map_err(|e| RetryError::permanent(e.into()))?;

    let is_permanent = Arc::new(Mutex::new(false));

    let child_stdout = apptainer_pull_child
        .stdout
        .take()
        .ok_or_else(|| RetryError::permanent(anyhow!("apptainer pull child stdout missing")))?;
    let stdout_container = container.to_owned();
    let _stdout_is_permanent = is_permanent.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(child_stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            trace!(stdout = line, container = stdout_container);
        }
    });
    let child_stderr = apptainer_pull_child
        .stderr
        .take()
        .ok_or_else(|| RetryError::permanent(anyhow!("apptainer pull child stderr missing")))?;
    let stderr_container = container.to_owned();
    let stderr_is_permanent = is_permanent.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(child_stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            // A collection of strings observed in `apptainer pull` stderr in unrecoverable
            // conditions. Finding one of these in the output marks the attempt as a
            // permanent failure.
            let needles = ["manifest unknown", "403 (Forbidden)"];
            for needle in needles {
                if line.contains(needle) {
                    error!(
                        stderr = line,
                        container = stderr_container,
                        "`apptainer pull` failed"
                    );
                    *stderr_is_permanent.lock().unwrap() = true;
                    break;
                }
            }
            trace!(stderr = line, container = stderr_container);
        }
    });

    let child_result = apptainer_pull_child
        .wait()
        .await
        // Permanently error if something goes wrong trying to wait for the child process
        .map_err(|e| RetryError::permanent(e.into()))?;
    if !child_result.success() {
        let e = anyhow!(
            "`apptainer pull` failed with exit code {:?}",
            child_result.code()
        );
        if *is_permanent.lock().unwrap() {
            Err(RetryError::permanent(e))
        } else {
            Err(RetryError::transient(e))
        }
    } else {
        Ok(())
    }
}
