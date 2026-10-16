//! Module for evaluation.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;

use anyhow::Result;
use cloud_copy::ContentDigest;
use cloud_copy::TransferEvent;
use crankshaft::events::Event as CrankshaftEvent;
use indexmap::IndexMap;
use regex::Regex;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::error;
use url::Url;
use wdl_analysis::Document;
use wdl_analysis::document::Task;
use wdl_analysis::types::Type;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;

use crate::Cache;
use crate::Engine;
use crate::EvaluationPath;
use crate::GuestPath;
use crate::HostPath;
use crate::Object;
use crate::Outputs;
use crate::Value;
use crate::backend::TaskExecutionConstraints;
use crate::backend::TaskExecutionResult;
use crate::config::FailureMode;
use crate::http::Location;
use crate::v1::requirements::ImageSource;

mod trie;
pub mod v1;

/// A name used whenever a file system "root" is mapped.
///
/// A root might be a root directory like `/` or `C:\`, but it also might be the root of a URL like `https://example.com`.
const ROOT_NAME: &str = ".root";

/// A constant to denote that no cancellation has occurred yet.
const CANCELLATION_STATE_NOT_CANCELED: u8 = 0;

/// A state bit to indicate that we're waiting for executing tasks to
/// complete.
///
/// This bit is mutually exclusive with the `CANCELING` bit.
const CANCELLATION_STATE_WAITING: u8 = 1;

/// A state bit to denote that we're waiting for executing tasks to cancel.
///
/// This bit is mutually exclusive with the `WAITING` bit.
const CANCELLATION_STATE_CANCELING: u8 = 2;

/// A state bit to denote that cancellation was the result of an error.
///
/// This bit will only be set if either the `CANCELING` bit or the `WAITING`
/// bit are set.
const CANCELLATION_STATE_ERROR: u8 = 4;

/// The mask to apply to the state for excluding the error bit.
const CANCELLATION_STATE_MASK: u8 = 0x3;

/// Represents the current state of a [`CancellationContext`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancellationContextState {
    /// The context has not been canceled yet.
    NotCanceled,
    /// The context has been canceled and is waiting for executing tasks to
    /// complete.
    Waiting,
    /// The context has been canceled and is waiting for executing tasks to
    /// cancel.
    Canceling,
}

impl CancellationContextState {
    /// Gets the context state from a raw state byte.
    fn from_raw(raw: u8) -> Self {
        match raw & CANCELLATION_STATE_MASK {
            CANCELLATION_STATE_NOT_CANCELED => Self::NotCanceled,
            CANCELLATION_STATE_WAITING => Self::Waiting,
            CANCELLATION_STATE_CANCELING => Self::Canceling,
            _ => unreachable!("unexpected cancellation context state"),
        }
    }

    /// Updates the context state and returns the new state.
    ///
    /// Returns `None` if the update is for an error and there has already been
    /// a cancellation (i.e. the update was not successful).
    fn update(mode: FailureMode, error: bool, state: &Arc<AtomicU8>) -> Option<Self> {
        // Update the provided state with the new state
        let previous_state = state
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |state| {
                // If updating for an error and there has been a cancellation,
                // bail out
                if error && state != CANCELLATION_STATE_NOT_CANCELED {
                    return None;
                }

                // Otherwise, calculate the new state
                let mut new_state = match state & CANCELLATION_STATE_MASK {
                    CANCELLATION_STATE_NOT_CANCELED => match mode {
                        FailureMode::Slow => CANCELLATION_STATE_WAITING,
                        FailureMode::Fast => CANCELLATION_STATE_CANCELING,
                    },
                    CANCELLATION_STATE_WAITING => CANCELLATION_STATE_CANCELING,
                    CANCELLATION_STATE_CANCELING => CANCELLATION_STATE_CANCELING,
                    _ => unreachable!("unexpected cancellation context state"),
                };

                // Mark the error bit upon error
                if error {
                    new_state |= CANCELLATION_STATE_ERROR;
                }

                // Return the new state along with the old error bit
                Some(new_state | (state & CANCELLATION_STATE_ERROR))
            })
            .ok()?;

        match previous_state & CANCELLATION_STATE_MASK {
            CANCELLATION_STATE_NOT_CANCELED => match mode {
                FailureMode::Slow => Some(Self::Waiting),
                FailureMode::Fast => Some(Self::Canceling),
            },
            CANCELLATION_STATE_WAITING => Some(Self::Canceling),
            CANCELLATION_STATE_CANCELING => Some(Self::Canceling),
            _ => unreachable!("unexpected cancellation context state"),
        }
    }
}

/// Represents context for cancelling workflow or task evaluation.
///
/// Uses a default failure mode of [`Slow`](FailureMode::Slow).
///
/// This type is cheaply cloned and all clones reference the same cancellation
/// state.
#[derive(Debug, Clone)]
pub struct CancellationContext {
    /// The failure mode for the cancellation context.
    mode: FailureMode,
    /// The state of the cancellation context.
    state: Arc<AtomicU8>,
    /// The parent context, consulted read-only when folding the effective
    /// state. `None` for a root context created by [`new`](Self::new).
    parent: Option<Arc<Self>>,
    /// The cancellation token that is canceled upon the first cancellation.
    first: CancellationToken,
    /// The cancellation token that is canceled upon the second cancellation
    /// when the failure mode is "slow" or upon the first cancellation when the
    /// failure mode is "fast".
    second: CancellationToken,
}

impl CancellationContext {
    /// Constructs a cancellation context for the given [`FailureMode`].
    ///
    /// If the provided `mode` is [`Slow`](FailureMode::Slow), the first call to
    /// [`cancel`](Self::cancel) will wait for currently executing tasks to
    /// complete; a subsequent call to [`cancel`](Self::cancel) will cancel the
    /// currently executing tasks.
    ///
    /// If the provided `mode` is [`Fast`](FailureMode::Fast), the first call to
    /// [`cancel`](Self::cancel) will cancel the currently executing tasks.
    pub fn new(mode: FailureMode) -> Self {
        Self {
            mode,
            state: Arc::new(CANCELLATION_STATE_NOT_CANCELED.into()),
            parent: None,
            first: CancellationToken::new(),
            second: CancellationToken::new(),
        }
    }

