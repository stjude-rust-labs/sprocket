//! Module for evaluation.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::BufRead;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use anyhow::bail;
use cloud_copy::TransferEvent;
use crankshaft::events::Event as CrankshaftEvent;
use indexmap::IndexMap;
use itertools::Itertools;
use rev_buf_reader::RevBufReader;
use tokio::sync::broadcast;
use wdl_analysis::Document;
use wdl_analysis::document::Task;
use wdl_analysis::types::Type;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::v1::TASK_REQUIREMENT_RETURN_CODES;
use wdl_ast::v1::TASK_REQUIREMENT_RETURN_CODES_ALIAS;

use crate::CompoundValue;
use crate::Outputs;
use crate::PrimitiveValue;
use crate::TaskExecutionResult;
use crate::Value;
use crate::http::Location;
use crate::http::Transferer;
use crate::path::EvaluationPath;
use crate::stdlib::download_file;

pub mod trie;
pub mod v1;

/// The maximum number of stderr lines to display in error messages.
const MAX_STDERR_LINES: usize = 10;

/// A name used whenever a file system "root" is mapped.
///
/// A root might be a root directory like `/` or `C:\`, but it also might be the root of a URL like `https://example.com`.
const ROOT_NAME: &str = ".root";

/// Represents events that may be sent during evaluation.
#[derive(Debug, Clone, Default)]
pub struct Events {
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
    pub fn all(capacity: usize) -> Self {
        Self {
            crankshaft: Some(broadcast::Sender::new(capacity)),
            transfer: Some(broadcast::Sender::new(capacity)),
        }
    }

    /// Constructs a new `Events` and disable subscribing to any event channel.
    pub fn none() -> Self {
        Self::default()
    }

    /// Constructs a new `Events` and enable subscribing to only the Crankshaft
    /// events channel.
    pub fn crankshaft_only(capacity: usize) -> Self {
        Self {
            crankshaft: Some(broadcast::Sender::new(capacity)),
            transfer: None,
        }
    }

