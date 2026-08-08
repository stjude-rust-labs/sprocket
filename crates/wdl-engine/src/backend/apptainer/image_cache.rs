//! Cross-process coordination for the Apptainer `.sif` image cache.
//!
//! A single Sprocket invocation can start many per-task runtimes, and each
//! spawns its own Apptainer process, so a cold cache directory is
//! routinely opened by many processes at once. Without coordination those
//! processes would race to pull the same image redundantly, wasting
//! bandwidth and disk, and a reader could observe a partially written
//! `.sif` file. [`ApptainerImageCache`] exists to prevent both.
//!
//! Coordination happens in two layers. Within one process, concurrent
//! requests for the same [`ContainerSource`] attach to a single spawned
//! pull operation and observe its result through a `tokio::sync::watch`
//! channel, so the process itself never starts two pulls for the same
//! image. Across processes, which may run on different hosts sharing this
//! cache directory over a network filesystem, a per-image advisory file
//! lock under `.sprocket/images/` serializes pulls for the same image so
//! only one process pulls it at a time.
//!
//! A cache directory may also be configured with a limit on how many
//! distinct images may be pulled concurrently. When set, a fixed set of
//! slot lock files under `.sprocket/slots/` acts as a cache-wide advisory
//! semaphore; a process acquires one exclusively before pulling and
//! releases it once the pull finishes, sweeping the slots and retrying
//! with jitter while every slot is held.
//!
//! A pull writes to a temporary file in the same directory as its final
//! `.sif` destination and only renames it into place once the pull
//! succeeds, so a reader only ever sees either no file or a complete one,
//! never a partial one. A failed pull is recorded in a persisted failure
//! marker under `.sprocket/failures/`, which every coordinator sharing the
//! cache directory consults before starting a new pull, and which records an
//! exponentially increasing delay before a new attempt for that image
//! becomes eligible again.
//!
//! The coordination layout is created lazily, only once a request for a
//! registry image finds no `.sif` already cached for it. A cache that a
//! process may read but not write therefore still serves images it already
//! holds, which matters when an administrator populates a shared cache and
//! exposes it read-only to the hosts that consume it.
//!
//! A caller waiting on a pull may cancel its own wait at any time. Doing so
//! only stops that caller from waiting; it does not cancel the underlying
//! pull, which keeps running to completion for the benefit of any other
//! local or cross-process waiter still attached to it.
//!
//! This protocol depends on the cache directory sitting on a filesystem
//! whose advisory locks are honored across every host that shares it and
//! whose rename within a single directory is atomic; both guarantees above
//! rely on that support being present. All hosts sharing the cache directory
//! must also have reasonably synchronized UTC clocks, because backoff
//! eligibility is determined by comparing the marker's `retry_at` field
//! against each host's `Utc::now()`; a host whose clock is skewed or has
//! been stepped backward may bypass, shorten, or extend the intended backoff
//! window.

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
use chrono::SecondsFormat;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use tokio::process::Command;
use tokio::sync::OnceCell;
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
pub(super) const COORDINATION_DIR_NAME: &str = ".sprocket";

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

/// The nominal number of milliseconds to wait before the first retry of a
/// sweep over the concurrency slot files.
const SLOT_RETRY_INITIAL_INTERVAL_MILLIS: u64 = 50;

/// The largest nominal number of milliseconds to wait between sweeps over
/// the concurrency slot files.
const SLOT_RETRY_MAX_INTERVAL_MILLIS: u64 = 2_000;

/// The largest number of times the nominal slot retry interval doubles.
///
/// `SLOT_RETRY_INITIAL_INTERVAL_MILLIS` doubled six times is 3200
/// milliseconds, which already exceeds `SLOT_RETRY_MAX_INTERVAL_MILLIS`, so
/// no further doubling can change the capped result. Clamping the exponent
/// here also keeps the shift used by [`slot_retry_interval_millis`] far away
/// from overflowing.
const SLOT_RETRY_MAX_DOUBLINGS: u32 = 6;

/// The divisor applied to a nominal slot retry interval to obtain the
/// maximum jitter applied in either direction around it.
///
/// A divisor of two spreads each sampled delay across half the nominal
/// interval on either side, so the smallest possible jitter magnitude is
/// half of `SLOT_RETRY_INITIAL_INTERVAL_MILLIS` and is therefore never zero.
const SLOT_RETRY_JITTER_DIVISOR: u64 = 2;

/// The smallest age at which a leftover partial file beside a final `.sif`
/// is treated as abandoned by a crashed process and removed.
///
/// The legacy cache layout maps a container name onto a path, so two
/// distinct container sources can share one final `.sif` path while using
/// different per-image locks. A partial file beside that path may therefore
/// belong to a pull that another process is actively running, and only a
/// conservatively old one can be assumed abandoned.
const STALE_PARTIAL_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);

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
    /// Set to `failed_at` plus the delay `retry_delay` computes from
    /// `consecutive_failures`.
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
///
/// Constructing a coordinator touches no filesystem state. The coordination
/// layout is created and validated lazily, by [`Self::ensure_initialized`],
/// only once a request misses the cache and a pull is actually required.
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
    /// Whether the on-disk coordination layout has been created and its
    /// recorded policy validated against `max_concurrent_pulls`.
    ///
    /// Concurrent callers that all miss the cache share one initialization
    /// attempt and therefore one result. A failed attempt leaves the cell
    /// empty so a later request can try again, which matters when the
    /// failure was transient.
    initialized: OnceCell<()>,
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