    /// Creates a new child cancellation context.
    ///
    /// The returned [`CancellationContext`] is bound to the parent: both token
    /// cancellation and the effective [`state`](Self::state) propagate from the
    /// parent to the child. Cancelling the child, however, never affects the
    /// parent or its other children.
    pub fn child(&self, mode: FailureMode) -> Self {
        Self {
            mode,
            state: Arc::new(CANCELLATION_STATE_NOT_CANCELED.into()),
            parent: Some(Arc::new(self.clone())),
            first: self.first.child_token(),
            second: self.second.child_token(),
        }
    }

    /// Folds this context's state with its ancestors' into an effective
    /// `(level, user_initiated)` pair.
    ///
    /// `level` is the masked state level, where
    /// `CANCELLATION_STATE_NOT_CANCELED` < `CANCELLATION_STATE_WAITING` <
    /// `CANCELLATION_STATE_CANCELING`. `user_initiated` is whether the
    /// effective cancellation was initiated by the user rather than by an
    /// error.
    ///
    /// The `(level, user_initiated)` pair is always taken from a single
    /// context rather than mixed across the hierarchy: an ancestor's pair
    /// wins only when its effective level is strictly greater than this
    /// context's. On a tie the local cause is authoritative, so a parent
    /// cancellation that arrives after a child has already entered the same
    /// level does not relabel the child's cause.
    fn effective(&self) -> (u8, bool) {
        let raw = self.state.load(Ordering::SeqCst);
        let level = raw & CANCELLATION_STATE_MASK;
        let user =
            level != CANCELLATION_STATE_NOT_CANCELED && (raw & CANCELLATION_STATE_ERROR == 0);

        match &self.parent {
            Some(parent) => match parent.effective() {
                (parent_level, parent_user) if parent_level > level => (parent_level, parent_user),
                _ => (level, user),
            },
            None => (level, user),
        }
    }

    /// Gets the effective [`CancellationContextState`] of this
    /// [`CancellationContext`], folding in any ancestor's cancellation.
    pub fn state(&self) -> CancellationContextState {
        CancellationContextState::from_raw(self.effective().0)
    }

    /// Performs a cancellation.
    ///
    /// Returns the current [`CancellationContextState`] which should be checked
    /// to ensure the desired cancellation occurred.
    ///
    /// This method will never return a
    /// [`CancellationContextState::NotCanceled`] state.
    #[must_use]
    pub fn cancel(&self) -> CancellationContextState {
        let state =
            CancellationContextState::update(self.mode, false, &self.state).expect("should update");

        match state {
            CancellationContextState::NotCanceled => panic!("should be canceled"),
            CancellationContextState::Waiting => self.first.cancel(),
            CancellationContextState::Canceling => {
                self.first.cancel();
                self.second.cancel();
            }
        }

        state
    }

    /// Gets the cancellation token that is canceled upon the first
    /// cancellation.
    ///
    /// The token will be canceled when [`CancellationContext::cancel`] is
    /// called and the resulting state is [`CancellationContextState::Waiting`]
    /// or [`CancellationContextState::Canceling`].
    ///
    /// Callers should _not_ directly cancel the returned token and instead call
    /// [`CancellationContext::cancel`].
    pub fn first(&self) -> &CancellationToken {
        &self.first
    }

    /// Gets the cancellation token that is canceled upon the second
    /// cancellation when the failure mode is "slow" or first cancellation when
    /// the failure mode is "fast".
    ///
    /// The token will be canceled when [`CancellationContext::cancel`] is
    /// called and the resulting state is
    /// [`CancellationContextState::Canceling`].
    ///
    /// Callers should _not_ directly cancel the returned token and instead call
    /// [`CancellationContext::cancel`].
    pub fn second(&self) -> &CancellationToken {
        &self.second
    }

    /// Determines if the user initiated the cancellation, considering any
    /// ancestor's cancellation.
    pub fn user_canceled(&self) -> bool {
        self.effective().1
    }

    /// Triggers a cancellation as a result of an error.
    ///
    /// If the context has already been canceled, this is a no-op.
    ///
    /// Otherwise, a cancellation is attempted and an error message is logged
    /// depending on the current state of the context.
    pub(crate) fn error(&self, error: &EvaluationError) {
        if let Some(state) = CancellationContextState::update(self.mode, true, &self.state) {
            let message: Cow<'_, str> = match error {
                EvaluationError::Canceled => "evaluation was canceled".into(),
                EvaluationError::Source(e) => e.diagnostic.message().into(),
                EvaluationError::Other(e) => format!("{e:#}").into(),
            };

            match state {
                CancellationContextState::NotCanceled => unreachable!("should be canceled"),
                CancellationContextState::Waiting => {
                    self.first.cancel();

                    error!(
                        "an evaluation error occurred: waiting for any executing tasks to \
                         complete: {message}"
                    );
                }
                CancellationContextState::Canceling => {
                    self.first.cancel();
                    self.second.cancel();

                    error!(
                        "an evaluation error occurred: waiting for any executing tasks to cancel: \
                         {message}"
                    );
                }
            }
        }
    }
}

impl Default for CancellationContext {
    fn default() -> Self {
        Self::new(FailureMode::Slow)
    }
}

/// The inner state of [`EvaluationHttpClient`].
struct EvaluationHttpClientInner {
    /// The engine associated with evaluation.
    engine: Engine,
    /// The evaluator associated with the client.
    events: Option<broadcast::Sender<TransferEvent>>,
    /// The cache for calls to the `download` method.
    downloads: Cache<Url, Location>,
    /// The cache for calls to the `upload` method.
    uploads: Cache<Url, ()>,
    /// The cache for calls to the `size` method.
    sizes: Cache<Url, Option<u64>>,
    /// The cache for calls to the `walk` method.
    walks: Cache<Url, Arc<[String]>>,
    /// The cache for calls to the `exists` method.
    exists: Cache<Url, bool>,
    /// The cache for calls to the `digests` method.
    digests: Cache<Url, Option<Arc<ContentDigest>>>,
}

/// A HTTP client implementation used for evaluation.
///
/// This type wraps an inner [`HttpClient`] and provides transfer events from a
/// related [`Engine`].
///
/// Successful calls to this type's methods will be cached for the evaluation.
///
/// This type is cheaply cloned.
#[derive(Clone)]
pub(crate) struct EvaluationHttpClient(Arc<EvaluationHttpClientInner>);

