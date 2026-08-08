//! Cross-process coordination for the Apptainer `.sif` image cache.
//!
//! [`ApptainerImageCache`] serializes pulls for the same image across
//! processes using advisory file locks, optionally caps how many distinct
//! images may be pulled concurrently, and publishes a successfully pulled
//! image with a temporary file and rename so a partially pulled image is
//! never mistaken for a complete one.

// This module is not yet wired into `ApptainerRuntime`; a later task in the
// apptainer image cache plan does so.
#![cfg_attr(not(test), expect(dead_code))]

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::Weak;
use std::time::Duration;

use anyhow::Context as _;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use tokio::process::Command;
use tokio::sync::watch;
use tokio_retry2::Retry;
use tokio_retry2::RetryError;
use tokio_retry2::strategy::ExponentialBackoff;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use tracing::warn;

use crate::cache::lock::LockedFile;
use crate::v1::requirements::ContainerSource;

/// The name of the coordination directory within a cache directory.
const COORDINATION_DIR_NAME: &str = ".sprocket";

/// The name of the per-image lock directory within the coordination
/// directory.
const IMAGES_DIR_NAME: &str = "images";

/// The name of the failure marker directory within the coordination
/// directory.
const FAILURES_DIR_NAME: &str = "failures";

/// The name of the concurrency slot directory within the coordination
/// directory.
const SLOTS_DIR_NAME: &str = "slots";

/// The name of the cache policy file within the coordination directory.
const POLICY_FILE_NAME: &str = "policy.json";

/// The name of the lock file that guards the cache policy file.
const POLICY_LOCK_FILE_NAME: &str = "policy.lock";

/// The current [`CachePolicy`] schema version.
const CACHE_POLICY_VERSION: u32 = 1;

/// The current [`FailureMarker`] schema version.
const FAILURE_MARKER_VERSION: u32 = 1;

/// How long to wait between attempts to acquire a concurrency slot.
const SLOT_RETRY_INTERVAL: Duration = Duration::from_millis(50);

/// Stderr substrings that mark a pull failure as permanent rather than
/// transient.
const PERMANENT_FAILURE_NEEDLES: [&str; 2] = ["manifest unknown", "403 (Forbidden)"];

/// The policy recorded for a cache directory.
///
/// Every coordinator that opens the same cache directory must agree on this
/// policy. A coordinator that observes a mismatch fails instead of silently
/// adopting a different concurrency limit than the one already recorded.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct CachePolicy {
    /// The schema version of this policy file.
    version: u32,
    /// The configured limit on concurrently pulling distinct images.
    max_concurrent_pulls: Option<usize>,
}

/// A persisted record of a failed pull for one image.
///
/// Stored under the cache's failures directory so every coordinator sharing
/// the cache directory observes the same recorded failure and retry time.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct FailureMarker {
    /// The schema version of this marker file.
    version: u32,
    /// The number of consecutive failures recorded for the image.
    consecutive_failures: u32,
    /// The formatted error from the most recent failed pull.
    error: String,
    /// The time of the most recent failed pull.
    failed_at: DateTime<Utc>,
    /// The time at which a new pull attempt becomes eligible.
    ///
    /// Task 2 of the apptainer image cache plan does not implement a real
    /// backoff schedule; this is always set equal to `failed_at`. A later
    /// task applies an actual delay.
    retry_at: DateTime<Utc>,
}

/// The result of a completed pull, shared with every local waiter.
///
/// Uses an owned error message rather than [`anyhow::Error`] so the value can
/// be cloned when broadcasting it through a [`tokio::sync::watch`] channel.
type PullOutcome = Result<PathBuf, String>;

/// Tracks a single in-flight or completed pull for one [`ContainerSource`].
///
/// The operation runs independently of any individual waiter and publishes
/// its result once, through `sender`.
struct Operation {
    /// Publishes the operation's result once it completes.
    ///
    /// Holds `None` while the pull is still running.
    sender: watch::Sender<Option<Arc<PullOutcome>>>,
}

/// Coordinates access to a shared directory of cached Apptainer `.sif`
/// images across processes.
///
/// Pulls for the same image are always serialized using an advisory file
/// lock so that only one process pulls a given image at a time. Pulls for
/// different images may optionally be capped using a fixed number of slot
/// files under the cache directory.
#[derive(Debug)]
pub(crate) struct ApptainerImageCache {
    /// The root cache directory.
    cache_dir: PathBuf,
    /// The configured limit on concurrently pulling distinct images.
    ///
    /// `None` means pulls for distinct images are unlimited.
    ///
    /// Callers are expected to have already rejected `Some(0)`, as
    /// [`crate::config::ApptainerConfig::validate`] does.
    max_concurrent_pulls: Option<usize>,
    /// In-process pull operations, keyed by container source.
    ///
    /// Entries use a weak reference so that a completed operation is dropped
    /// once its spawned task and every waiter release their strong
    /// references. A later request for the same container simply starts a
    /// new operation; correctness comes from the filesystem, not this map.
    operations: Mutex<HashMap<ContainerSource, Weak<Operation>>>,
}