/// Returns the cache directories currently present in the process-wide
/// registry, whether or not their coordinator is still alive.
///
/// Exposed only to tests so that pruning of entries whose coordinator has
/// been dropped can be observed directly.
#[cfg(test)]
fn registry_entries() -> Vec<PathBuf> {
    registry()
        .lock()
        .expect("failed to lock registry")
        .keys()
        .cloned()
        .collect()
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
///
/// Uses `container`'s alternate (`{:#}`) `Display` form, which includes its
/// protocol prefix (e.g. `docker://`, `oras://`), rather than the
/// non-alternate form, which omits it. Hashing the non-alternate form would
/// alias sources that share a name but differ only in protocol (for
/// example `docker://same-name` and `oras://same-name`), causing them to
/// incorrectly share a per-image lock and failure marker.
fn image_key(container: &ContainerSource) -> arrayvec::ArrayString<64> {
    blake3::hash(format!("{container:#}").as_bytes()).to_hex()
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

/// Returns the delay to wait before a new pull attempt becomes eligible
/// after `consecutive_failures` consecutive failed pulls.
///
/// The delay doubles with each additional failure, from one second up to a
/// sixty-four second cap that applies from the seventh consecutive failure
/// onward.
fn retry_delay(consecutive_failures: u32) -> Duration {
    Duration::from_secs(1_u64 << consecutive_failures.saturating_sub(1).min(6))
}

/// Atomically writes an updated failure marker recording `error`.
///
/// If `previous` is present, its consecutive failure count is incremented;
/// otherwise the count starts at one. The marker's `retry_at` is set to the
/// current time plus the delay `retry_delay` computes for that failure
/// count.
async fn write_failure_marker(
    failures_dir: &Path,
    path: &Path,
    previous: Option<&FailureMarker>,
    error: &str,
) -> Result<()> {
    let now = Utc::now();
    let consecutive_failures = previous.map_or(1, |marker| marker.consecutive_failures + 1);
    let delay = chrono::Duration::from_std(retry_delay(consecutive_failures))
        .context("failed to convert Apptainer image cache retry delay to a `chrono::Duration`")?;
    let marker = FailureMarker {
        version: FAILURE_MARKER_VERSION,
        consecutive_failures,
        error: error.to_string(),
        failed_at: now,
        retry_at: now + delay,
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

impl ApptainerImageCache {
    /// Returns the root cache directory this coordinator manages.
    ///
    /// This is the absolute, normalized path passed to [`Self::get`], and is
    /// exposed so a caller can derive the final path for an image within the
    /// cache without duplicating that normalization itself.
    pub(crate) fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Gets or creates the coordinator for the given cache directory.
    ///
    /// Coordinators are cached per process so that concurrent callers with
    /// the same normalized cache directory share the same in-process state.
    /// If a live coordinator already exists for the directory, it is reused
    /// only when its `max_concurrent_pulls` matches the requested value;
    /// otherwise this returns a configuration error.
    ///
    /// This performs no filesystem access beyond making `cache_dir`
    /// absolute; the cache's coordination layout is created lazily by
    /// [`Self::ensure_initialized`] when a pull is first required.
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

        let cache = Arc::new(Self::new(cache_dir.clone(), max_concurrent_pulls));

        let mut registered = registry().lock().expect("failed to lock registry");
        // Another task may have raced us to construct a coordinator for the same
        // directory; prefer whichever one is already registered so the process
        // has a single coordinator per directory.
        if let Some(existing) = registered.get(&cache_dir).and_then(Weak::upgrade) {
            return Self::reuse_or_reject(existing, max_concurrent_pulls);
        }
        // Runs that do not configure a shared cache directory each get their own,
        // so without this a long-lived process would accumulate one permanently
        // dead entry per run. Pruning here keeps that growth bounded by the
        // number of live coordinators plus the entries added since the last
        // insertion.
        registered.retain(|_, cache| cache.strong_count() > 0);
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

    /// Creates a coordinator for `cache_dir` without touching the
    /// filesystem.
    fn new(cache_dir: PathBuf, max_concurrent_pulls: Option<usize>) -> Self {
        Self {
            cache_dir,
            max_concurrent_pulls,
            initialized: OnceCell::new(),
            operations: Mutex::new(HashMap::new()),
        }
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
        Ok(Arc::new(Self::new(cache_dir, max_concurrent_pulls)))
    }

    /// Creates the on-disk coordination layout for this cache and validates
    /// its recorded policy, at most once per coordinator.
    ///
    /// Concurrent callers share a single attempt and therefore observe the
    /// same result. A failed attempt is not remembered, so a later request
    /// retries rather than being permanently poisoned by a transient
    /// failure.
    async fn ensure_initialized(&self) -> Result<()> {
        self.initialized
            .get_or_try_init(|| Self::initialize(&self.cache_dir, self.max_concurrent_pulls))
            .await
            .copied()
    }

    /// Initializes the on-disk coordination layout for `cache_dir` and
    /// validates its recorded policy.
    async fn initialize(cache_dir: &Path, max_concurrent_pulls: Option<usize>) -> Result<()> {
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

        Self::apply_policy(cache_dir, &sprocket_dir, &slots_dir, max_concurrent_pulls).await
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
    /// again and without creating or reading any coordination state, so a
    /// cache the process may read but not write still serves the images it
    /// already holds. Returns `Ok(None)` if `token` is cancelled before the
    /// pull completes; the pull itself continues running for the benefit of
    /// any other waiter.
    pub(crate) async fn pull(
        self: &Arc<Self>,
        executable: &str,
        container: &ContainerSource,
        final_path: &Path,
        token: CancellationToken,
    ) -> Result<Option<PathBuf>> {
        if final_path.exists() {
            debug!(
                path = %final_path.display(),
                "Apptainer image `{container:#}` already cached; using existing image"
            );
            return Ok(Some(final_path.to_path_buf()));
        }

        // Creating the coordination layout and validating the recorded policy
        // happen before any shared operation is started, so a policy mismatch
        // fails before a pull is attempted and no coordination state is touched
        // for a cache that never misses.
        self.ensure_initialized().await?;

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

            // Drop this operation's own entry from the shared map so a later request
            // for this container starts a fresh operation rather than finding a
            // `Weak` that can never be upgraded again. Identity is compared first
            // because a cancelled waiter drops only its own receiver, never this
            // task, so `task_operation` remains the map's sole strong owner up to
            // this point and no other request could have replaced this entry.
            let mut operations = cache.operations.lock().expect("failed to lock operations");
            if let Some(current) = operations.get(&container)
                && Weak::ptr_eq(current, &Arc::downgrade(&task_operation))
            {
                operations.remove(&container);
            }
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
                return match outcome.as_ref() {
                    Ok(path) => Ok(Some(path.clone())),
                    Err(message) => Err(anyhow!("{message}")),
                };
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
            debug!(
                path = %final_path.display(),
                "Apptainer image `{container:#}` was cached by another process while this one \
                 waited for its image lock; using existing image"
            );
            return Ok(final_path.to_path_buf());
        }

        let failures_dir = sprocket_dir.join(FAILURES_DIR_NAME);
        let failure_path = failures_dir.join(format!("{key}.json"));
        if let Some(marker) = read_failure_marker(&failure_path).await?
            && Utc::now() < marker.retry_at
        {
            return Err(replayed_failure_error(container, &marker));
        }

        let _slot = self.acquire_slot().await?;

        // Recheck now that we may have waited for a cache-wide slot; another
        // coordinator may have published the image or recorded a failure while
        // we waited.
        if final_path.exists() {
            debug!(
                path = %final_path.display(),
                "Apptainer image `{container:#}` was cached by another process while this one \
                 waited for a pull slot; using existing image"
            );
            return Ok(final_path.to_path_buf());
        }
        let previous_marker = read_failure_marker(&failure_path).await?;
        if let Some(marker) = &previous_marker
            && Utc::now() < marker.retry_at
        {
            return Err(replayed_failure_error(container, marker));
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

        // Runs under the per-image lock and before the pull begins, so no other
        // coordinator is pulling this image at the same time.
        remove_stale_partials(parent, file_name).await;

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
                // The image is published at this point, so a failure to clear the
                // now-stale marker must not turn this success into an error. The
                // marker is harmless because every request checks the final path
                // before it reads a marker.
                if let Err(e) = remove_failure_marker(&failure_path).await {
                    let e = format!("{e:#}");
                    warn!(
                        e = %e,
                        "failed to remove the stale Apptainer image cache failure marker \
                         `{path}` after publishing `{container:#}`",
                        path = failure_path.display()
                    );
                }
                debug!(
                    path = %final_path.display(),
                    "Apptainer image `{container:#}` pulled successfully"
                );
                Ok(final_path.to_path_buf())
            }
            Err(e) => {
                if let Err(removal) = tokio::fs::remove_file(&tmp_path).await
                    && removal.kind() != std::io::ErrorKind::NotFound
                {
                    warn!(
                        e = %removal,
                        "failed to remove the Apptainer image cache temporary file `{path}` \
                         after a failed pull of `{container:#}`",
                        path = tmp_path.display()
                    );
                }
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
    ///
    /// Each complete sweep over the fixed slot files runs inside
    /// [`tokio::task::spawn_blocking`] via [`try_acquire_any_slot`], so the
    /// synchronous `open()` and advisory-lock calls it makes never run on a
    /// Tokio worker thread. The `.await` points that remain between sweeps
    /// (joining the blocking task, then the retry sleep) keep the loop
    /// cancellation-safe: dropping or aborting the future that calls this
    /// method can only happen between sweeps, never inside one, so a
    /// cancellation is never left waiting on a synchronous filesystem call.
    ///
    /// The interval between sweeps backs off exponentially, per call, so a
    /// waiter that has been queued for a long time stops hammering a shared
    /// filesystem with metadata operations. Every call starts again from
    /// [`SLOT_RETRY_INITIAL_INTERVAL_MILLIS`], because each call represents
    /// a fresh contender rather than a continuation of an earlier wait.
    async fn acquire_slot(&self) -> Result<Option<LockedFile>> {
        let Some(limit) = self.max_concurrent_pulls else {
            return Ok(None);
        };

        let slots_dir = self
            .cache_dir
            .join(COORDINATION_DIR_NAME)
            .join(SLOTS_DIR_NAME);
        let mut failed_sweeps = 0;
        loop {
            let dir = slots_dir.clone();
            let lock = tokio::task::spawn_blocking(move || try_acquire_any_slot(&dir, limit))
                .await
                .context("failed to join Apptainer image cache slot-acquisition task")??;

            if let Some(lock) = lock {
                return Ok(Some(lock));
            }

            tokio::time::sleep(jittered_slot_retry_delay(failed_sweeps)).await;
            failed_sweeps = failed_sweeps.saturating_add(1);
        }
    }
}

/// Removes partial files left beside `final_file_name` in `parent` by a
/// crashed pull.
///
/// Only regular files whose name begins with `{final_file_name}.partial.`
/// and that have not been modified within [`STALE_PARTIAL_MIN_AGE`] are
/// removed, so a partial file belonging to a pull that is still running
/// elsewhere is left alone. Cleanup is opportunistic; a directory that
/// cannot be read, an entry whose metadata cannot be inspected, and a file
/// that cannot be removed are all reported and then ignored, because none of
/// them prevents the pull that follows from succeeding.
async fn remove_stale_partials(parent: &Path, final_file_name: &str) {
    let prefix = format!("{final_file_name}.partial.");
    let mut entries = match tokio::fs::read_dir(parent).await {
        Ok(entries) => entries,
        Err(e) => {
            warn!(
                e = %e,
                "failed to scan Apptainer image cache directory `{path}` for abandoned partial \
                 files",
                path = parent.display()
            );
            return;
        }
    };

    loop {
        let entry = match entries.next_entry().await {
            Ok(Some(entry)) => entry,
            Ok(None) => return,
            Err(e) => {
                warn!(
                    e = %e,
                    "failed to scan Apptainer image cache directory `{path}` for abandoned \
                     partial files",
                    path = parent.display()
                );
                return;
            }
        };

        if !entry.file_name().to_string_lossy().starts_with(&prefix) {
            continue;
        }

        let path = entry.path();
        let metadata = match entry.metadata().await {
            Ok(metadata) => metadata,
            Err(e) => {
                warn!(
                    e = %e,
                    "failed to inspect the Apptainer image cache partial file `{path}`",
                    path = path.display()
                );
                continue;
            }
        };

        if !metadata.is_file() {
            continue;
        }

        let stale = metadata
            .modified()
            .map(|modified| {
                // A file modified in the future, which `elapsed` reports as an
                // error, is treated as recent so a skewed clock cannot cause a
                // live pull's file to be deleted.
                modified
                    .elapsed()
                    .is_ok_and(|age| age >= STALE_PARTIAL_MIN_AGE)
            })
            .unwrap_or_else(|e| {
                warn!(
                    e = %e,
                    "failed to read the modification time of the Apptainer image cache partial \
                     file `{path}`",
                    path = path.display()
                );
                false
            });
        if !stale {
            continue;
        }

        match tokio::fs::remove_file(&path).await {
            Ok(()) => debug!(
                path = %path.display(),
                "removed an abandoned Apptainer image cache partial file"
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => warn!(
                e = %e,
                "failed to remove the abandoned Apptainer image cache partial file `{path}`",
                path = path.display()
            ),
        }
    }
}

/// Returns the error reported for a request that observed a recorded
/// failure marker whose backoff has not yet elapsed.
///
/// Centralized so both the gate before a concurrency slot is acquired and
/// the gate after one is acquired report a replayed failure identically.
fn replayed_failure_error(container: &ContainerSource, marker: &FailureMarker) -> anyhow::Error {
    anyhow!(
        "cached Apptainer pull failure for `{container:#}`; no pull was attempted; \
         {consecutive_failures} consecutive failures are recorded and a new attempt becomes \
         eligible at {retry_at}; the recorded error was: {error}",
        consecutive_failures = marker.consecutive_failures,
        retry_at = marker.retry_at.to_rfc3339_opts(SecondsFormat::Secs, true),
        error = marker.error,
    )
}

/// Performs one complete, synchronous sweep over the fixed slot files
/// `0..limit` under `slots_dir`, returning the first one that can be locked
/// exclusively without blocking, or `None` if every slot is currently held.
///
/// This function itself performs synchronous `open()` and advisory-lock
/// system calls (via [`LockedFile::try_acquire_exclusive`]) and must only be
/// called from inside [`tokio::task::spawn_blocking`], as
/// [`ApptainerImageCache::acquire_slot`] does, never directly from an async
/// context.
fn try_acquire_any_slot(slots_dir: &Path, limit: usize) -> Result<Option<LockedFile>> {
    for i in 0..limit {
        let slot_path = slots_dir.join(format!("{i}.lock"));
        if let Some(lock) = LockedFile::try_acquire_exclusive(&slot_path)? {
            return Ok(Some(lock));
        }
    }

    Ok(None)
}

/// Returns the nominal slot retry interval, in milliseconds, that applies
/// after `failed_sweeps` sweeps have already found every slot held.
///
/// The interval starts at [`SLOT_RETRY_INITIAL_INTERVAL_MILLIS`] and doubles
/// after each failed sweep until it reaches
/// [`SLOT_RETRY_MAX_INTERVAL_MILLIS`], where it stays.
fn slot_retry_interval_millis(failed_sweeps: u32) -> u64 {
    (SLOT_RETRY_INITIAL_INTERVAL_MILLIS << failed_sweeps.min(SLOT_RETRY_MAX_DOUBLINGS))
        .min(SLOT_RETRY_MAX_INTERVAL_MILLIS)
}

/// Returns a slot retry delay sampled uniformly from the nominal interval
/// for `failed_sweeps`, plus or minus that interval divided by
/// [`SLOT_RETRY_JITTER_DIVISOR`], inclusive.
///
/// Without jitter, many Sprocket processes contending for the same fixed
/// slot files would tend to wake and sweep them at the same moment, over
/// and over, rather than spreading their attempts out. Without backing off,
/// a waiter that has been queued behind a long pull would keep sweeping
/// every few tens of milliseconds for the whole pull, which is expensive on
/// the shared filesystem the slot files live on. The file locks remain the
/// sole source of truth for who holds a slot; this delay only changes when a
/// process next attempts a sweep, never whether that sweep succeeds.
///
/// The returned delay is always at least half and at most one and a half
/// times the nominal interval, and therefore never exceeds one and a half
/// times [`SLOT_RETRY_MAX_INTERVAL_MILLIS`].
fn jittered_slot_retry_delay(failed_sweeps: u32) -> Duration {
    let nominal = slot_retry_interval_millis(failed_sweeps);
    let jitter = nominal / SLOT_RETRY_JITTER_DIVISOR;
    Duration::from_millis(rand::random_range(nominal - jitter..=nominal + jitter))
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

    /// Writes a fake `apptainer`-like executable to `path` that increments
    /// `counter_path` on every invocation, under the same portable
    /// `mkdir`-based lock [`write_waiting_executable`] uses, then either
    /// succeeds by writing [`FAKE_IMAGE_BYTES`] to its destination argument
    /// if `succeed_flag_path` exists, or fails with output the cache
    /// classifies as a permanent failure otherwise.
    #[cfg(unix)]
    fn write_toggleable_executable(path: &Path, counter_path: &Path, succeed_flag_path: &Path) {
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
if [ -f "{flag}" ]; then
  printf '%s' '{bytes}' > "$dest"
  exit 0
fi
echo '403 (Forbidden)' >&2
exit 1
"#,
            counter = counter_path.display(),
            flag = succeed_flag_path.display(),
            bytes = FAKE_IMAGE_BYTES,
        );

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
    async fn same_name_different_protocol_are_not_aliased() {
        // `Docker` and `Oras` sources sharing the same inner name string must
        // not collide: `ContainerSource`'s non-alternate `Display` omits the
        // protocol, so hashing `container.to_string()` would alias
        // `docker://shared-name:latest` and `oras://shared-name:latest`,
        // making them share a per-image lock and failure marker even though
        // they are different images.
        let docker = ContainerSource::Docker("shared-name:latest".to_string());
        let oras = ContainerSource::Oras("shared-name:latest".to_string());
        assert_ne!(
            image_key(&docker),
            image_key(&oras),
            "image_key must be protocol-preserving so that different protocols sharing the same \
             name are not aliased to the same coordination key"
        );

        let dir = tempfile::tempdir().unwrap();
        let cache = ApptainerImageCache::new_uncoordinated(dir.path(), None)
            .await
            .unwrap();

        let exe_docker = dir.path().join("fake-apptainer-docker");
        let counter_docker = dir.path().join("counter-docker");
        let release_docker = dir.path().join("release-docker");
        write_waiting_executable(&exe_docker, &counter_docker, &release_docker, 0);

        let exe_oras = dir.path().join("fake-apptainer-oras");
        let counter_oras = dir.path().join("counter-oras");
        let release_oras = dir.path().join("release-oras");
        write_waiting_executable(&exe_oras, &counter_oras, &release_oras, 0);

        let final_docker = dir.path().join("docker-image.sif");
        let final_oras = dir.path().join("oras-image.sif");

        let task_docker = {
            let cache = Arc::clone(&cache);
            let exe = exe_docker.clone();
            let container = docker.clone();
            let final_path = final_docker.clone();
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

        // Give the Docker pull a head start. Under the aliasing bug this
        // would leave it holding the (incorrectly) shared per-image lock, so
        // the Oras pull below would never start until the Docker pull's
        // release file is written.
        wait_until(Duration::from_secs(5), || {
            read_counter(&counter_docker) >= 1
        })
        .await;

        let task_oras = {
            let cache = Arc::clone(&cache);
            let exe = exe_oras.clone();
            let container = oras.clone();
            let final_path = final_oras.clone();
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

        // The Oras pull must start even though the Docker pull for the same
        // name is still waiting on its own release file: distinct protocols
        // must not share coordination.
        wait_until(Duration::from_secs(5), || read_counter(&counter_oras) >= 1).await;

        std::fs::write(&release_docker, b"go").unwrap();
        std::fs::write(&release_oras, b"go").unwrap();

        task_docker
            .await
            .unwrap()
            .unwrap()
            .expect("docker pull should succeed");
        task_oras
            .await
            .unwrap()
            .unwrap()
            .expect("oras pull should succeed");
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

    #[test]
    fn slot_retry_interval_follows_expected_progression() {
        // Fifty milliseconds through two seconds, doubling after each failed
        // sweep, then flat at the two second cap.
        let expected = [50, 100, 200, 400, 800, 1_600, 2_000, 2_000, 2_000, 2_000];
        for (failed_sweeps, &millis) in expected.iter().enumerate() {
            assert_eq!(
                slot_retry_interval_millis(failed_sweeps as u32),
                millis,
                "unexpected nominal slot retry interval after {failed_sweeps} failed sweeps"
            );
        }

        assert_eq!(
            slot_retry_interval_millis(u32::MAX),
            SLOT_RETRY_MAX_INTERVAL_MILLIS,
            "an unbounded number of failed sweeps must stay at the capped interval"
        );
    }

    #[test]
    fn jittered_slot_retry_delay_stays_within_bounds() {
        // Samples the helper many times rather than asserting on the
        // distribution: the requirement is only that every sample stays within
        // half of the nominal interval on either side, not that the underlying
        // RNG is unbiased.
        let ceiling = Duration::from_millis(
            SLOT_RETRY_MAX_INTERVAL_MILLIS + SLOT_RETRY_MAX_INTERVAL_MILLIS / 2,
        );

        for failed_sweeps in 0..12u32 {
            let nominal = slot_retry_interval_millis(failed_sweeps);
            let jitter = nominal / SLOT_RETRY_JITTER_DIVISOR;
            assert!(
                jitter > 0,
                "the jitter applied after {failed_sweeps} failed sweeps must never collapse to \
                 zero"
            );

            let low = Duration::from_millis(nominal - jitter);
            let high = Duration::from_millis(nominal + jitter);
            for _ in 0..1_000 {
                let delay = jittered_slot_retry_delay(failed_sweeps);
                assert!(
                    delay >= low && delay <= high,
                    "sampled slot retry delay {delay:?} for {failed_sweeps} failed sweeps fell \
                     outside the expected {low:?}..={high:?} bound"
                );
                assert!(
                    delay <= ceiling,
                    "sampled slot retry delay {delay:?} exceeded the {ceiling:?} ceiling derived \
                     from the capped interval"
                );
            }
        }
    }

    #[test]
    fn slot_retry_delay_backs_off_across_sweeps() {
        // The shortest delay a long-queued waiter can draw must exceed the
        // longest delay a fresh waiter can draw, so a waiter that has been
        // queued behind a long pull stops sweeping the shared filesystem at the
        // initial rate.
        let first_high = Duration::from_millis(
            SLOT_RETRY_INITIAL_INTERVAL_MILLIS
                + SLOT_RETRY_INITIAL_INTERVAL_MILLIS / SLOT_RETRY_JITTER_DIVISOR,
        );
        let late_low = Duration::from_millis(
            SLOT_RETRY_MAX_INTERVAL_MILLIS
                - SLOT_RETRY_MAX_INTERVAL_MILLIS / SLOT_RETRY_JITTER_DIVISOR,
        );
        assert!(late_low > first_high);

        for _ in 0..1_000 {
            assert!(
                jittered_slot_retry_delay(0) <= first_high,
                "a fresh waiter must keep sweeping at the initial rate"
            );
            assert!(
                jittered_slot_retry_delay(SLOT_RETRY_MAX_DOUBLINGS) >= late_low,
                "a long-queued waiter must have backed off well beyond the initial rate"
            );
        }
    }

    #[tokio::test]
    async fn acquire_slot_waits_across_multiple_sweeps_then_succeeds() {
        // Directly protects the `acquire_slot` async boundary: each retry
        // sweep over the fixed slot files runs inside
        // `tokio::task::spawn_blocking`, but the surrounding loop must still
        // behave exactly as before from the caller's perspective (wait while
        // the only slot is held, then succeed once it is released), across
        // several real sweep-and-sleep iterations.
        let dir = tempfile::tempdir().unwrap();
        let cache = ApptainerImageCache::new_uncoordinated(dir.path(), Some(1))
            .await
            .unwrap();
        cache
            .ensure_initialized()
            .await
            .expect("the coordination layout should initialize");

        let held = cache
            .acquire_slot()
            .await
            .unwrap()
            .expect("the only slot should be free initially");

        let waiter = {
            let cache = Arc::clone(&cache);
            tokio::spawn(async move { cache.acquire_slot().await })
        };

        // The first two retries can sleep at most one and a half times the
        // initial interval and then one and a half times twice that interval
        // (see `jittered_slot_retry_delay`), so waiting for the sum of those two
        // worst cases forces the waiter through at least three full sweeps, each
        // a separate `spawn_blocking` call, before the slot is released below.
        let worst_case_first_two_retries = Duration::from_millis(
            slot_retry_interval_millis(0)
                + slot_retry_interval_millis(0) / SLOT_RETRY_JITTER_DIVISOR
                + slot_retry_interval_millis(1)
                + slot_retry_interval_millis(1) / SLOT_RETRY_JITTER_DIVISOR,
        );
        tokio::time::sleep(worst_case_first_two_retries).await;
        assert!(
            !waiter.is_finished(),
            "the waiter must still be retrying while the only slot is held"
        );

        drop(held);

        let reacquired = waiter
            .await
            .unwrap()
            .unwrap()
            .expect("the waiter should acquire the slot once it is released");
        drop(reacquired);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn independent_coordinator_rejects_mismatched_policy() {
        let dir = tempfile::tempdir().unwrap();
        let first = ApptainerImageCache::new_uncoordinated(dir.path(), Some(2))
            .await
            .expect("first coordinator should be created");

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
        assert_eq!(read_counter(&counter), 1);

        let second = ApptainerImageCache::new_uncoordinated(dir.path(), None)
            .await
            .expect("constructing a coordinator must not touch the recorded policy");

        // The mismatch is detected while initializing the coordination layout,
        // which happens after the cache misses but before any pull is started.
        let error = second
            .pull(
                exe.to_str().unwrap(),
                &ContainerSource::Docker("test/policy-image-other:latest".to_string()),
                &dir.path().join("policy-image-other.sif"),
                CancellationToken::new(),
            )
            .await
            .expect_err("a coordinator requesting a different policy should fail");
        assert_eq!(
            read_counter(&counter),
            1,
            "a policy mismatch must be reported before any pull is attempted"
        );

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
            "a partial file too recent to be considered abandoned must never be treated as a \
             cached image nor removed"
        );
        assert_eq!(
            read_counter(&counter),
            1,
            "the cache must still perform a real pull despite the stale partial file"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn successful_publish_survives_a_failed_marker_cleanup() {
        // A publish that has already renamed its temporary file into place is
        // complete; failing to delete the now-stale failure marker afterwards
        // must not turn that success into an error. The stale marker is harmless
        // because the final path fast path runs before any marker read.
        let dir = tempfile::tempdir().unwrap();
        let cache = ApptainerImageCache::new_uncoordinated(dir.path(), None)
            .await
            .unwrap();

        let exe = dir.path().join("fake-apptainer-cleanup");
        let counter = dir.path().join("counter");
        let release = dir.path().join("release");
        write_waiting_executable(&exe, &counter, &release, 0);

        let container = ContainerSource::Docker("test/cleanup-image:latest".to_string());
        let final_path = dir.path().join("cleanup-image.sif");
        let failures_dir = dir
            .path()
            .join(COORDINATION_DIR_NAME)
            .join(FAILURES_DIR_NAME);
        let failure_path = failures_dir.join(format!("{}.json", image_key(&container)));

        std::fs::create_dir_all(&failures_dir).unwrap();
        let now = Utc::now();
        std::fs::write(
            &failure_path,
            serde_json::to_vec_pretty(&FailureMarker {
                version: FAILURE_MARKER_VERSION,
                consecutive_failures: 3,
                error: "an earlier pull failed".to_string(),
                failed_at: now - chrono::Duration::seconds(10),
                retry_at: now - chrono::Duration::seconds(1),
            })
            .unwrap(),
        )
        .unwrap();

        let pull = {
            let cache = Arc::clone(&cache);
            let exe = exe.clone();
            let container = container.clone();
            let final_path = final_path.clone();
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

        // Both marker reads happen before the executable is spawned, so replacing
        // the marker now only affects the post-publish cleanup. Replacing the
        // marker file with a non-empty directory makes `unlink` fail regardless
        // of the caller's privileges rather than relying on file permissions,
        // which a privileged caller would bypass.
        wait_until(Duration::from_secs(5), || read_counter(&counter) >= 1).await;
        std::fs::remove_file(&failure_path).unwrap();
        std::fs::create_dir(&failure_path).unwrap();
        std::fs::write(failure_path.join("not-a-marker"), b"blocks removal").unwrap();

        std::fs::write(&release, b"go").unwrap();

        let path = pull
            .await
            .unwrap()
            .expect("a publish whose marker cleanup fails must still succeed")
            .expect("the pull should not have been cancelled");
        assert_eq!(path, final_path);
        assert_eq!(
            std::fs::read(&final_path).unwrap(),
            FAKE_IMAGE_BYTES.as_bytes(),
            "the published image must contain the pulled bytes"
        );
        assert!(
            failure_path.is_dir(),
            "the un-removable marker should still be present"
        );

        // A later request takes the final path fast path, so the stale marker is
        // never consulted again.
        let again = cache
            .pull(
                exe.to_str().unwrap(),
                &container,
                &final_path,
                CancellationToken::new(),
            )
            .await
            .expect("the published image should still resolve")
            .expect("the pull should not have been cancelled");
        assert_eq!(again, final_path);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn old_partial_files_are_removed_and_recent_ones_are_kept() {
        // A crashed process can leave a uniquely named partial file beside the
        // final SIF. Old ones are swept away before a new pull, but recent ones
        // may still belong to a live pull in another process (the legacy path
        // mapping can alias distinct container sources onto one image lock), so
        // they are left alone. Files that do not match the final name's partial
        // prefix are never touched.
        let dir = tempfile::tempdir().unwrap();
        let cache = ApptainerImageCache::new_uncoordinated(dir.path(), None)
            .await
            .unwrap();

        let final_path = dir.path().join("partials.sif");
        let old_partial = dir
            .path()
            .join("partials.sif.partial.4242.00000000deadbeef");
        let recent_partial = dir
            .path()
            .join("partials.sif.partial.4243.00000000feedface");
        let other_image_partial = dir.path().join("other.sif.partial.4244.00000000cafed00d");
        let unrelated = dir.path().join("partials.sif.notes");

        for path in [
            &old_partial,
            &recent_partial,
            &other_image_partial,
            &unrelated,
        ] {
            std::fs::write(path, b"leftover").unwrap();
        }

        // Ages the candidate deterministically rather than waiting out the real
        // staleness threshold.
        std::fs::File::options()
            .write(true)
            .open(&old_partial)
            .unwrap()
            .set_modified(std::time::SystemTime::now() - Duration::from_secs(48 * 60 * 60))
            .unwrap();

        let exe = dir.path().join("fake-apptainer-partials");
        let counter = dir.path().join("counter");
        let release = dir.path().join("release");
        write_waiting_executable(&exe, &counter, &release, 0);
        std::fs::write(&release, b"go").unwrap();

        let path = cache
            .pull(
                exe.to_str().unwrap(),
                &ContainerSource::Docker("test/partials-image:latest".to_string()),
                &final_path,
                CancellationToken::new(),
            )
            .await
            .unwrap()
            .expect("the pull should succeed");

        assert_eq!(path, final_path);
        assert!(
            !old_partial.exists(),
            "an abandoned partial file older than the staleness threshold should be removed"
        );
        assert!(
            recent_partial.exists(),
            "a recently modified partial file may belong to a live pull and must be kept"
        );
        assert!(
            other_image_partial.exists(),
            "a partial file belonging to a different final image must never be removed"
        );
        assert!(
            unrelated.exists(),
            "a file that does not match the partial prefix must never be removed"
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

        let failure_path = dir
            .path()
            .join(COORDINATION_DIR_NAME)
            .join(FAILURES_DIR_NAME)
            .join(format!("{}.json", image_key(&container)));

        // Advance past the first failure's one second backoff delay by rewriting the
        // marker's `retry_at` directly, without pausing or advancing Tokio's clock
        // (which does not control the `Utc::now()` timestamps markers use).
        let mut marker: FailureMarker =
            serde_json::from_slice(&std::fs::read(&failure_path).unwrap()).unwrap();
        marker.retry_at = Utc::now() - chrono::Duration::seconds(1);
        std::fs::write(&failure_path, serde_json::to_vec_pretty(&marker).unwrap()).unwrap();

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

        let marker: FailureMarker =
            serde_json::from_slice(&std::fs::read(&failure_path).unwrap()).unwrap();
        assert_eq!(
            marker.consecutive_failures, 2,
            "the marker should record two consecutive failures"
        );
    }

    #[test]
    fn retry_delay_follows_expected_schedule() {
        // One second through sixty-four seconds, doubling for each additional
        // consecutive failure, then flat at the sixty-four second cap for every
        // failure count beyond the seventh.
        let expected_secs = [1, 2, 4, 8, 16, 32, 64, 64, 64, 64];
        for (i, &secs) in expected_secs.iter().enumerate() {
            let consecutive_failures = (i + 1) as u32;
            assert_eq!(
                retry_delay(consecutive_failures),
                Duration::from_secs(secs),
                "unexpected retry delay for {consecutive_failures} consecutive failures"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failure_backoff() {
        // Marker timestamps are `Utc::now()`, which a paused Tokio clock does not
        // control, so this test never pauses or advances Tokio time. Instead it
        // rewrites the on-disk marker's `retry_at` directly to simulate the
        // backoff deadline elapsing; the cache treats that file as
        // authoritative, so this has the same observable effect on eligibility
        // as real time actually passing, without an eight-step test waiting up to
        // 191 seconds of real delay.
        let dir = tempfile::tempdir().unwrap();
        let cache = ApptainerImageCache::new_uncoordinated(dir.path(), None)
            .await
            .unwrap();

        let exe = dir.path().join("fake-apptainer-backoff");
        let counter = dir.path().join("counter");
        // Never created, so the fake executable always takes its failing branch.
        let succeed_flag = dir.path().join("succeed");
        write_toggleable_executable(&exe, &counter, &succeed_flag);

        let container = ContainerSource::Docker("test/backoff-image:latest".to_string());
        let final_path = dir.path().join("backoff-image.sif");
        let failure_path = dir
            .path()
            .join(COORDINATION_DIR_NAME)
            .join(FAILURES_DIR_NAME)
            .join(format!("{}.json", image_key(&container)));

        // One consecutive failure doubles the delay from one second, capping at
        // sixty-four seconds from the seventh failure onward.
        let expected_delay_secs = [1, 2, 4, 8, 16, 32, 64, 64];

        for (i, &expected_secs) in expected_delay_secs.iter().enumerate() {
            let invocations_before = read_counter(&counter);

            let error = cache
                .pull(
                    exe.to_str().unwrap(),
                    &container,
                    &final_path,
                    CancellationToken::new(),
                )
                .await
                .expect_err("the eligible pull should fail");
            let message = format!("{error:#}");
            assert!(message.contains("403 (Forbidden)"));
            assert!(
                !message.contains("no pull was attempted"),
                "a failure from a real attempt must not be reported as a replayed one at step \
                 {i}: {message}"
            );
            assert_eq!(
                read_counter(&counter),
                invocations_before + 1,
                "the eligible request at step {i} should cause exactly one new invocation"
            );

            let marker: FailureMarker =
                serde_json::from_slice(&std::fs::read(&failure_path).unwrap()).unwrap();
            assert_eq!(
                marker.consecutive_failures,
                (i + 1) as u32,
                "unexpected consecutive failure count at step {i}"
            );
            assert_eq!(
                (marker.retry_at - marker.failed_at).to_std().unwrap(),
                Duration::from_secs(expected_secs),
                "unexpected backoff delay recorded at step {i}"
            );

            let invocations_before_retry = read_counter(&counter);
            let blocked_error = cache
                .pull(
                    exe.to_str().unwrap(),
                    &container,
                    &final_path,
                    CancellationToken::new(),
                )
                .await
                .expect_err("a request issued before retry_at should still fail");
            assert_eq!(
                read_counter(&counter),
                invocations_before_retry,
                "a request issued before retry_at must not cause a new invocation at step {i}"
            );
            let blocked_message = format!("{blocked_error:#}");
            assert!(
                blocked_message.contains("403 (Forbidden)"),
                "a replayed failure should quote the recorded error at step {i}: {blocked_message}"
            );
            assert!(
                blocked_message.contains("no pull was attempted"),
                "a replayed failure should say no pull was attempted at step {i}: \
                 {blocked_message}"
            );
            assert!(
                blocked_message.contains(&format!(
                    "{count} consecutive failure",
                    count = marker.consecutive_failures
                )),
                "a replayed failure should report the consecutive failure count at step {i}: \
                 {blocked_message}"
            );
            assert!(
                blocked_message
                    .contains(&marker.retry_at.to_rfc3339_opts(SecondsFormat::Secs, true)),
                "a replayed failure should report when a retry becomes eligible at step {i}: \
                 {blocked_message}"
            );

            // Advance through the deadline by rewriting the marker's `retry_at` into the
            // past.
            let mut past_marker = marker;
            past_marker.retry_at = Utc::now() - chrono::Duration::seconds(1);
            std::fs::write(
                &failure_path,
                serde_json::to_vec_pretty(&past_marker).unwrap(),
            )
            .unwrap();
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn success_resets_backoff() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ApptainerImageCache::new_uncoordinated(dir.path(), None)
            .await
            .unwrap();

        let exe = dir.path().join("fake-apptainer-toggle");
        let counter = dir.path().join("counter");
        let succeed_flag = dir.path().join("succeed");
        write_toggleable_executable(&exe, &counter, &succeed_flag);

        let container = ContainerSource::Docker("test/reset-image:latest".to_string());
        let final_path = dir.path().join("reset-image.sif");
        let failure_path = dir
            .path()
            .join(COORDINATION_DIR_NAME)
            .join(FAILURES_DIR_NAME)
            .join(format!("{}.json", image_key(&container)));

        // `succeed_flag` does not exist yet, so this first pull fails and records a
        // marker with a one second delay.
        let error = cache
            .pull(
                exe.to_str().unwrap(),
                &container,
                &final_path,
                CancellationToken::new(),
            )
            .await
            .expect_err("the first pull should fail");
        assert!(format!("{error:#}").contains("403 (Forbidden)"));

        let marker: FailureMarker =
            serde_json::from_slice(&std::fs::read(&failure_path).unwrap()).unwrap();
        assert_eq!(marker.consecutive_failures, 1);
        assert_eq!(
            (marker.retry_at - marker.failed_at).to_std().unwrap(),
            Duration::from_secs(1),
            "the first failure should record a one second delay"
        );

        // Advance past the one second delay by rewriting the marker's `retry_at`
        // directly, the same on-disk authoritative state a waiting process
        // would eventually observe once real time passed, without pausing or
        // advancing Tokio's clock (which does not control `Utc::now()`).
        let mut past_marker = marker;
        past_marker.retry_at = Utc::now() - chrono::Duration::seconds(1);
        std::fs::write(
            &failure_path,
            serde_json::to_vec_pretty(&past_marker).unwrap(),
        )
        .unwrap();

        // Let the fake executable succeed and pull again.
        std::fs::write(&succeed_flag, b"go").unwrap();
        let path = cache
            .pull(
                exe.to_str().unwrap(),
                &container,
                &final_path,
                CancellationToken::new(),
            )
            .await
            .unwrap()
            .expect("the second pull should succeed once the backoff has elapsed");
        assert_eq!(path, final_path);
        assert!(
            !failure_path.exists(),
            "a successful publish must remove the failure marker"
        );

        // Delete the final SIF to force another pull, and let the fake executable fail
        // again.
        std::fs::remove_file(&final_path).unwrap();
        std::fs::remove_file(&succeed_flag).unwrap();
        let error = cache
            .pull(
                exe.to_str().unwrap(),
                &container,
                &final_path,
                CancellationToken::new(),
            )
            .await
            .expect_err("the forced pull should fail again");
        assert!(format!("{error:#}").contains("403 (Forbidden)"));

        let marker: FailureMarker =
            serde_json::from_slice(&std::fs::read(&failure_path).unwrap()).unwrap();
        assert_eq!(
            marker.consecutive_failures, 1,
            "a successful publish should reset the failure count"
        );
        assert_eq!(
            (marker.retry_at - marker.failed_at).to_std().unwrap(),
            Duration::from_secs(1),
            "the new marker should return to a one second delay"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancelled_waiter_does_not_cancel_pull() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ApptainerImageCache::new_uncoordinated(dir.path(), None)
            .await
            .unwrap();

        let exe = dir.path().join("fake-apptainer-cancel");
        let counter = dir.path().join("counter");
        let release = dir.path().join("release");
        write_waiting_executable(&exe, &counter, &release, 0);

        let container = ContainerSource::Docker("test/cancel-image:latest".to_string());
        let final_path = dir.path().join("cancel-image.sif");

        let cancel_token = CancellationToken::new();
        let cancelled_waiter = {
            let cache = Arc::clone(&cache);
            let exe = exe.clone();
            let container = container.clone();
            let final_path = final_path.clone();
            let token = cancel_token.clone();
            tokio::spawn(async move {
                cache
                    .pull(exe.to_str().unwrap(), &container, &final_path, token)
                    .await
            })
        };

        let surviving_waiter = {
            let cache = Arc::clone(&cache);
            let exe = exe.clone();
            let container = container.clone();
            let final_path = final_path.clone();
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

        wait_until(Duration::from_secs(5), || read_counter(&counter) >= 1).await;

        cancel_token.cancel();
        let cancelled_result = cancelled_waiter
            .await
            .unwrap()
            .expect("a cancelled waiter should not itself return an error");
        assert!(
            cancelled_result.is_none(),
            "a cancelled waiter should observe `Ok(None)`"
        );

        assert_eq!(
            read_counter(&counter),
            1,
            "cancelling one waiter must not trigger a second invocation of the shared pull"
        );
        assert!(
            !surviving_waiter.is_finished(),
            "the surviving waiter must still be waiting on the shared pull"
        );

        std::fs::write(&release, b"go").unwrap();

        let path = surviving_waiter
            .await
            .unwrap()
            .unwrap()
            .expect("the surviving waiter should still receive the pull result");
        assert_eq!(path, final_path);
        assert!(
            final_path.exists(),
            "the final SIF should exist after the shared pull completes"
        );
        assert_eq!(
            read_counter(&counter),
            1,
            "only one invocation total should have occurred despite the cancellation"
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

    #[tokio::test]
    async fn registry_prunes_entries_for_dropped_coordinators() {
        // Every run gets its own cache directory unless one is configured, so a
        // long-lived server would otherwise accumulate one permanently dead
        // registry entry per run.
        let dead_dirs: Vec<_> = (0..8).map(|_| tempfile::tempdir().unwrap()).collect();
        let mut coordinators = Vec::new();
        for dir in &dead_dirs {
            coordinators.push(
                ApptainerImageCache::get(dir.path(), None)
                    .await
                    .expect("a coordinator should be created for each run directory"),
            );
        }
        drop(coordinators);

        let before = registry_entries();
        for dir in &dead_dirs {
            assert!(
                before.iter().any(|path| path == dir.path()),
                "a dropped coordinator should still leave its dead entry behind until a later \
                 insertion prunes it"
            );
        }

        let live_dir = tempfile::tempdir().unwrap();
        let _live = ApptainerImageCache::get(live_dir.path(), None)
            .await
            .expect("a coordinator should be created for the live directory");

        let after = registry_entries();
        for dir in &dead_dirs {
            assert!(
                !after.iter().any(|path| path == dir.path()),
                "inserting a coordinator should prune entries whose coordinator was dropped"
            );
        }
        assert!(
            after.iter().any(|path| path == live_dir.path()),
            "the newly inserted coordinator should remain registered"
        );
        assert!(
            after.len() < before.len(),
            "pruning eight dead entries while inserting one must shrink the registry"
        );
    }
}