impl EvaluationHttpClient {
    /// Constructs a new HTTP evaluation client.
    ///
    /// # Panics
    ///
    /// Panics if the provided configuration specifies a zero for response cache
    /// capacity.
    pub fn new(engine: &Engine, events: &Events) -> Self {
        let capacity = NonZeroUsize::new(engine.config().http.response_cache_capacity as usize)
            .expect("the cache capacity cannot be zero");

        Self(
            EvaluationHttpClientInner {
                engine: engine.clone(),
                events: events.transfer().cloned(),
                downloads: Cache::new(capacity),
                uploads: Cache::new(capacity),
                sizes: Cache::new(capacity),
                walks: Cache::new(capacity),
                exists: Cache::new(capacity),
                digests: Cache::new(capacity),
            }
            .into(),
        )
    }

    /// Downloads a file or directory to a temporary path.
    pub async fn download(&self, source: &Url, token: &CancellationToken) -> Result<Location> {
        self.0
            .engine
            .http_client()
            .download(source, self.0.events.clone(), token, &self.0.downloads)
            .await
    }

    /// Uploads a local file or directory to a cloud storage URL.
    ///
    /// The destination URL is expected to be content-addressed (meaning
    /// specific to the content being uploaded).
    pub async fn upload(
        &self,
        source: &Path,
        destination: &Url,
        token: &CancellationToken,
    ) -> Result<()> {
        self.0
            .engine
            .http_client()
            .upload(
                source,
                destination,
                self.0.events.clone(),
                token,
                &self.0.uploads,
            )
            .await
    }

    /// Gets the size of a resource at a given URL.
    ///
    /// Returns `Ok(Some(_))` if the size is known.
    ///
    /// Returns `Ok(None)` if the URL is valid but the size cannot be
    /// determined.
    pub async fn size(&self, url: &Url, token: &CancellationToken) -> Result<Option<u64>> {
        self.0
            .engine
            .http_client()
            .size(url, token, &self.0.sizes)
            .await
    }

    /// Walks a given storage URL as if it were a directory.
    ///
    /// Returns a list of relative paths from the given URL that are in
    /// lexicographical order.
    ///
    /// If the given storage URL is not a directory, an empty list is returned.
    pub async fn walk(&self, url: &Url, token: &CancellationToken) -> Result<Arc<[String]>> {
        self.0
            .engine
            .http_client()
            .walk(url, token, &self.0.walks)
            .await
    }

    /// Determines if the given URL exists.
    ///
    /// Returns `Ok(true)` if a HEAD request returns success or if a walk of the
    /// URL returns at least one contained URL.
    pub async fn exists(&self, url: &Url, token: &CancellationToken) -> Result<bool> {
        self.0
            .engine
            .http_client()
            .exists(url, token, &self.0.exists)
            .await
    }

    /// Gets the content digest of the resource identified by the given URL.
    ///
    /// Returns `Ok(None)` if the resource has no associated content digest.
    pub async fn digest(
        &self,
        url: &Url,
        token: &CancellationToken,
    ) -> Result<Option<Arc<ContentDigest>>> {
        self.0
            .engine
            .http_client()
            .digest(url, token, &self.0.digests)
            .await
    }
}

/// The prefix of the name given to a task a backend runs on its own behalf
/// rather than on behalf of a WDL task.
///
/// The Docker backend's `chown` of a work directory is one: it is submitted to
/// Crankshaft and reported through [`CrankshaftEvent`] like any other task,
/// even though no WDL task corresponds to it. Consumers that describe a run's
/// tasks should ignore any Crankshaft task whose name carries this prefix,
/// since the engine never announces one through [`EngineEvent`].
///
/// The prefix contains a `.`, which a name minted for a WDL task can never
/// contain: those names are a WDL call path relative to the root workflow,
/// whose levels are joined by `--` and whose segments within a level are WDL
/// identifiers joined by `-`, followed by a `-` and an alphanumeric suffix.
pub const CLEANUP_TASK_NAME_PREFIX: &str = "cleanup.";

/// The separator a backend uses when it derives a name for a resubmission of a
/// task it runs on the user's behalf.
///
/// A backend may retry an execution attempt on its own — the TES backend
/// resubmits a preempted task, for example — without the evaluator minting a
/// new attempt name. Each such resubmission is announced through
/// [`EngineEvent::TaskRetrying`] and executes under a derived name of the form
/// `{name}{RESUBMIT_NAME_SEPARATOR}{n}`, where `{name}` is the
/// evaluator-minted attempt name and `{n}` is the 1-based resubmission
/// ordinal.
///
/// The separator can never appear in an evaluator-minted name: those names are
/// a WDL call path, whose levels are joined by `--` and whose segments within
/// a level are WDL identifiers joined by `-`, followed by a `-` and an
/// alphanumeric suffix.
pub const RESUBMIT_NAME_SEPARATOR: char = '~';

/// Derives the name for a backend-local resubmission of a task execution.
///
/// See [`RESUBMIT_NAME_SEPARATOR`] for the naming convention.
pub fn resubmit_task_name(name: &str, resubmission: u64) -> String {
    // Derive from the original attempt name so repeated resubmissions do not
    // accumulate separators.
    let base = name
        .split(RESUBMIT_NAME_SEPARATOR)
        .next()
        .expect("split always yields at least one element");
    format!("{base}{RESUBMIT_NAME_SEPARATOR}{resubmission}")
}

/// The cause of a task execution attempt being retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum RetryCause {
    /// The attempt exited with a code that the task's `return_codes`
    /// requirement does not accept.
    UnacceptableExitCode {
        /// The exit code of the failed attempt.
        code: i32,
    },
    /// The attempt was preempted by the execution environment.
    Preempted,
}

impl fmt::Display for RetryCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnacceptableExitCode { code } => {
                write!(f, "unacceptable exit code {code}")
            }
            Self::Preempted => write!(f, "preempted"),
        }
    }
}