/// Returns the process-wide registry of live coordinators, keyed by their
/// normalized cache directory.
fn registry() -> &'static Mutex<HashMap<PathBuf, Weak<ApptainerImageCache>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<PathBuf, Weak<ApptainerImageCache>>>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

/// Converts a configured pull limit to the `usize` used for filesystem slot
/// iteration.
fn to_slot_limit(max_concurrent_pulls: Option<u64>) -> Result<Option<usize>> {
    max_concurrent_pulls
        .map(|limit| {
            usize::try_from(limit).with_context(|| {
                format!(
                    "Apptainer configuration value `max_concurrent_pulls` of {limit} does not fit \
                     in a `usize` on this platform"
                )
            })
        })
        .transpose()
}

/// Derives the stable key used to identify `container` within the cache's
/// coordination files.
fn image_key(container: &ContainerSource) -> arrayvec::ArrayString<64> {
    blake3::hash(container.to_string().as_bytes()).to_hex()
}

/// Writes `contents` to `path` atomically using a temporary file within
/// `dir` followed by a rename.
async fn write_atomic(dir: &Path, path: &Path, contents: &[u8]) -> Result<()> {
    let tmp_path = dir.join(format!(
        ".tmp.{pid}.{nonce:016x}",
        pid = std::process::id(),
        nonce = rand::random::<u64>(),
    ));

    tokio::fs::write(&tmp_path, contents)
        .await
        .with_context(|| {
            format!(
                "failed to write Apptainer image cache file `{path}`",
                path = tmp_path.display()
            )
        })?;
    tokio::fs::rename(&tmp_path, path).await.with_context(|| {
        format!(
            "failed to publish Apptainer image cache file `{path}`",
            path = path.display()
        )
    })?;
    Ok(())
}

/// Reads the failure marker for an image, if one is recorded.
async fn read_failure_marker(path: &Path) -> Result<Option<FailureMarker>> {
    match tokio::fs::read(path).await {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes).with_context(|| {
            format!(
                "failed to deserialize Apptainer image cache failure marker `{path}`",
                path = path.display()
            )
        })?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| {
            format!(
                "failed to read Apptainer image cache failure marker `{path}`",
                path = path.display()
            )
        }),
    }
}

/// Atomically writes an updated failure marker recording `error`.
///
/// If `previous` is present, its consecutive failure count is incremented;
/// otherwise the count starts at one.
async fn write_failure_marker(
    failures_dir: &Path,
    path: &Path,
    previous: Option<&FailureMarker>,
    error: &str,
) -> Result<()> {
    let now = Utc::now();
    let marker = FailureMarker {
        version: FAILURE_MARKER_VERSION,
        consecutive_failures: previous.map_or(1, |marker| marker.consecutive_failures + 1),
        error: error.to_string(),
        failed_at: now,
        retry_at: now,
    };

    write_atomic(
        failures_dir,
        path,
        &serde_json::to_vec_pretty(&marker)
            .context("failed to serialize Apptainer image cache failure marker")?,
    )
    .await
}

/// Removes a failure marker, if one is recorded.
async fn remove_failure_marker(path: &Path) -> Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| {
            format!(
                "failed to remove Apptainer image cache failure marker `{path}`",
                path = path.display()
            )
        }),
    }
}

/// Runs a single `{executable} pull` invocation to completion.
///
/// Classifies a failure as permanent when its stderr contains a known
/// unrecoverable pattern, and transient otherwise.
///
/// This duplicates similar logic in [`super::ApptainerRuntime`]; a later
/// task in the apptainer image cache plan consolidates the two, at which
/// point this copy adds `kill_on_drop(true)`, which the original lacks.
async fn try_pull_once(
    executable: &str,
    image: &str,
    path: &Path,
) -> Result<(), RetryError<anyhow::Error>> {
    debug!("spawning `{executable}` to pull image `{image}`");

    let child = Command::new(executable)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .arg("pull")
        .arg(path)
        .arg(image)
        .spawn()
        .with_context(|| {
            format!(
                "failed to spawn `{executable} pull '{path}' '{image}'`",
                path = path.display()
            )
        })
        // If the system can't handle spawning a process, we're better off failing quickly
        .map_err(RetryError::permanent)?;

    let output = child
        .wait_with_output()
        .await
        .context(format!("failed to wait for `{executable}`"))
        .map_err(RetryError::permanent)?;
    if !output.status.success() {
        let permanent = if let Ok(stderr) = str::from_utf8(&output.stderr) {
            PERMANENT_FAILURE_NEEDLES
                .iter()
                .any(|needle| stderr.contains(needle))
        } else {
            false
        };

        let e = anyhow!(
            "`{executable}` failed: {status}: {stderr}",
            status = output.status,
            stderr = str::from_utf8(&output.stderr)
                .unwrap_or("<output not UTF-8>")
                .trim()
        );
        return if permanent {
            Err(RetryError::permanent(e))
        } else {
            Err(RetryError::transient(e))
        };
    }

    Ok(())
}