    /// Constructs a new `Events` and enable subscribing to only the transfer
    /// events channel.
    pub fn transfer_only(capacity: usize) -> Self {
        Self {
            crankshaft: None,
            transfer: Some(broadcast::Sender::new(capacity)),
        }
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
    #[cfg(feature = "codespan-reporting")]
    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        use codespan_reporting::diagnostic::Label;
        use codespan_reporting::diagnostic::LabelStyle;
        use codespan_reporting::files::SimpleFiles;
        use codespan_reporting::term::Config;
        use codespan_reporting::term::termcolor::Buffer;
        use codespan_reporting::term::{self};
        use wdl_ast::AstNode;

        match self {
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
                term::emit(&mut buffer, &Config::default(), &files, &diagnostic)
                    .expect("failed to emit diagnostic");

                String::from_utf8(buffer.into_inner()).expect("should be UTF-8")
            }
            Self::Other(e) => format!("{e:?}"),
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

/// Represents a path to a file or directory on the host file system or a URL to
/// a remote file.
///
/// The host in this context is where the WDL evaluation is taking place.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HostPath(pub(crate) Arc<String>);

impl HostPath {
    /// Constructs a new host path from a string.
    pub fn new(path: impl Into<String>) -> Self {
        Self(Arc::new(path.into()))
    }

    /// Gets the string representation of the host path.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HostPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Arc<String>> for HostPath {
    fn from(path: Arc<String>) -> Self {
        Self(path)
    }
}

impl From<HostPath> for Arc<String> {
    fn from(path: HostPath) -> Self {
        path.0
    }
}

/// Represents a path to a file or directory on the guest.
///
/// The guest in this context is the container where tasks are run.
///
/// For backends that do not use containers, a guest path is the same as a host
/// path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GuestPath(pub(crate) Arc<String>);

impl GuestPath {
    /// Constructs a new guest path from a string.
    pub fn new(path: impl Into<String>) -> Self {
        Self(Arc::new(path.into()))
    }

    /// Gets the string representation of the guest path.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GuestPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Arc<String>> for GuestPath {
    fn from(path: Arc<String>) -> Self {
        Self(path)
    }
}

impl From<GuestPath> for Arc<String> {
    fn from(path: GuestPath) -> Self {
        path.0
    }
}

/// Represents context to an expression evaluator.
pub trait EvaluationContext: Send + Sync {
    /// Gets the supported version of the document being evaluated.
    fn version(&self) -> SupportedVersion;

    /// Gets the value of the given name in scope.
    fn resolve_name(&self, name: &str, span: Span) -> Result<Value, Diagnostic>;

    /// Resolves a type name to a type.
    fn resolve_type_name(&self, name: &str, span: Span) -> Result<Type, Diagnostic>;

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
pub struct ScopeIndex(usize);

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
pub struct Scope {
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
        let name = name.into();
        let prev = self.names.insert(name.clone(), value.into());
        assert!(prev.is_none(), "conflicting name in scope: `{name}`");
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

/// Represents a reference to a scope.
#[derive(Debug, Clone, Copy)]
pub struct ScopeRef<'a> {
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

    /// Gets all of the name and values available at this scope.
    pub fn names(&self) -> impl Iterator<Item = (&str, &Value)> + use<'_> {
        self.scopes[self.index.0]
            .names
            .iter()
            .map(|(n, name)| (n.as_str(), name))
    }

    /// Iterates over each name and value visible to the scope and calls the
    /// provided callback.
    ///
    /// Stops iterating and returns an error if the callback returns an error.
    pub fn for_each(&self, mut cb: impl FnMut(&str, &Value) -> Result<()>) -> Result<()> {
        let mut current = Some(self.index);

        while let Some(index) = current {
            for (n, v) in self.scopes[index.0].local() {
                cb(n, v)?;
            }

            current = self.scopes[index.0].parent;
        }

        Ok(())
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
#[derive(Debug)]
pub struct EvaluatedTask {
    /// The task attempt directory.
    attempt_dir: PathBuf,
    /// The task execution result.
    result: TaskExecutionResult,
    /// The evaluated outputs of the task.
    ///
    /// This is `Ok` when the task executes successfully and all of the task's
    /// outputs evaluated without error.
    ///
    /// Otherwise, this contains the error that occurred while attempting to
    /// evaluate the task's outputs.
    outputs: EvaluationResult<Outputs>,
}

impl EvaluatedTask {
    /// Constructs a new evaluated task.
    ///
    /// Returns an error if the stdout or stderr paths are not UTF-8.
    fn new(attempt_dir: PathBuf, result: TaskExecutionResult) -> anyhow::Result<Self> {
        Ok(Self {
            result,
            attempt_dir,
            outputs: Ok(Default::default()),
        })
    }

    /// Gets the exit code of the evaluated task.
    pub fn exit_code(&self) -> i32 {
        self.result.exit_code
    }

    /// Gets the attempt directory of the task.
    pub fn attempt_dir(&self) -> &Path {
        &self.attempt_dir
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

    /// Gets the outputs of the evaluated task.
    ///
    /// This is `Ok` when the task executes successfully and all of the task's
    /// outputs evaluated without error.
    ///
    /// Otherwise, this contains the error that occurred while attempting to
    /// evaluate the task's outputs.
    pub fn outputs(&self) -> &EvaluationResult<Outputs> {
        &self.outputs
    }

    /// Converts the evaluated task into an evaluation result.
    ///
    /// Returns `Ok(_)` if the task outputs were evaluated.
    ///
    /// Returns `Err(_)` if the task outputs could not be evaluated.
    pub fn into_result(self) -> EvaluationResult<Outputs> {
        self.outputs
    }

    /// Handles the exit of a task execution.
    ///
    /// Returns an error if the task failed.
    async fn handle_exit(
        &self,
        requirements: &HashMap<String, Value>,
        transferer: &dyn Transferer,
    ) -> anyhow::Result<()> {
        let mut error = true;
        if let Some(return_codes) = requirements
            .get(TASK_REQUIREMENT_RETURN_CODES)
            .or_else(|| requirements.get(TASK_REQUIREMENT_RETURN_CODES_ALIAS))
        {
            match return_codes {
                Value::Primitive(PrimitiveValue::String(s)) if s.as_ref() == "*" => {
                    error = false;
                }
                Value::Primitive(PrimitiveValue::String(s)) => {
                    bail!(
                        "invalid return code value `{s}`: only `*` is accepted when the return \
                         code is specified as a string"
                    );
                }
                Value::Primitive(PrimitiveValue::Integer(ok)) => {
                    if self.result.exit_code == i32::try_from(*ok).unwrap_or_default() {
                        error = false;
                    }
                }
                Value::Compound(CompoundValue::Array(codes)) => {
                    error = !codes.as_slice().iter().any(|v| {
                        v.as_integer()
                            .map(|i| i32::try_from(i).unwrap_or_default() == self.result.exit_code)
                            .unwrap_or(false)
                    });
                }
                _ => unreachable!("unexpected return codes value"),
            }
        } else {
            error = self.result.exit_code != 0;
        }

        if error {
            // Read the last `MAX_STDERR_LINES` number of lines from stderr
            // If there's a problem reading stderr, don't output it
            let stderr = download_file(
                transferer,
                self.work_dir(),
                self.stderr().as_file().unwrap(),
            )
            .await
            .ok()
            .and_then(|l| {
                fs::File::open(l).ok().map(|f| {
                    // Buffer the last N number of lines
                    let reader = RevBufReader::new(f);
                    let lines: Vec<_> = reader
                        .lines()
                        .take(MAX_STDERR_LINES)
                        .map_while(|l| l.ok())
                        .collect();

                    // Iterate the lines in reverse order as we read them in reverse
                    lines
                        .iter()
                        .rev()
                        .format_with("\n", |l, f| f(&format_args!("  {l}")))
                        .to_string()
                })
            })
            .unwrap_or_default();

            // If the work directory is remote,
            bail!(
                "process terminated with exit code {code}: see `{stdout_path}` and \
                 `{stderr_path}` for task output and the related files in \
                 `{dir}`{header}{stderr}{trailer}",
                code = self.result.exit_code,
                dir = self.attempt_dir().display(),
                stdout_path = self.stdout().as_file().expect("must be file"),
                stderr_path = self.stderr().as_file().expect("must be file"),
                header = if stderr.is_empty() {
                    Cow::Borrowed("")
                } else {
                    format!("\n\ntask stderr output (last {MAX_STDERR_LINES} lines):\n\n").into()
                },
                trailer = if stderr.is_empty() { "" } else { "\n" }
            );
        }

        Ok(())
    }
}

/// Gets the kind of an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputKind {
    /// The input is a single file.
    File,
    /// The input is a directory.
    Directory,
}

impl From<InputKind> for crankshaft::engine::task::input::Type {
    fn from(value: InputKind) -> Self {
        match value {
            InputKind::File => Self::File,
            InputKind::Directory => Self::Directory,
        }
    }
}

/// Represents a `File` or `Directory` input to a task.
#[derive(Debug, Clone)]
pub struct Input {
    /// The input kind.
    kind: InputKind,
    /// The path for the input.
    path: EvaluationPath,
    /// The guest path for the input.
    ///
    /// This is `None` when the backend isn't mapping input paths.
    guest_path: Option<GuestPath>,
    /// The download location for the input.
    ///
    /// This is `Some` if the input has been downloaded to a known location.
    location: Option<Location>,
}

impl Input {
    /// Creates a new input with the given path and access.
    fn new(kind: InputKind, path: EvaluationPath, guest_path: Option<GuestPath>) -> Self {
        Self {
            kind,
            path,
            guest_path,
            location: None,
        }
    }

    /// Gets the kind of the input.
    pub fn kind(&self) -> InputKind {
        self.kind
    }

    /// Gets the path to the input.
    ///
    /// The path of the input may be local or remote.
    pub fn path(&self) -> &EvaluationPath {
        &self.path
    }

    /// Gets the guest path for the input.
    ///
    /// This is `None` for inputs to backends that don't use containers.
    pub fn guest_path(&self) -> Option<&GuestPath> {
        self.guest_path.as_ref()
    }

    /// Gets the local path of the input.
    ///
    /// Returns `None` if the input is remote and has not been localized.
    pub fn local_path(&self) -> Option<&Path> {
        self.location.as_deref().or_else(|| self.path.as_local())
    }

    /// Sets the location of the input.
    ///
    /// This is used during localization to set a local path for remote inputs.
    pub fn set_location(&mut self, location: Location) {
        self.location = Some(location);
    }
}