/// A serializable snapshot of the resolved execution constraints for a task
/// execution attempt.
///
/// This mirrors the resource-related fields of the backend's
/// `TaskExecutionConstraints` at the moment the attempt is submitted for
/// execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TaskConstraintsSnapshot {
    /// The full URI of the primary container image the task requested.
    ///
    /// This is the first (highest priority) container candidate; `None` when
    /// the task runs directly on the host.
    pub container: Option<String>,
    /// The allocated number of CPUs.
    pub cpu: f64,
    /// The allocated memory, in bytes.
    pub memory: u64,
    /// One specification per allocated GPU (execution engine-specific).
    pub gpu: Vec<String>,
    /// One specification per allocated FPGA (execution engine-specific).
    pub fpga: Vec<String>,
    /// A map of disk mount points to the initial amount of disk space
    /// allocated, in bytes.
    pub disks: BTreeMap<String, i64>,
    /// The resolved `max_retries` requirement in force for the attempt.
    ///
    /// Both the attempt count and the cost of a run depend on the retry
    /// policy, so it is recorded alongside the constraints.
    pub max_retries: Option<u64>,
    /// The resolved `preemptible` hint: the number of times the task may be
    /// preempted before it must be run on non-preemptible resources.
    pub preemptible: Option<i64>,
    /// The resolved `max_cpu` hint, if one was provided.
    pub max_cpu: Option<f64>,
    /// The resolved `max_memory` hint in bytes, if one was provided.
    pub max_memory: Option<i64>,
}

impl From<&TaskExecutionConstraints> for TaskConstraintsSnapshot {
    fn from(constraints: &TaskExecutionConstraints) -> Self {
        Self {
            container: constraints.sources.first().map(ImageSource::uri),
            cpu: constraints.cpu,
            memory: constraints.memory,
            gpu: constraints.gpu.clone(),
            fpga: constraints.fpga.clone(),
            disks: constraints
                .disks
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            max_retries: None,
            preemptible: None,
            max_cpu: None,
            max_memory: None,
        }
    }
}

/// Represents an event from the WDL evaluation engine.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum EngineEvent {
    /// A task has started evaluation of an execution attempt.
    ///
    /// This is the first event emitted for any attempt and precedes both
    /// input localization and submission of the task to the backend.
    TaskInitializing {
        /// The id of the task being evaluated.
        id: String,
        /// The unique name of the task for this attempt.
        ///
        /// This is the same name later reported by
        /// [`CrankshaftEvent::TaskCreated`] when the attempt reaches the
        /// backend.
        name: String,
        /// The 0-based number of this execution attempt.
        attempt: u64,
    },
    /// A task has started localizing its inputs.
    ///
    /// Depending on the backend, localizing means either downloading remote
    /// inputs to the host or digesting and uploading local inputs to remote
    /// storage. The event is only emitted when there is at least one input to
    /// transfer.
    TaskLocalizing {
        /// The unique name of the task for this attempt.
        name: String,
    },
    /// A task execution attempt is being submitted to the execution backend.
    ///
    /// Carries the resolved execution constraints for the attempt. This event
    /// is not emitted when the attempt's result is served from the call
    /// cache.
    TaskExecuting {
        /// The unique name of the task for this attempt.
        name: String,
        /// The resolved execution constraints for this attempt.
        constraints: TaskConstraintsSnapshot,
    },
    /// A task execution attempt failed and is being retried.
    ///
    /// A retry from the evaluator mints a new attempt name, which is
    /// announced by the [`EngineEvent::TaskInitializing`] event that follows
    /// this one. A backend-local resubmission (see
    /// [`RESUBMIT_NAME_SEPARATOR`]) executes under the derived `next_name`
    /// without a fresh `TaskInitializing`.
    TaskRetrying {
        /// The name of the attempt that failed.
        prior_name: String,
        /// The name of the attempt about to be made.
        next_name: String,
        /// The cause of the retry.
        cause: RetryCause,
    },
    /// A cached task execution result was reused due to a call cache hit.
    ReusedCachedExecutionResult {
        /// The id of the task that reused a cached execution result.
        id: String,
        /// The unique name of the task for this attempt.
        name: String,
    },
    /// Resource usage the engine measured for a task execution attempt.
    ///
    /// Emitted by the engine after the attempt's execution returns, carrying
    /// whatever the engine could measure:
    ///
    /// * the disk space used by the attempt's work directory, when the work
    ///   directory is on a local file system (its size at the end of execution,
    ///   not a high-water mark, excluding scratch space written outside the
    ///   work directory); and
    /// * when resource usage measurement is enabled in the configuration, the
    ///   CPU time and (in containerized environments) peak cgroup memory
    ///   recorded by the measurement shim from within the execution
    ///   environment.
    ///
    /// This complements the scheduler-observed measurements a backend may
    /// report through Crankshaft's `TaskResourceUsage` event: consumers
    /// should merge the fields present in this snapshot over whatever the
    /// backend reported.
    TaskUsageMeasured {
        /// The unique name of the task for this attempt.
        name: String,
        /// The measured resource usage; only the measured fields are set.
        usage: crankshaft::events::TaskResourceUsage,
    },
    /// A locally running task has been parked by the engine due to insufficient
    /// resources.
    TaskParked,
    /// A locally running task has been unparked by the engine.
    TaskUnparked {
        /// Whether or not the task was unparked due to being canceled.
        canceled: bool,
    },
}

/// Represents events that may be sent during WDL evaluation.
///
/// This type is cheaply cloned.
#[derive(Debug, Clone, Default)]
pub struct Events {
    /// The WDL engine events channel.
    ///
    /// This is `None` when engine events are not enabled.
    engine: Option<broadcast::Sender<EngineEvent>>,
    /// The Crankshaft events channel.
    ///
    /// This is `None` when Crankshaft events are not enabled.
    crankshaft: Option<broadcast::Sender<CrankshaftEvent>>,
    /// The transfer events channel.
    ///
    /// This is `None` when transfer events are not enabled.
    transfer: Option<broadcast::Sender<TransferEvent>>,
}

impl Events {
    /// Constructs a new `Events` and enables subscribing to all event channels.
    pub fn new(capacity: usize) -> Self {
        Self {
            engine: Some(broadcast::Sender::new(capacity)),
            crankshaft: Some(broadcast::Sender::new(capacity)),
            transfer: Some(broadcast::Sender::new(capacity)),
        }
    }

    /// Constructs a new `Events` and disable subscribing to any event channel.
    pub fn disabled() -> Self {
        Self::default()
    }

    /// Subscribes to the WDL engine events channel.
    ///
    /// Returns `None` if WDL engine events are not enabled.
    pub fn subscribe_engine(&self) -> Option<broadcast::Receiver<EngineEvent>> {
        self.engine.as_ref().map(|s| s.subscribe())
    }