/// Retries a single `{executable} pull` invocation with exponential backoff.
async fn retrying_pull(executable: &str, image: &str, path: &Path) -> Result<()> {
    let executable = executable.to_string();
    let image = image.to_string();
    Retry::spawn_notify(
        ExponentialBackoff::from_millis(50)
            .max_delay_millis(60_000)
            .take(10),
        || try_pull_once(&executable, &image, path),
        {
            let executable = executable.clone();
            move |e: &anyhow::Error, _| {
                warn!(e = %e, "`{executable} pull` failed");
            }
        },
    )
    .await
}

/// Converts a completed pull outcome into the caller-facing result.
fn outcome_to_result(outcome: &PullOutcome) -> Result<Option<PathBuf>> {
    match outcome {
        Ok(path) => Ok(Some(path.clone())),
        Err(message) => Err(anyhow!("{message}")),
    }
}

impl ApptainerImageCache {
    /// Gets or creates the coordinator for the given cache directory.
    ///
    /// Coordinators are cached per process so that concurrent callers with
    /// the same normalized cache directory share the same in-process state.
    /// If a live coordinator already exists for the directory, it is reused
    /// only when its `max_concurrent_pulls` matches the requested value;
    /// otherwise this returns a configuration error.
    pub(crate) async fn get(
        cache_dir: &Path,
        max_concurrent_pulls: Option<u64>,
    ) -> Result<Arc<Self>> {
        let cache_dir = absolute(cache_dir).with_context(|| {
            format!(
                "failed to make Apptainer image cache path `{path}` absolute",
                path = cache_dir.display()
            )
        })?;
        let max_concurrent_pulls = to_slot_limit(max_concurrent_pulls)?;

        if let Some(existing) = registry()
            .lock()
            .expect("failed to lock registry")
            .get(&cache_dir)
            .and_then(Weak::upgrade)
        {
            return Self::reuse_or_reject(existing, max_concurrent_pulls);
        }

        let cache = Arc::new(Self::initialize(cache_dir.clone(), max_concurrent_pulls).await?);

        let mut registered = registry().lock().expect("failed to lock registry");
        // Another task may have raced us to construct a coordinator for the same
        // directory; prefer whichever one is already registered so the process
        // has a single coordinator per directory.
        if let Some(existing) = registered.get(&cache_dir).and_then(Weak::upgrade) {
            return Self::reuse_or_reject(existing, max_concurrent_pulls);
        }
        registered.insert(cache_dir, Arc::downgrade(&cache));
        Ok(cache)
    }

    /// Returns `existing` if its policy matches `requested`, otherwise
    /// returns a configuration error naming both.
    fn reuse_or_reject(existing: Arc<Self>, requested: Option<usize>) -> Result<Arc<Self>> {
        if existing.max_concurrent_pulls != requested {
            bail!(
                "Apptainer image cache `{path}` is already coordinating with \
                 `max_concurrent_pulls` set to {recorded:?}, but {requested:?} was requested",
                path = existing.cache_dir.display(),
                recorded = existing.max_concurrent_pulls,
            );
        }

        Ok(existing)
    }

    /// Creates a coordinator for the given cache directory without
    /// registering it in the process-wide registry.
    ///
    /// This allows tests to construct independent coordinators for the same
    /// on-disk cache directory, simulating separate processes, without
    /// weakening the registry that production code relies on.
    #[cfg(test)]
    async fn new_uncoordinated(
        cache_dir: &Path,
        max_concurrent_pulls: Option<u64>,
    ) -> Result<Arc<Self>> {
        let cache_dir = absolute(cache_dir).with_context(|| {
            format!(
                "failed to make Apptainer image cache path `{path}` absolute",
                path = cache_dir.display()
            )
        })?;
        let max_concurrent_pulls = to_slot_limit(max_concurrent_pulls)?;
        Ok(Arc::new(
            Self::initialize(cache_dir, max_concurrent_pulls).await?,
        ))
    }

    /// Initializes the on-disk coordination layout for `cache_dir` and
    /// validates its recorded policy.
    async fn initialize(cache_dir: PathBuf, max_concurrent_pulls: Option<usize>) -> Result<Self> {
        let sprocket_dir = cache_dir.join(COORDINATION_DIR_NAME);
        let images_dir = sprocket_dir.join(IMAGES_DIR_NAME);
        let failures_dir = sprocket_dir.join(FAILURES_DIR_NAME);
        let slots_dir = sprocket_dir.join(SLOTS_DIR_NAME);
        for dir in [&sprocket_dir, &images_dir, &failures_dir, &slots_dir] {
            tokio::fs::create_dir_all(dir).await.with_context(|| {
                format!(
                    "failed to create Apptainer image cache directory `{path}`",
                    path = dir.display()
                )
            })?;
        }

        Self::apply_policy(&cache_dir, &sprocket_dir, &slots_dir, max_concurrent_pulls).await?;

        Ok(Self {
            cache_dir,
            max_concurrent_pulls,
            operations: Mutex::new(HashMap::new()),
        })
    }

