//! Module for evaluation.

use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;

use anyhow::Result;
use cloud_copy::TransferEvent;
use crankshaft::events::Event as CrankshaftEvent;
use indexmap::IndexMap;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::error;
use wdl_analysis::Document;
use wdl_analysis::document::Task;
use wdl_analysis::types::Type;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;

use crate::EvaluationPath;
use crate::GuestPath;
use crate::HostPath;
use crate::Outputs;
use crate::Value;
use crate::backend::TaskExecutionResult;
use crate::config::FailureMode;
use crate::http::Transferer;

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
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |state| {
                // If updating for an error and there has been a cancellation, bail out
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
#[derive(Debug, Clone)]
pub struct CancellationContext {
    /// The failure mode for the cancellation context.
    mode: FailureMode,
    /// The state of the cancellation context.
    state: Arc<AtomicU8>,
    /// The parent context, consulted read-only when folding the effective
    /// state. `None` for a root context created by [`new`](Self::new).
    parent: Option<Arc<CancellationContext>>,
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
    pub fn first(&self) -> CancellationToken {
        self.first.clone()
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
    pub fn second(&self) -> CancellationToken {
        self.second.clone()
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

/// Represents an event from the WDL evaluation engine.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// A cached task execution result was reused due to a call cache hit.
    ReusedCachedExecutionResult {
        /// The id of the task that reused a cached execution result.
        id: String,
        /// The WDL task name (un-mangled, e.g. `align`).
        name: String,
    },
    /// A locally running task has been parked by the engine due to insufficient
    /// resources.
    TaskParked,
    /// A locally running task has been unparked by the engine.
    TaskUnparked {
        /// Whether or not the task was unparked due to being canceled.
        canceled: bool,
    },
    /// A WDL task execution attempt has started (one per attempt).
    ///
    /// Carries structured identity directly from the engine, so consumers need
    /// not parse the backend's mangled task id.
    WdlTaskStarted {
        /// The full task id for this attempt (e.g. `align-0`).
        id: String,
        /// The WDL task name (un-mangled, e.g. `align`).
        name: String,
    },
    /// A WDL task execution attempt has finished (one per attempt).
    WdlTaskCompleted {
        /// The full task id for this attempt (e.g. `align-0`).
        id: String,
        /// The WDL task name (un-mangled, e.g. `align`).
        name: String,
        /// The process exit code, if the attempt produced one.
        exit_code: Option<i32>,
        /// Whether the attempt was canceled rather than run to completion.
        canceled: bool,
    },
}

/// Represents events that may be sent during WDL evaluation.
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
    pub(crate) fn engine(&self) -> &Option<broadcast::Sender<EngineEvent>> {
        &self.engine
    }

    /// Gets the sender for the Crankshaft events.
    pub(crate) fn crankshaft(&self) -> &Option<broadcast::Sender<CrankshaftEvent>> {
        &self.crankshaft
    }

    /// Gets the sender for the transfer events.
    pub(crate) fn transfer(&self) -> &Option<broadcast::Sender<TransferEvent>> {
        &self.transfer
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

    /// Returns the literal value of an enum variant.
    fn enum_variant_value(&self, enum_name: &str, variant_name: &str) -> Result<Value, Diagnostic>;

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

    /// Gets the transferer to use for evaluating expressions.
    fn transferer(&self) -> &dyn Transferer;

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

    /// Notifies the context that a file was created as a result of a call to a
    /// stdlib function.
    ///
    /// A context may map a guest path for the new host path.
    fn notify_file_created(&mut self, path: &HostPath) -> Result<()> {
        let _ = path;
        Ok(())
    }
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
mod test {
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
}