    /// Subscribes to the Crankshaft events channel.
    ///
    /// Returns `None` if Crankshaft events are not enabled.
    pub fn subscribe_crankshaft(&self) -> Option<broadcast::Receiver<CrankshaftEvent>> {
        self.crankshaft.as_ref().map(|s| s.subscribe())
    }

    /// Subscribes to the transfer events channel.
    ///
    /// Returns `None` if transfer events are not enabled.
    pub fn subscribe_transfer(&self) -> Option<broadcast::Receiver<TransferEvent>> {
        self.transfer.as_ref().map(|s| s.subscribe())
    }

    /// Gets the sender for the Crankshaft events.
    pub(crate) fn engine(&self) -> Option<&broadcast::Sender<EngineEvent>> {
        self.engine.as_ref()
    }

    /// Gets the sender for the Crankshaft events.
    pub(crate) fn crankshaft(&self) -> Option<&broadcast::Sender<CrankshaftEvent>> {
        self.crankshaft.as_ref()
    }

    /// Gets the sender for the transfer events.
    pub(crate) fn transfer(&self) -> Option<&broadcast::Sender<TransferEvent>> {
        self.transfer.as_ref()
    }
}

/// Represents the location of a call in an evaluation error.
#[derive(Debug, Clone)]
pub struct CallLocation {
    /// The document containing the call statement.
    pub document: Document,
    /// The span of the call statement.
    pub span: Span,
}

/// Represents an error that originates from WDL source.
#[derive(Debug)]
pub struct SourceError {
    /// The document originating the diagnostic.
    pub document: Document,
    /// The evaluation diagnostic.
    pub diagnostic: Diagnostic,
    /// The call backtrace for the error.
    ///
    /// An empty backtrace denotes that the error was encountered outside of
    /// a call.
    ///
    /// The call locations are stored as most recent to least recent.
    pub backtrace: Vec<CallLocation>,
}

/// Represents an error that may occur when evaluating a workflow or task.
#[derive(Debug)]
pub enum EvaluationError {
    /// Evaluation was canceled.
    Canceled,
    /// The error came from WDL source evaluation.
    Source(Box<SourceError>),
    /// The error came from another source.
    Other(anyhow::Error),
}

impl EvaluationError {
    /// Creates a new evaluation error from the given document and diagnostic.
    pub fn new(document: Document, diagnostic: Diagnostic) -> Self {
        Self::Source(Box::new(SourceError {
            document,
            diagnostic,
            backtrace: Default::default(),
        }))
    }

    /// Helper for tests for converting an evaluation error to a string.
    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        use std::collections::HashMap;

        use codespan_reporting::diagnostic::Label;
        use codespan_reporting::diagnostic::LabelStyle;
        use codespan_reporting::files::SimpleFiles;
        use codespan_reporting::term;
        use codespan_reporting::term::Config;
        use codespan_reporting::term::termcolor::Buffer;
        use wdl_ast::AstNode;

        match self {
            Self::Canceled => "evaluation was canceled".to_string(),
            Self::Source(e) => {
                let mut files = SimpleFiles::new();
                let mut map = HashMap::new();

                let file_id = files.add(e.document.path(), e.document.root().text().to_string());

                let diagnostic =
                    e.diagnostic
                        .to_codespan(file_id)
                        .with_labels_iter(e.backtrace.iter().map(|l| {
                            let id = l.document.id();
                            let file_id = *map.entry(id).or_insert_with(|| {
                                files.add(l.document.path(), l.document.root().text().to_string())
                            });

                            Label {
                                style: LabelStyle::Secondary,
                                file_id,
                                range: l.span.start()..l.span.end(),
                                message: "called from this location".into(),
                            }
                        }));

                let mut buffer = Buffer::no_color();
                term::emit_to_write_style(&mut buffer, &Config::default(), &files, &diagnostic)
                    .expect("failed to emit diagnostic");

                String::from_utf8(buffer.into_inner()).expect("should be UTF-8")
            }
            Self::Other(e) => format!("{e:#}"),
        }
    }
}

impl From<anyhow::Error> for EvaluationError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e)
    }
}

/// Represents a result from evaluating a workflow or task.
pub type EvaluationResult<T> = Result<T, EvaluationError>;

/// Represents context to an expression evaluator.
pub(crate) trait EvaluationContext: Send + Sync {
    /// Gets the supported version of the document being evaluated.
    fn version(&self) -> SupportedVersion;

    /// Gets the value of the given name in scope.
    fn resolve_name(&self, name: &str, span: Span) -> Result<Value, Diagnostic>;

    /// Resolves a type name to a type.
    fn resolve_type_name(&self, name: &str, span: Span) -> Result<Type, Diagnostic>;

    /// Returns the literal value of an enum choice.
    fn enum_choice_value(&self, enum_name: &str, choice_name: &str) -> Result<Value, Diagnostic>;

    /// Gets the base directory for the evaluation.
    ///
    /// The base directory is what paths are relative to.
    ///
    /// For workflow evaluation, the base directory is the document's directory.
    ///
    /// For task evaluation, the base directory is the document's directory or
    /// the task's working directory if the `output` section is being evaluated.
    fn base_dir(&self) -> &EvaluationPath;

    /// Gets the temp directory for the evaluation.
    fn temp_dir(&self) -> &Path;

    /// Gets the value to return for a call to the `stdout` function.
    ///
    /// This returns `Some` only when evaluating a task's outputs section.
    fn stdout(&self) -> Option<&Value> {
        None
    }

    /// Gets the value to return for a call to the `stderr` function.
    ///
    /// This returns `Some` only when evaluating a task's outputs section.
    fn stderr(&self) -> Option<&Value> {
        None
    }

    /// Gets the task associated with the evaluation context.
    ///
    /// This returns `Some` only when evaluating a task's hints sections.
    fn task(&self) -> Option<&Task> {
        None
    }

    /// Gets the client and cancellation token for HTTP operations.
    fn http(&self) -> (&EvaluationHttpClient, &CancellationToken);

    /// Gets a guest path representation of a host path.
    ///
    /// Returns `None` if there is no guest path representation of the host
    /// path.
    fn guest_path(&self, path: &HostPath) -> Option<GuestPath> {
        let _ = path;
        None
    }

    /// Gets a host path representation of a guest path.
    ///
    /// Returns `None` if there is no host path representation of the guest
    /// path.
    fn host_path(&self, path: &GuestPath) -> Option<HostPath> {
        let _ = path;
        None
    }