    /// Reads, validates, or records the cache policy, and ensures slot files
    /// exist for a configured limit.
    async fn apply_policy(
        cache_dir: &Path,
        sprocket_dir: &Path,
        slots_dir: &Path,
        max_concurrent_pulls: Option<usize>,
    ) -> Result<()> {
        let policy_path = sprocket_dir.join(POLICY_FILE_NAME);
        let _lock = LockedFile::acquire_exclusive(sprocket_dir.join(POLICY_LOCK_FILE_NAME)).await?;

        match tokio::fs::read(&policy_path).await {
            Ok(bytes) => {
                let policy: CachePolicy = serde_json::from_slice(&bytes).with_context(|| {
                    format!(
                        "failed to deserialize Apptainer image cache policy `{path}`",
                        path = policy_path.display()
                    )
                })?;

                if policy.version != CACHE_POLICY_VERSION {
                    bail!(
                        "Apptainer image cache `{path}` has an unsupported policy version \
                         {version}, expected {CACHE_POLICY_VERSION}",
                        path = cache_dir.display(),
                        version = policy.version,
                    );
                }

                if policy.max_concurrent_pulls != max_concurrent_pulls {
                    bail!(
                        "Apptainer image cache `{path}` is already configured with \
                         `max_concurrent_pulls` set to {recorded:?}, but {requested:?} was \
                         requested; stop all processes using the cache and remove its \
                         `{COORDINATION_DIR_NAME}` directory to change the policy",
                        path = cache_dir.display(),
                        recorded = policy.max_concurrent_pulls,
                        requested = max_concurrent_pulls,
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let policy = CachePolicy {
                    version: CACHE_POLICY_VERSION,
                    max_concurrent_pulls,
                };
                write_atomic(
                    sprocket_dir,
                    &policy_path,
                    &serde_json::to_vec_pretty(&policy)
                        .context("failed to serialize Apptainer image cache policy")?,
                )
                .await?;
            }
            Err(e) => {
                return Err(e).with_context(|| {
                    format!(
                        "failed to read Apptainer image cache policy `{path}`",
                        path = policy_path.display()
                    )
                });
            }
        }

        if let Some(limit) = max_concurrent_pulls {
            for i in 0..limit {
                let slot_path = slots_dir.join(format!("{i}.lock"));
                // Creates the slot file if it does not already exist; an existing
                // slot file is left untouched so a lock held by another process is
                // unaffected.
                tokio::fs::OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .write(true)
                    .open(&slot_path)
                    .await
                    .with_context(|| {
                        format!(
                            "failed to create Apptainer image cache slot file `{path}`",
                            path = slot_path.display()
                        )
                    })?;
            }
        }

        Ok(())
    }

    /// Pulls `container` to `final_path`, coordinating with other processes
    /// and other local callers requesting the same image.
    ///
    /// If `final_path` already exists, its path is returned without pulling
    /// again. Returns `Ok(None)` if `token` is cancelled before the pull
    /// completes; the pull itself continues running for the benefit of any
    /// other waiter.
    pub(crate) async fn pull(
        self: &Arc<Self>,
        executable: &str,
        container: &ContainerSource,
        final_path: &Path,
        token: CancellationToken,
    ) -> Result<Option<PathBuf>> {
        if final_path.exists() {
            return Ok(Some(final_path.to_path_buf()));
        }

        let operation = self.attach_or_start(executable, container, final_path);
        Self::wait_for_operation(operation, token).await
    }

    /// Attaches to an already-running pull for `container`, or starts a new
    /// one.
    ///
    /// The returned operation runs independently of any individual waiter;
    /// it keeps running even if the caller that started it is cancelled, so
    /// that other waiters attached to the same operation still observe its
    /// result.
    fn attach_or_start(
        self: &Arc<Self>,
        executable: &str,
        container: &ContainerSource,
        final_path: &Path,
    ) -> Arc<Operation> {
        let mut operations = self.operations.lock().expect("failed to lock operations");
        if let Some(operation) = operations.get(container).and_then(Weak::upgrade) {
            return operation;
        }

        let (sender, _receiver) = watch::channel(None);
        let operation = Arc::new(Operation { sender });
        operations.insert(container.clone(), Arc::downgrade(&operation));
        drop(operations);

        let cache = Arc::clone(self);
        let executable = executable.to_string();
        let container = container.clone();
        let final_path = final_path.to_path_buf();
        let task_operation = Arc::clone(&operation);
        tokio::spawn(async move {
            let outcome = cache.run_pull(&executable, &container, &final_path).await;
            // Ignored because a send error only means every waiter already gave
            // up; the pull itself already ran to completion either way.
            let _ = task_operation.sender.send(Some(Arc::new(outcome)));
        });

        operation
    }

    /// Waits for `operation` to complete, or for `token` to be cancelled.
    ///
    /// Cancellation only stops waiting; it does not cancel the shared
    /// operation, which keeps running for the benefit of other waiters.
    async fn wait_for_operation(
        operation: Arc<Operation>,
        token: CancellationToken,
    ) -> Result<Option<PathBuf>> {
        let mut receiver = operation.sender.subscribe();
        drop(operation);

        loop {
            if let Some(outcome) = receiver.borrow_and_update().clone() {
                return outcome_to_result(&outcome);
            }

            tokio::select! {
                _ = token.cancelled() => return Ok(None),
                changed = receiver.changed() => {
                    changed.context("apptainer image pull operation ended without a result")?;
                }
            }
        }
    }

    /// Runs the pull for `container` to completion, returning an outcome
    /// that can be cheaply cloned and shared with every local waiter.
    async fn run_pull(
        &self,
        executable: &str,
        container: &ContainerSource,
        final_path: &Path,
    ) -> PullOutcome {
        self.run_pull_inner(executable, container, final_path)
            .await
            .map_err(|e| format!("{e:#}"))
    }

    /// Runs the coordinated pull protocol for `container` under the
    /// per-image and per-slot locks, publishing the result atomically on
    /// success.
    async fn run_pull_inner(
        &self,
        executable: &str,
        container: &ContainerSource,
        final_path: &Path,
    ) -> Result<PathBuf> {
        let sprocket_dir = self.cache_dir.join(COORDINATION_DIR_NAME);
        let key = image_key(container);
        let image_lock_path = sprocket_dir
            .join(IMAGES_DIR_NAME)
            .join(format!("{key}.lock"));
        // Held for the remainder of this function so that only one contender
        // for `container` ever waits for a global slot at a time.
        let _image_lock = LockedFile::acquire_exclusive(&image_lock_path).await?;

        if final_path.exists() {
            return Ok(final_path.to_path_buf());
        }

        let failures_dir = sprocket_dir.join(FAILURES_DIR_NAME);
        let failure_path = failures_dir.join(format!("{key}.json"));
        if let Some(marker) = read_failure_marker(&failure_path).await?
            && Utc::now() < marker.retry_at
        {
            bail!("{error}", error = marker.error);
        }

        let _slot = self.acquire_slot().await?;

        // Recheck now that we may have waited for a cache-wide slot; another
        // coordinator may have published the image or recorded a failure while
        // we waited.
        if final_path.exists() {
            return Ok(final_path.to_path_buf());
        }
        let previous_marker = read_failure_marker(&failure_path).await?;
        if let Some(marker) = &previous_marker
            && Utc::now() < marker.retry_at
        {
            bail!("{error}", error = marker.error);
        }

        let parent = final_path.parent().unwrap_or_else(|| Path::new("."));
        tokio::fs::create_dir_all(parent).await.with_context(|| {
            format!(
                "failed to create Apptainer image cache directory `{path}`",
                path = parent.display()
            )
        })?;

        let file_name = final_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                anyhow!(
                    "Apptainer image cache destination `{path}` has no file name",
                    path = final_path.display()
                )
            })?;
        let tmp_path = parent.join(format!(
            "{file_name}.partial.{pid}.{nonce:016x}",
            pid = std::process::id(),
            nonce = rand::random::<u64>(),
        ));

        let image = format!("{container:#}");
        match retrying_pull(executable, &image, &tmp_path).await {
            Ok(()) => {
                tokio::fs::rename(&tmp_path, final_path)
                    .await
                    .with_context(|| {
                        format!(
                            "failed to publish Apptainer image `{path}`",
                            path = final_path.display()
                        )
                    })?;
                remove_failure_marker(&failure_path).await?;
                Ok(final_path.to_path_buf())
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                write_failure_marker(
                    &failures_dir,
                    &failure_path,
                    previous_marker.as_ref(),
                    &format!("{e:#}"),
                )
                .await?;
                Err(e)
            }
        }
    }

    /// Acquires one of the fixed slot files, or returns immediately if
    /// pulls for distinct images are unlimited.
    ///
    /// The returned lock must be held for the duration of the pull;
    /// dropping it releases the slot for another contender.
    async fn acquire_slot(&self) -> Result<Option<LockedFile>> {
        let Some(limit) = self.max_concurrent_pulls else {
            return Ok(None);
        };

        let slots_dir = self
            .cache_dir
            .join(COORDINATION_DIR_NAME)
            .join(SLOTS_DIR_NAME);
        loop {
            for i in 0..limit {
                let slot_path = slots_dir.join(format!("{i}.lock"));
                if let Some(lock) = LockedFile::try_acquire_exclusive(&slot_path)? {
                    return Ok(Some(lock));
                }
            }

            tokio::time::sleep(SLOT_RETRY_INTERVAL).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::v1::requirements::ContainerSource;

    /// The bytes the fake waiting executable writes to its destination
    /// argument when it succeeds.
    #[cfg(unix)]
    const FAKE_IMAGE_BYTES: &str = "fake-sif-bytes";

    /// Marks `path` as executable by its owner and group.
    #[cfg(unix)]
    fn set_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o770);
        std::fs::set_permissions(path, perms).unwrap();
    }

    /// Writes a fake `apptainer`-like executable to `path`.
    ///
    /// When invoked as `<path> pull <dest> <image>`, the script increments
    /// `counter_path` under a portable `mkdir`-based lock, waits until
    /// `release_path` exists, writes [`FAKE_IMAGE_BYTES`] to its destination
    /// argument, and exits with `status`.
    #[cfg(unix)]
    fn write_waiting_executable(
        path: &Path,
        counter_path: &Path,
        release_path: &Path,
        status: i32,
    ) {
        let script = format!(
            r#"#!/bin/sh
set -eu
dest="$2"
lockdir="{counter}.lockdir"
until mkdir "$lockdir" 2>/dev/null; do
  sleep 0.01
done
current=0
if [ -f "{counter}" ]; then
  current=$(cat "{counter}")
fi
current=$((current + 1))
printf '%s' "$current" > "{counter}"
rmdir "$lockdir"
while [ ! -f "{release}" ]; do
  sleep 0.02
done
printf '%s' '{bytes}' > "$dest"
exit {status}
"#,
            counter = counter_path.display(),
            release = release_path.display(),
            bytes = FAKE_IMAGE_BYTES,
            status = status,
        );

        std::fs::write(path, script).unwrap();
        set_executable(path);
    }