    /// Notifies the context that a temporary file was created as a result of a
    /// call to a stdlib function.
    ///
    /// A context may map a guest path for the new host path.
    fn notify_temp_file_created(&mut self, path: &HostPath) -> Result<()> {
        let _ = path;
        Ok(())
    }

    /// Callback that is invoked prior to accessing an object.
    ///
    /// If the callback returns `Some`, the returned value will be treated as
    /// the result of the access.
    ///
    /// If the callback returns `None`, the object will be accessed normally.
    fn object_access(&self, object: &Object, name: &str) -> Option<Value> {
        let _ = (object, name);
        None
    }

    /// Compiles a regular expression.
    fn compile_regex(&self, pattern: &str) -> Result<Regex, regex::Error>;
}

/// Represents an index of a scope in a collection of scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ScopeIndex(usize);

impl ScopeIndex {
    /// Constructs a new scope index from a raw index.
    pub const fn new(index: usize) -> Self {
        Self(index)
    }
}

impl From<usize> for ScopeIndex {
    fn from(index: usize) -> Self {
        Self(index)
    }
}

impl From<ScopeIndex> for usize {
    fn from(index: ScopeIndex) -> Self {
        index.0
    }
}

/// Represents an evaluation scope in a WDL document.
#[derive(Default, Debug)]
struct Scope {
    /// The index of the parent scope.
    ///
    /// This is `None` for the root scopes.
    parent: Option<ScopeIndex>,
    /// The map of names in scope to their values.
    names: IndexMap<String, Value>,
}

impl Scope {
    /// Creates a new scope given the parent scope.
    pub fn new(parent: ScopeIndex) -> Self {
        Self {
            parent: Some(parent),
            names: Default::default(),
        }
    }

    /// Inserts a name into the scope.
    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        let prev = self.names.insert(name.into(), value.into());
        assert!(prev.is_none(), "conflicting name in scope");
    }

    /// Iterates over the local names and values in the scope.
    pub fn local(&self) -> impl Iterator<Item = (&str, &Value)> + use<'_> {
        self.names.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Gets a mutable reference to an existing name in scope.
    pub(crate) fn get_mut(&mut self, name: &str) -> Option<&mut Value> {
        self.names.get_mut(name)
    }

    /// Clears the scope.
    pub(crate) fn clear(&mut self) {
        self.parent = None;
        self.names.clear();
    }

    /// Sets the scope's parent.
    pub(crate) fn set_parent(&mut self, parent: ScopeIndex) {
        self.parent = Some(parent);
    }
}

impl From<Scope> for IndexMap<String, Value> {
    fn from(scope: Scope) -> Self {
        scope.names
    }
}

impl From<Scope> for Outputs {
    fn from(scope: Scope) -> Self {
        scope.names.into()
    }
}

/// Represents a reference to a scope.
#[derive(Debug, Clone, Copy)]
struct ScopeRef<'a> {
    /// The reference to the scopes collection.
    scopes: &'a [Scope],
    /// The index of the scope in the collection.
    index: ScopeIndex,
}

impl<'a> ScopeRef<'a> {
    /// Creates a new scope reference given the scope index.
    pub fn new(scopes: &'a [Scope], index: impl Into<ScopeIndex>) -> Self {
        Self {
            scopes,
            index: index.into(),
        }
    }

    /// Gets the parent scope.
    ///
    /// Returns `None` if there is no parent scope.
    pub fn parent(&self) -> Option<Self> {
        self.scopes[self.index.0].parent.map(|p| Self {
            scopes: self.scopes,
            index: p,
        })
    }

    /// Gets the value of a name local to this scope.
    ///
    /// Returns `None` if a name local to this scope was not found.
    pub fn local(&self, name: &str) -> Option<&Value> {
        self.scopes[self.index.0].names.get(name)
    }

    /// Lookups a name in the scope.
    ///
    /// Returns `None` if the name is not available in the scope.
    pub fn lookup(&self, name: &str) -> Option<&Value> {
        let mut current = Some(self.index);

        while let Some(index) = current {
            if let Some(name) = self.scopes[index.0].names.get(name) {
                return Some(name);
            }

            current = self.scopes[index.0].parent;
        }

        None
    }
}

/// Represents an evaluated task.
///
/// An evaluated task is one that was executed by a task execution backend.
///
/// The evaluated task may have failed as a result of an unacceptable exit code.
///
/// Use [`EvaluatedTask::into_outputs`] to get the outputs of the task.
#[derive(Debug)]
pub struct EvaluatedTask {
    /// The underlying task execution result.
    result: TaskExecutionResult,
    /// The evaluated outputs of the task.
    outputs: Outputs,
    /// Stores the execution error for the evaluated task.
    ///
    /// This is `None` when the evaluated task successfully executed.
    error: Option<EvaluationError>,
    /// Whether or not the execution result was from the call cache.
    cached: bool,
}

impl EvaluatedTask {
    /// Constructs a new evaluated task.
    fn new(cached: bool, result: TaskExecutionResult, error: Option<EvaluationError>) -> Self {
        Self {
            result,
            outputs: Default::default(),
            error,
            cached,
        }
    }

    /// Gets whether or not the evaluated task failed as a result of an
    /// unacceptable exit code.
    pub fn failed(&self) -> bool {
        self.error.is_some()
    }

    /// Determines whether or not the task execution result was used from the
    /// call cache.
    pub fn cached(&self) -> bool {
        self.cached
    }

    /// Gets the exit code of the evaluated task.
    pub fn exit_code(&self) -> i32 {
        self.result.exit_code
    }

    /// Gets the working directory of the evaluated task.
    pub fn work_dir(&self) -> &EvaluationPath {
        &self.result.work_dir
    }

    /// Gets the stdout value of the evaluated task.
    pub fn stdout(&self) -> &Value {
        &self.result.stdout
    }

    /// Gets the stderr value of the evaluated task.
    pub fn stderr(&self) -> &Value {
        &self.result.stderr
    }