    /// Writes a fake `apptainer`-like executable to `path` that always fails
    /// with output the cache classifies as a permanent failure.
    #[cfg(unix)]
    fn write_failing_executable(path: &Path) {
        let script = "#!/bin/sh\necho '403 (Forbidden)' >&2\nexit 1\n";
        std::fs::write(path, script).unwrap();
        set_executable(path);
    }

    /// Polls `condition` until it returns `true`, panicking if `timeout`
    /// elapses first.
    #[cfg(unix)]
    async fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) {
        let start = tokio::time::Instant::now();
        loop {
            if condition() {
                return;
            }
            assert!(
                start.elapsed() < timeout,
                "condition was not met within {timeout:?}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Reads the counter file written by the fake waiting executable,
    /// returning `0` if it does not exist yet.
    #[cfg(unix)]
    fn read_counter(path: &Path) -> u64 {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn same_image_pull_is_coalesced_and_published_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let cache_a = ApptainerImageCache::new_uncoordinated(dir.path(), None)
            .await
            .unwrap();
        let cache_b = ApptainerImageCache::new_uncoordinated(dir.path(), None)
            .await
            .unwrap();

        let exe = dir.path().join("fake-apptainer");
        let counter = dir.path().join("counter");
        let release = dir.path().join("release");
        write_waiting_executable(&exe, &counter, &release, 0);

        let container = ContainerSource::Docker("test/same-image:latest".to_string());
        let final_path = dir.path().join("same-image.sif");

        let mut calls = tokio::task::JoinSet::new();
        for i in 0..8u32 {
            let cache = if i % 2 == 0 {
                Arc::clone(&cache_a)
            } else {
                Arc::clone(&cache_b)
            };
            let exe = exe.clone();
            let container = container.clone();
            let final_path = final_path.clone();
            calls.spawn(async move {
                cache
                    .pull(
                        exe.to_str().unwrap(),
                        &container,
                        &final_path,
                        CancellationToken::new(),
                    )
                    .await
            });
        }

        wait_until(Duration::from_secs(5), || read_counter(&counter) >= 1).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            read_counter(&counter),
            1,
            "only one fake pull should have started for the same image"
        );
        assert!(
            !final_path.exists(),
            "the final SIF must not exist while the fake pull is waiting on release"
        );

        std::fs::write(&release, b"go").unwrap();

        let mut paths = Vec::new();
        while let Some(result) = calls.join_next().await {
            let path = result
                .unwrap()
                .unwrap()
                .expect("pull should not have been cancelled");
            paths.push(path);
        }

        assert_eq!(paths.len(), 8);
        assert!(
            paths.iter().all(|path| *path == final_path),
            "every caller should receive the same final path"
        );
        assert_eq!(
            std::fs::read(&final_path).unwrap(),
            FAKE_IMAGE_BYTES.as_bytes()
        );
        assert_eq!(
            read_counter(&counter),
            1,
            "the fake pull must run exactly once even though eight callers requested it"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn different_images_pull_concurrently_when_unlimited() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ApptainerImageCache::new_uncoordinated(dir.path(), None)
            .await
            .unwrap();

        let exe_a = dir.path().join("fake-apptainer-a");
        let counter_a = dir.path().join("counter-a");
        let release_a = dir.path().join("release-a");
        write_waiting_executable(&exe_a, &counter_a, &release_a, 0);

        let exe_b = dir.path().join("fake-apptainer-b");
        let counter_b = dir.path().join("counter-b");
        let release_b = dir.path().join("release-b");
        write_waiting_executable(&exe_b, &counter_b, &release_b, 0);

        let container_a = ContainerSource::Docker("test/image-a:latest".to_string());
        let container_b = ContainerSource::Docker("test/image-b:latest".to_string());
        let final_a = dir.path().join("image-a.sif");
        let final_b = dir.path().join("image-b.sif");

        let task_a = {
            let cache = Arc::clone(&cache);
            let exe = exe_a.clone();
            let container = container_a.clone();
            let final_path = final_a.clone();
            tokio::spawn(async move {
                cache
                    .pull(
                        exe.to_str().unwrap(),
                        &container,
                        &final_path,
                        CancellationToken::new(),
                    )
                    .await
            })
        };

        let task_b = {
            let cache = Arc::clone(&cache);
            let exe = exe_b.clone();
            let container = container_b.clone();
            let final_path = final_b.clone();
            tokio::spawn(async move {
                cache
                    .pull(
                        exe.to_str().unwrap(),
                        &container,
                        &final_path,
                        CancellationToken::new(),
                    )
                    .await
            })
        };

        wait_until(Duration::from_secs(5), || {
            read_counter(&counter_a) >= 1 && read_counter(&counter_b) >= 1
        })
        .await;

        std::fs::write(&release_a, b"go").unwrap();
        std::fs::write(&release_b, b"go").unwrap();

        task_a
            .await
            .unwrap()
            .unwrap()
            .expect("pull a should succeed");
        task_b
            .await
            .unwrap()
            .unwrap()
            .expect("pull b should succeed");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn distinct_images_are_limited_by_available_slots() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ApptainerImageCache::new_uncoordinated(dir.path(), Some(1))
            .await
            .unwrap();

        let exe_a = dir.path().join("fake-apptainer-a");
        let counter_a = dir.path().join("counter-a");
        let release_a = dir.path().join("release-a");
        write_waiting_executable(&exe_a, &counter_a, &release_a, 0);

        let exe_b = dir.path().join("fake-apptainer-b");
        let counter_b = dir.path().join("counter-b");
        let release_b = dir.path().join("release-b");
        write_waiting_executable(&exe_b, &counter_b, &release_b, 0);

        let container_a = ContainerSource::Docker("test/slot-a:latest".to_string());
        let container_b = ContainerSource::Docker("test/slot-b:latest".to_string());
        let final_a = dir.path().join("slot-a.sif");
        let final_b = dir.path().join("slot-b.sif");

        let task_a = {
            let cache = Arc::clone(&cache);
            let exe = exe_a.clone();
            let container = container_a.clone();
            let final_path = final_a.clone();
            tokio::spawn(async move {
                cache
                    .pull(
                        exe.to_str().unwrap(),
                        &container,
                        &final_path,
                        CancellationToken::new(),
                    )
                    .await
            })
        };

        wait_until(Duration::from_secs(5), || read_counter(&counter_a) >= 1).await;

        let task_b = {
            let cache = Arc::clone(&cache);
            let exe = exe_b.clone();
            let container = container_b.clone();
            let final_path = final_b.clone();
            tokio::spawn(async move {
                cache
                    .pull(
                        exe.to_str().unwrap(),
                        &container,
                        &final_path,
                        CancellationToken::new(),
                    )
                    .await
            })
        };

        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            read_counter(&counter_b),
            0,
            "the second image must not start pulling while the only slot is held"
        );

        std::fs::write(&release_a, b"go").unwrap();
        task_a
            .await
            .unwrap()
            .unwrap()
            .expect("pull a should succeed");

        wait_until(Duration::from_secs(5), || read_counter(&counter_b) >= 1).await;
        std::fs::write(&release_b, b"go").unwrap();
        task_b
            .await
            .unwrap()
            .unwrap()
            .expect("pull b should succeed");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn independent_coordinator_rejects_mismatched_policy() {
        let dir = tempfile::tempdir().unwrap();
        let first = ApptainerImageCache::new_uncoordinated(dir.path(), Some(2))
            .await
            .expect("first coordinator should initialize the cache policy");

        let exe = dir.path().join("fake-apptainer");
        let counter = dir.path().join("counter");
        let release = dir.path().join("release");
        write_waiting_executable(&exe, &counter, &release, 0);
        std::fs::write(&release, b"go").unwrap();

        first
            .pull(
                exe.to_str().unwrap(),
                &ContainerSource::Docker("test/policy-image:latest".to_string()),
                &dir.path().join("policy-image.sif"),
                CancellationToken::new(),
            )
            .await
            .unwrap()
            .expect("initial pull should succeed");

        let error = ApptainerImageCache::new_uncoordinated(dir.path(), None)
            .await
            .expect_err("a coordinator requesting a different policy should fail");
        let message = format!("{error:#}");
        assert!(
            message.contains("Some(2)"),
            "error should name the recorded policy: {message}"
        );
        assert!(
            message.contains("None"),
            "error should name the requested policy: {message}"
        );
        assert!(
            message.contains(dir.path().to_str().unwrap()),
            "error should name the cache path: {message}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stale_partial_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ApptainerImageCache::new_uncoordinated(dir.path(), None)
            .await
            .unwrap();

        let final_path = dir.path().join("stale.sif");
        let stale_partial = dir.path().join("stale.sif.partial.999999.dead0deaddead0de");
        std::fs::write(&stale_partial, b"leftover from a crashed process").unwrap();

        let exe = dir.path().join("fake-apptainer");
        let counter = dir.path().join("counter");
        let release = dir.path().join("release");
        write_waiting_executable(&exe, &counter, &release, 0);
        std::fs::write(&release, b"go").unwrap();

        let path = cache
            .pull(
                exe.to_str().unwrap(),
                &ContainerSource::Docker("test/stale-image:latest".to_string()),
                &final_path,
                CancellationToken::new(),
            )
            .await
            .unwrap()
            .expect("pull should succeed despite the stale partial file");

        assert_eq!(path, final_path);
        assert_eq!(
            std::fs::read(&final_path).unwrap(),
            FAKE_IMAGE_BYTES.as_bytes()
        );
        assert!(
            stale_partial.exists(),
            "the cache must not delete unrelated stale partial files it did not create"
        );
        assert_eq!(
            read_counter(&counter),
            1,
            "the cache must still perform a real pull despite the stale partial file"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failing_pull_reports_error_to_every_waiter_and_records_a_marker() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ApptainerImageCache::new_uncoordinated(dir.path(), None)
            .await
            .unwrap();

        let exe = dir.path().join("fake-apptainer-failing");
        write_failing_executable(&exe);

        let container = ContainerSource::Docker("test/failing-image:latest".to_string());
        let final_path = dir.path().join("failing-image.sif");

        let mut calls = tokio::task::JoinSet::new();
        for _ in 0..3 {
            let cache = Arc::clone(&cache);
            let exe = exe.clone();
            let container = container.clone();
            let final_path = final_path.clone();
            calls.spawn(async move {
                cache
                    .pull(
                        exe.to_str().unwrap(),
                        &container,
                        &final_path,
                        CancellationToken::new(),
                    )
                    .await
            });
        }

        let mut messages = Vec::new();
        while let Some(result) = calls.join_next().await {
            let error = result
                .unwrap()
                .expect_err("pull should fail for every waiter");
            messages.push(format!("{error:#}"));
        }

        assert_eq!(messages.len(), 3);
        assert!(
            messages.iter().all(|m| m.contains("403 (Forbidden)")),
            "every waiter should see the same underlying failure: {messages:?}"
        );
        assert!(!final_path.exists());
        assert_eq!(
            std::fs::read_dir(dir.path())
                .unwrap()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_name().to_string_lossy().contains(".partial."))
                .count(),
            0,
            "the temporary partial file must be cleaned up after a failed pull"
        );

        // A second attempt should still fail, and the recorded failure marker's
        // consecutive count should have increased.
        let error = cache
            .pull(
                exe.to_str().unwrap(),
                &container,
                &final_path,
                CancellationToken::new(),
            )
            .await
            .expect_err("a subsequent pull should also fail");
        assert!(format!("{error:#}").contains("403 (Forbidden)"));

        let failure_path = dir
            .path()
            .join(COORDINATION_DIR_NAME)
            .join(FAILURES_DIR_NAME)
            .join(format!("{}.json", image_key(&container)));
        let marker: FailureMarker =
            serde_json::from_slice(&std::fs::read(&failure_path).unwrap()).unwrap();
        assert_eq!(
            marker.consecutive_failures, 2,
            "the marker should record two consecutive failures"
        );
    }

    #[tokio::test]
    async fn get_reuses_a_live_coordinator_with_a_matching_policy() {
        let dir = tempfile::tempdir().unwrap();
        let first = ApptainerImageCache::get(dir.path(), Some(3)).await.unwrap();
        let second = ApptainerImageCache::get(dir.path(), Some(3)).await.unwrap();
        assert!(
            Arc::ptr_eq(&first, &second),
            "requests for the same directory and policy should reuse the same coordinator"
        );
    }

    #[tokio::test]
    async fn get_rejects_a_live_coordinator_with_a_mismatched_policy() {
        let dir = tempfile::tempdir().unwrap();
        let _first = ApptainerImageCache::get(dir.path(), Some(3)).await.unwrap();
        let error = ApptainerImageCache::get(dir.path(), Some(4))
            .await
            .expect_err("a live coordinator with a mismatched policy should be rejected");
        let message = format!("{error:#}");
        assert!(message.contains("Some(3)"));
        assert!(message.contains("Some(4)"));
    }
}