    /// Converts the evaluated task into its [`Outputs`].
    ///
    /// An error is returned if the task failed as a result of an unacceptable
    /// exit code.
    pub fn into_outputs(self) -> EvaluationResult<Outputs> {
        match self.error {
            Some(e) => Err(e),
            None => Ok(self.outputs),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_slow() {
        let context = CancellationContext::new(FailureMode::Slow);
        assert_eq!(context.state(), CancellationContextState::NotCanceled);

        // The first cancel should not cancel the fast token
        assert_eq!(context.cancel(), CancellationContextState::Waiting);
        assert_eq!(context.state(), CancellationContextState::Waiting);
        assert!(context.user_canceled());
        assert!(context.first.is_cancelled());
        assert!(!context.second.is_cancelled());

        // The second cancel should cancel both tokens
        assert_eq!(context.cancel(), CancellationContextState::Canceling);
        assert_eq!(context.state(), CancellationContextState::Canceling);
        assert!(context.user_canceled());
        assert!(context.first.is_cancelled());
        assert!(context.second.is_cancelled());

        // Subsequent cancellations have no effect
        assert_eq!(context.cancel(), CancellationContextState::Canceling);
        assert_eq!(context.state(), CancellationContextState::Canceling);
        assert!(context.user_canceled());
        assert!(context.first.is_cancelled());
        assert!(context.second.is_cancelled());
    }

    #[test]
    fn cancellation_fast() {
        let context = CancellationContext::new(FailureMode::Fast);
        assert_eq!(context.state(), CancellationContextState::NotCanceled);

        // Fail fast should immediately cancel both tokens
        assert_eq!(context.cancel(), CancellationContextState::Canceling);
        assert_eq!(context.state(), CancellationContextState::Canceling);
        assert!(context.user_canceled());
        assert!(context.first.is_cancelled());
        assert!(context.second.is_cancelled());

        // Subsequent cancellations have no effect
        assert_eq!(context.cancel(), CancellationContextState::Canceling);
        assert_eq!(context.state(), CancellationContextState::Canceling);
        assert!(context.user_canceled());
        assert!(context.first.is_cancelled());
        assert!(context.second.is_cancelled());
    }

    #[test]
    fn cancellation_error_slow() {
        let context = CancellationContext::new(FailureMode::Slow);
        assert_eq!(context.state(), CancellationContextState::NotCanceled);

        // An error should not cancel the fast token
        context.error(&EvaluationError::Canceled);
        assert_eq!(context.state(), CancellationContextState::Waiting);
        assert!(!context.user_canceled());
        assert!(context.first.is_cancelled());
        assert!(!context.second.is_cancelled());

        // A repeated error should not cancel the fast token either
        context.error(&EvaluationError::Canceled);
        assert_eq!(context.state(), CancellationContextState::Waiting);
        assert!(!context.user_canceled());
        assert!(context.first.is_cancelled());
        assert!(!context.second.is_cancelled());

        // However, another cancellation will cancel both tokens
        assert_eq!(context.cancel(), CancellationContextState::Canceling);
        assert_eq!(context.state(), CancellationContextState::Canceling);
        assert!(!context.user_canceled());
        assert!(context.first.is_cancelled());
        assert!(context.second.is_cancelled());
    }

    #[test]
    fn cancellation_error_fast() {
        let context = CancellationContext::new(FailureMode::Fast);
        assert_eq!(context.state(), CancellationContextState::NotCanceled);

        // An error should cancel both tokens
        context.error(&EvaluationError::Canceled);
        assert_eq!(context.state(), CancellationContextState::Canceling);
        assert!(!context.user_canceled());
        assert!(context.first.is_cancelled());
        assert!(context.second.is_cancelled());

        // A repeated error should not change anything
        context.error(&EvaluationError::Canceled);
        assert_eq!(context.state(), CancellationContextState::Canceling);
        assert!(!context.user_canceled());
        assert!(context.first.is_cancelled());
        assert!(context.second.is_cancelled());

        // Neither should another `cancel` call
        assert_eq!(context.cancel(), CancellationContextState::Canceling);
        assert_eq!(context.state(), CancellationContextState::Canceling);
        assert!(!context.user_canceled());
        assert!(context.first.is_cancelled());
        assert!(context.second.is_cancelled());
    }

    #[test]
    fn cancellation_child() {
        let context = CancellationContext::new(FailureMode::Fast);
        assert_eq!(context.state(), CancellationContextState::NotCanceled);

        // Children can have a different failure mode
        let child = context.child(FailureMode::Slow);
        assert_eq!(child.state(), CancellationContextState::NotCanceled);
        assert_eq!(child.cancel(), CancellationContextState::Waiting);
        assert_eq!(child.cancel(), CancellationContextState::Canceling);
        assert!(child.user_canceled());
        assert!(child.first.is_cancelled());
        assert!(child.second.is_cancelled());

        // Child cancellation doesn't affect the parent
        assert_eq!(context.state(), CancellationContextState::NotCanceled);

        // But parent cancellation affects the child
        let child = context.child(FailureMode::Fast);
        assert_eq!(context.cancel(), CancellationContextState::Canceling);
        assert!(child.first.is_cancelled());
        assert!(child.second.is_cancelled());
    }

    #[test]
    fn child_inherits_parent_cancellation_state() {
        let parent = CancellationContext::new(FailureMode::Fast);
        let child = parent.child(FailureMode::Fast);
        assert_eq!(child.state(), CancellationContextState::NotCanceled);

        // The user cancels the parent (`Fast` mode cancels in one step).
        assert_eq!(parent.cancel(), CancellationContextState::Canceling);

        // The child's effective state now reflects the parent's cancellation.
        assert_eq!(child.state(), CancellationContextState::Canceling);
        assert!(child.user_canceled());

        // Tokens still propagate from parent to child as before.
        assert!(child.first.is_cancelled());
        assert!(child.second.is_cancelled());
    }

    #[test]
    fn child_keeps_local_error_cause_on_tie_with_parent() {
        let parent = CancellationContext::new(FailureMode::Fast);
        let child = parent.child(FailureMode::Fast);

        // The user cancels the parent.
        assert_eq!(parent.cancel(), CancellationContextState::Canceling);

        // The child then records a local error, as happens when its aborted
        // task surfaces `EvaluationError::Canceled`.
        child.error(&EvaluationError::Canceled);

        // Parent and child both sit at `Canceling`, so the levels tie and the
        // child's own error cause is authoritative: it does not report a user
        // cancellation despite the parent's cancellation being user initiated.
        assert_eq!(child.state(), CancellationContextState::Canceling);
        assert!(!child.user_canceled());
    }

    #[test]
    fn later_parent_cancel_does_not_relabel_child_error() {
        let parent = CancellationContext::new(FailureMode::Fast);
        let child = parent.child(FailureMode::Fast);

        // The child errors first and enters `Canceling` on its own.
        child.error(&EvaluationError::Canceled);
        assert_eq!(child.state(), CancellationContextState::Canceling);
        assert!(!child.user_canceled());

        // A later user cancellation of the parent ties at `Canceling`, so it
        // does not trample the child's already-established error cause.
        assert_eq!(parent.cancel(), CancellationContextState::Canceling);
        assert_eq!(child.state(), CancellationContextState::Canceling);
        assert!(!child.user_canceled());
    }

    #[test]
    fn higher_parent_level_escalates_child_and_carries_its_cause() {
        let parent = CancellationContext::new(FailureMode::Fast);
        let child = parent.child(FailureMode::Slow);

        // The child errors in `Slow` mode, reaching only `Waiting`.
        child.error(&EvaluationError::Canceled);
        assert_eq!(child.state(), CancellationContextState::Waiting);
        assert!(!child.user_canceled());

        // The user cancels the parent in `Fast` mode, reaching `Canceling`.
        // The parent's level is strictly greater, so its whole pair wins and
        // the child now reports the parent's user cancellation.
        assert_eq!(parent.cancel(), CancellationContextState::Canceling);
        assert_eq!(child.state(), CancellationContextState::Canceling);
        assert!(child.user_canceled());
    }

    #[test]
    fn closest_descendant_wins_tie_across_multiple_levels() {
        let root = CancellationContext::new(FailureMode::Fast);
        let child = root.child(FailureMode::Fast);
        let grandchild = child.child(FailureMode::Fast);

        // The user cancels the root, which folds down to the grandchild.
        assert_eq!(root.cancel(), CancellationContextState::Canceling);

        // The grandchild then errors locally, tying at `Canceling`. The
        // closest cause, the grandchild's own error, wins over the ancestor.
        grandchild.error(&EvaluationError::Canceled);
        assert_eq!(grandchild.state(), CancellationContextState::Canceling);
        assert!(!grandchild.user_canceled());
    }

    #[test]
    fn grandchild_inherits_ancestor_cancellation() {
        let root = CancellationContext::new(FailureMode::Fast);
        let child = root.child(FailureMode::Fast);
        let grandchild = child.child(FailureMode::Fast);
        assert_eq!(grandchild.state(), CancellationContextState::NotCanceled);

        assert_eq!(root.cancel(), CancellationContextState::Canceling);

        // Cancellation folds across more than one level.
        assert_eq!(grandchild.state(), CancellationContextState::Canceling);
        assert!(grandchild.user_canceled());
    }

    #[test]
    fn child_cancellation_does_not_affect_parent_or_siblings() {
        let parent = CancellationContext::new(FailureMode::Fast);
        let a = parent.child(FailureMode::Fast);
        let b = parent.child(FailureMode::Fast);

        assert_eq!(a.cancel(), CancellationContextState::Canceling);

        // Cancelling one child leaves the parent and the sibling untouched.
        assert_eq!(parent.state(), CancellationContextState::NotCanceled);
        assert!(!parent.user_canceled());
        assert_eq!(b.state(), CancellationContextState::NotCanceled);
        assert!(!b.user_canceled());
    }

    #[test]
    fn resubmit_names_derive_from_the_original_attempt_name() {
        assert_eq!(resubmit_task_name("wf-task-abc123", 1), "wf-task-abc123~1");
        assert_eq!(resubmit_task_name("wf-task-abc123", 2), "wf-task-abc123~2");

        // Deriving from an already-derived name must not stack separators.
        assert_eq!(
            resubmit_task_name("wf-task-abc123~1", 2),
            "wf-task-abc123~2"
        );
    }

    #[test]
    fn retry_causes_serialize_with_a_kind_tag() {
        let cause = RetryCause::UnacceptableExitCode { code: 137 };
        let json = serde_json::to_value(cause).expect("cause should serialize");
        assert_eq!(
            json,
            serde_json::json!({ "kind": "unacceptable_exit_code", "code": 137 })
        );
        assert_eq!(cause.to_string(), "unacceptable exit code 137");

        let cause = RetryCause::Preempted;
        let json = serde_json::to_value(cause).expect("cause should serialize");
        assert_eq!(json, serde_json::json!({ "kind": "preempted" }));
        assert_eq!(cause.to_string(), "preempted");

        let parsed: RetryCause = serde_json::from_value(
            serde_json::json!({ "kind": "unacceptable_exit_code", "code": 1 }),
        )
        .expect("cause should deserialize");
        assert_eq!(parsed, RetryCause::UnacceptableExitCode { code: 1 });
    }

    #[test]
    fn constraints_snapshot_captures_the_resolved_resources() {
        let constraints = TaskExecutionConstraints {
            sources: vec![ImageSource::Library("example.org/tools/tool:1".to_string())],
            cpu: 4.0,
            memory: 8 * 1024 * 1024 * 1024,
            gpu: vec!["nvidia-tesla-t4".to_string()],
            fpga: Vec::new(),
            disks: [("/".to_string(), 10i64)].into_iter().collect(),
        };

        let snapshot = TaskConstraintsSnapshot::from(&constraints);
        assert_eq!(
            snapshot.container.as_deref(),
            Some("library://example.org/tools/tool:1")
        );
        // The curated hints and retry policy are populated at emission, not
        // from the constraints.
        assert_eq!(snapshot.max_retries, None);
        assert_eq!(snapshot.preemptible, None);
        assert_eq!(snapshot.max_cpu, None);
        assert_eq!(snapshot.max_memory, None);
        assert_eq!(snapshot.cpu, 4.0);
        assert_eq!(snapshot.memory, 8 * 1024 * 1024 * 1024);
        assert_eq!(snapshot.gpu, ["nvidia-tesla-t4"]);
        assert!(snapshot.fpga.is_empty());
        assert_eq!(snapshot.disks.get("/"), Some(&10));

        // The snapshot round-trips through JSON.
        let json = serde_json::to_string(&snapshot).expect("snapshot should serialize");
        let parsed: TaskConstraintsSnapshot =
            serde_json::from_str(&json).expect("snapshot should deserialize");
        assert_eq!(parsed, snapshot);
    }
}
