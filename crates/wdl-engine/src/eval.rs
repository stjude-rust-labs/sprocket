//! Module for evaluation.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::io::BufRead;
use std::path::Component;
use std::path::MAIN_SEPARATOR;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use indexmap::IndexMap;
use itertools::Itertools;
use rev_buf_reader::RevBufReader;
use wdl_analysis::document::Document;
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
use crate::TaskExecutionRoot;
use crate::Value;
use crate::http::Downloader;
use crate::http::Location;
use crate::path::EvaluationPath;

pub mod v1;

/// The maximum number of stderr lines to display in error messages.
const MAX_STDERR_LINES: usize = 10;

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
}

impl From<anyhow::Error> for EvaluationError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e)
    }
}

/// Represents a result from evaluating a workflow or task.
pub type EvaluationResult<T> = Result<T, EvaluationError>;

/// Represents context to an expression evaluator.
pub trait EvaluationContext: Send + Sync {
    /// Gets the supported version of the document being evaluated.
    fn version(&self) -> SupportedVersion;

    /// Gets the value of the given name in scope.
    fn resolve_name(&self, name: &str, span: Span) -> Result<Value, Diagnostic>;

    /// Resolves a type name to a type.
    fn resolve_type_name(&self, name: &str, span: Span) -> Result<Type, Diagnostic>;

    /// Gets the working directory for the evaluation.
    ///
    /// Returns `None` if the task execution hasn't occurred yet.
    fn work_dir(&self) -> Option<&EvaluationPath>;

    /// Gets the temp directory for the evaluation.
    fn temp_dir(&self) -> &Path;

    /// Gets the value to return for a call to the `stdout` function.
    ///
    /// This is `Some` only when evaluating task outputs.
    fn stdout(&self) -> Option<&Value>;

    /// Gets the value to return for a call to the `stderr` function.
    ///
    /// This is `Some` only when evaluating task outputs.
    fn stderr(&self) -> Option<&Value>;

    /// Gets the task associated with the evaluation context.
    ///
    /// This is only `Some` when evaluating task hints sections.
    fn task(&self) -> Option<&Task>;

    /// Translates a host path to a guest path.
    ///
    /// Returns `None` if no translation is available.
    fn translate_path(&self, path: &str) -> Option<Cow<'_, Path>>;

    /// Gets the downloader to use for evaluating expressions.
    fn downloader(&self) -> &dyn Downloader;
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
    /// The evaluated task's exit code.
    exit_code: i32,
    /// The task execution root.
    root: Arc<TaskExecutionRoot>,
    /// The working directory of the executed task.
    work_dir: EvaluationPath,
    /// The value to return from the `stdout` function.
    stdout: Value,
    /// The value to return from the `stderr` function.
    stderr: Value,
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
    fn new(root: Arc<TaskExecutionRoot>, result: TaskExecutionResult) -> anyhow::Result<Self> {
        let stdout = PrimitiveValue::new_file(root.stdout().to_str().with_context(|| {
            format!(
                "path to stdout file `{path}` is not UTF-8",
                path = root.stdout().display()
            )
        })?)
        .into();
        let stderr = PrimitiveValue::new_file(root.stderr().to_str().with_context(|| {
            format!(
                "path to stderr file `{path}` is not UTF-8",
                path = root.stderr().display()
            )
        })?)
        .into();

        Ok(Self {
            exit_code: result.exit_code,
            root,
            work_dir: result.work_dir,
            stdout,
            stderr,
            outputs: Ok(Default::default()),
        })
    }

    /// Gets the exit code of the evaluated task.
    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }

    /// Gets the task execution root for the evaluated task.
    pub fn root(&self) -> &TaskExecutionRoot {
        &self.root
    }

    /// Gets the working directory of the evaluated task.
    pub fn work_dir(&self) -> &EvaluationPath {
        &self.work_dir
    }

    /// Gets the stdout value of the evaluated task.
    pub fn stdout(&self) -> &Value {
        &self.stdout
    }

    /// Gets the stderr value of the evaluated task.
    pub fn stderr(&self) -> &Value {
        &self.stderr
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
    fn handle_exit(&self, requirements: &HashMap<String, Value>) -> anyhow::Result<()> {
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
                    if self.exit_code == i32::try_from(*ok).unwrap_or_default() {
                        error = false;
                    }
                }
                Value::Compound(CompoundValue::Array(codes)) => {
                    error = !codes.as_slice().iter().any(|v| {
                        v.as_integer()
                            .map(|i| i32::try_from(i).unwrap_or_default() == self.exit_code)
                            .unwrap_or(false)
                    });
                }
                _ => unreachable!("unexpected return codes value"),
            }
        } else {
            error = self.exit_code != 0;
        }

        if error {
            // Read the last `MAX_STDERR_LINES` number of lines from stderr
            // If there's a problem reading stderr, don't output it
            let stderr = fs::File::open(self.root.stderr())
                .ok()
                .map(|f| {
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
                .unwrap_or_default();

            bail!(
                "task process terminated with exit code {code}: see the `stdout` and `stderr` \
                 files in execution directory `{dir}{MAIN_SEPARATOR}` for task command \
                 output{header}{stderr}{trailer}",
                code = self.exit_code,
                dir = self.root().attempt_dir().display(),
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

/// Represents a `File` or `Directory` input to a task.
#[derive(Debug, Clone)]
pub struct Input {
    /// The path for the input.
    path: EvaluationPath,
    /// The download location for the input.
    ///
    /// This is `Some` if the input has been downloaded to a known location.
    location: Option<Location<'static>>,
    /// The guest path for the input.
    guest_path: Option<String>,
}

impl Input {
    /// Creates a new input with the given path and access.
    pub fn new(path: EvaluationPath) -> Self {
        Self {
            path,
            location: None,
            guest_path: None,
        }
    }

    /// Gets the path to the input.
    pub fn path(&self) -> &EvaluationPath {
        &self.path
    }

    /// Gets the location of the input if it has been downloaded.
    pub fn location(&self) -> Option<&Path> {
        self.location.as_deref()
    }

    /// Sets the location of the input.
    pub fn set_location(&mut self, location: Location<'static>) {
        self.location = Some(location);
    }

    /// Gets the guest path for the input.
    pub fn guest_path(&self) -> Option<&str> {
        self.guest_path.as_deref()
    }

    /// Sets the guest path for the input.
    pub fn set_guest_path(&mut self, path: impl Into<String>) {
        self.guest_path = Some(path.into());
    }
}

/// Represents a node in an input trie.
#[derive(Debug)]
struct InputTrieNode<'a> {
    /// The children of this node.
    ///
    /// A `BTreeMap` is used here to get a consistent walk of the tree.
    children: BTreeMap<&'a str, Self>,
    /// The identifier of the node in the trie.
    ///
    /// A node's identifier is used when formatting guest paths of children.
    id: usize,
    /// The input represented by this node.
    ///
    /// This is `Some` only for terminal nodes in the trie.
    ///
    /// The first element in the tuple is the index of the input.
    input: Option<(usize, &'a Input)>,
}

impl InputTrieNode<'_> {
    /// Constructs a new input trie node with the given component.
    fn new(id: usize) -> Self {
        Self {
            children: Default::default(),
            id,
            input: None,
        }
    }

    /// Calculates the guest path for all terminal nodes in the trie.
    fn calculate_guest_paths(
        &self,
        root: &str,
        parent_id: usize,
        paths: &mut Vec<(usize, String)>,
    ) -> Result<()> {
        // Invoke the callback for any terminal node in the trie
        if let Some((index, input)) = self.input {
            let file_name = input.path.file_name()?.unwrap_or("");

            // If the file name is empty, it means this is a root URL
            let guest_path = if file_name.is_empty() {
                format!(
                    "{root}{sep}{parent_id}/.root",
                    root = root,
                    sep = if root.as_bytes().last() == Some(&b'/') {
                        ""
                    } else {
                        "/"
                    }
                )
            } else {
                format!(
                    "{root}{sep}{parent_id}/{file_name}",
                    root = root,
                    sep = if root.as_bytes().last() == Some(&b'/') {
                        ""
                    } else {
                        "/"
                    },
                )
            };

            paths.push((index, guest_path));
        }

        // Traverse into the children
        for child in self.children.values() {
            child.calculate_guest_paths(root, self.id, paths)?;
        }

        Ok(())
    }
}

/// Represents a prefix trie based on input paths.
///
/// This is used to determine guest paths for inputs.
///
/// From the root to a terminal node represents a unique input.
#[derive(Debug)]
pub struct InputTrie<'a> {
    /// The URL path children of the tree.
    ///
    /// The key in the map is the scheme of each URL.
    ///
    /// A `BTreeMap` is used here to get a consistent walk of the tree.
    urls: BTreeMap<&'a str, InputTrieNode<'a>>,
    /// The local path children of the tree.
    ///
    /// The key in the map is the first component of each path.
    ///
    /// A `BTreeMap` is used here to get a consistent walk of the tree.
    paths: BTreeMap<&'a str, InputTrieNode<'a>>,
    /// The next node identifier.
    next_id: usize,
    /// The number of inputs in the trie.
    count: usize,
}

impl<'a> InputTrie<'a> {
    /// Inserts a new input into the trie.
    pub fn insert(&mut self, input: &'a Input) -> Result<()> {
        let node = match &input.path {
            EvaluationPath::Local(path) => {
                // Don't both inserting anything into the trie for relative paths
                // We still consider the input part of the trie, but it will never have a guest
                // path
                if path.is_relative() {
                    self.count += 1;
                    return Ok(());
                }

                let mut components = path.components();

                let component = components
                    .next()
                    .context("input path cannot be empty")?
                    .as_os_str()
                    .to_str()
                    .with_context(|| {
                        format!("input path `{path}` is not UTF-8", path = path.display())
                    })?;
                let mut node = self.paths.entry(component).or_insert_with(|| {
                    let node = InputTrieNode::new(self.next_id);
                    self.next_id += 1;
                    node
                });

                for component in components {
                    match component {
                        Component::CurDir | Component::ParentDir => {
                            bail!(
                                "input path `{path}` may not contain `.` or `..`",
                                path = path.display()
                            );
                        }
                        _ => {}
                    }

                    let component = component.as_os_str().to_str().with_context(|| {
                        format!("input path `{path}` is not UTF-8", path = path.display())
                    })?;
                    node = node.children.entry(component).or_insert_with(|| {
                        let node = InputTrieNode::new(self.next_id);
                        self.next_id += 1;
                        node
                    });
                }

                node
            }
            EvaluationPath::Remote(url) => {
                // Insert for scheme
                let mut node = self.urls.entry(url.scheme()).or_insert_with(|| {
                    let node = InputTrieNode::new(self.next_id);
                    self.next_id += 1;
                    node
                });

                // Insert the authority
                node = node.children.entry(url.authority()).or_insert_with(|| {
                    let node = InputTrieNode::new(self.next_id);
                    self.next_id += 1;
                    node
                });

                // Insert the path segments
                if let Some(segments) = url.path_segments() {
                    for segment in segments {
                        node = node.children.entry(segment).or_insert_with(|| {
                            let node = InputTrieNode::new(self.next_id);
                            self.next_id += 1;
                            node
                        });
                    }
                }

                // Ignore query parameters and fragments
                node
            }
        };

        node.input = Some((self.count, input));
        self.count += 1;
        Ok(())
    }

    /// Calculates guest paths for the inputs in the trie.
    ///
    /// Returns a collection of input insertion index paired with the calculated
    /// guest path.
    pub fn calculate_guest_paths(&self, root: &str) -> Result<Vec<(usize, String)>> {
        let mut paths = Vec::with_capacity(self.count);
        for child in self.urls.values() {
            child.calculate_guest_paths(root, 0, &mut paths)?;
        }

        for child in self.paths.values() {
            child.calculate_guest_paths(root, 0, &mut paths)?;
        }

        Ok(paths)
    }
}

impl Default for InputTrie<'_> {
    fn default() -> Self {
        Self {
            urls: Default::default(),
            paths: Default::default(),
            // The first id starts at 1 as 0 is considered the "virtual root" of the trie
            next_id: 1,
            count: 0,
        }
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn empty_trie() {
        let empty = InputTrie::default();
        let paths = empty.calculate_guest_paths("/mnt/").unwrap();
        assert!(paths.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn non_empty_trie_unix() {
        let mut trie = InputTrie::default();
        let inputs = [
            Input::new("/".parse().unwrap()),
            Input::new("/foo/bar/foo.txt".parse().unwrap()),
            Input::new("/foo/bar/bar.txt".parse().unwrap()),
            Input::new("/foo/baz/foo.txt".parse().unwrap()),
            Input::new("/foo/baz/bar.txt".parse().unwrap()),
            Input::new("/bar/foo/foo.txt".parse().unwrap()),
            Input::new("/bar/foo/bar.txt".parse().unwrap()),
            Input::new("/baz".parse().unwrap()),
            Input::new("https://example.com/".parse().unwrap()),
            Input::new("https://example.com/foo/bar/foo.txt".parse().unwrap()),
            Input::new("https://example.com/foo/bar/bar.txt".parse().unwrap()),
            Input::new("https://example.com/foo/baz/foo.txt".parse().unwrap()),
            Input::new("https://example.com/foo/baz/bar.txt".parse().unwrap()),
            Input::new("https://example.com/bar/foo/foo.txt".parse().unwrap()),
            Input::new("https://example.com/bar/foo/bar.txt".parse().unwrap()),
            Input::new("https://foo.com/bar".parse().unwrap()),
        ];

        for input in &inputs {
            trie.insert(input).unwrap();
        }

        // The important part of the guest paths are:
        // 1) The guest file name should be the same (or `.root` if the path is
        //    considered to be root)
        // 2) Paths with the same parent should have the same guest parent
        let paths = trie.calculate_guest_paths("/mnt/").unwrap();
        let paths: Vec<_> = paths
            .iter()
            .map(|(index, guest)| (inputs[*index].path().to_str().unwrap(), guest.as_str()))
            .collect();

        assert_eq!(
            paths,
            [
                ("https://example.com/", "/mnt/15/.root"),
                ("https://example.com/bar/foo/bar.txt", "/mnt/25/bar.txt"),
                ("https://example.com/bar/foo/foo.txt", "/mnt/25/foo.txt"),
                ("https://example.com/foo/bar/bar.txt", "/mnt/18/bar.txt"),
                ("https://example.com/foo/bar/foo.txt", "/mnt/18/foo.txt"),
                ("https://example.com/foo/baz/bar.txt", "/mnt/21/bar.txt"),
                ("https://example.com/foo/baz/foo.txt", "/mnt/21/foo.txt"),
                ("https://foo.com/bar", "/mnt/28/bar"),
                ("/", "/mnt/0/.root"),
                ("/bar/foo/bar.txt", "/mnt/10/bar.txt"),
                ("/bar/foo/foo.txt", "/mnt/10/foo.txt"),
                ("/baz", "/mnt/1/baz"),
                ("/foo/bar/bar.txt", "/mnt/3/bar.txt"),
                ("/foo/bar/foo.txt", "/mnt/3/foo.txt"),
                ("/foo/baz/bar.txt", "/mnt/6/bar.txt"),
                ("/foo/baz/foo.txt", "/mnt/6/foo.txt"),
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn non_empty_trie_windows() {
        let mut trie = InputTrie::default();
        let inputs = [
            Input::new("C:\\".parse().unwrap()),
            Input::new("C:\\foo\\bar\\foo.txt".parse().unwrap()),
            Input::new("C:\\foo\\bar\\bar.txt".parse().unwrap()),
            Input::new("C:\\foo\\baz\\foo.txt".parse().unwrap()),
            Input::new("C:\\foo\\baz\\bar.txt".parse().unwrap()),
            Input::new("C:\\bar\\foo\\foo.txt".parse().unwrap()),
            Input::new("C:\\bar\\foo\\bar.txt".parse().unwrap()),
            Input::new("C:\\baz".parse().unwrap()),
            Input::new("https://example.com/".parse().unwrap()),
            Input::new("https://example.com/foo/bar/foo.txt".parse().unwrap()),
            Input::new("https://example.com/foo/bar/bar.txt".parse().unwrap()),
            Input::new("https://example.com/foo/baz/foo.txt".parse().unwrap()),
            Input::new("https://example.com/foo/baz/bar.txt".parse().unwrap()),
            Input::new("https://example.com/bar/foo/foo.txt".parse().unwrap()),
            Input::new("https://example.com/bar/foo/bar.txt".parse().unwrap()),
            Input::new("https://foo.com/bar".parse().unwrap()),
        ];

        for input in &inputs {
            trie.insert(input).unwrap();
        }

        // The important part of the guest paths are:
        // 1) The guest file name should be the same (or `.root` if the path is
        //    considered to be root)
        // 2) Paths with the same parent should have the same guest parent
        let paths = trie.calculate_guest_paths("/mnt/").unwrap();
        let paths: Vec<_> = paths
            .iter()
            .map(|(index, guest)| (inputs[*index].path().to_str().unwrap(), guest.as_str()))
            .collect();

        assert_eq!(
            paths,
            [
                ("https://example.com/", "/mnt/16/.root"),
                ("https://example.com/bar/foo/bar.txt", "/mnt/26/bar.txt"),
                ("https://example.com/bar/foo/foo.txt", "/mnt/26/foo.txt"),
                ("https://example.com/foo/bar/bar.txt", "/mnt/19/bar.txt"),
                ("https://example.com/foo/bar/foo.txt", "/mnt/19/foo.txt"),
                ("https://example.com/foo/baz/bar.txt", "/mnt/22/bar.txt"),
                ("https://example.com/foo/baz/foo.txt", "/mnt/22/foo.txt"),
                ("https://foo.com/bar", "/mnt/29/bar"),
                ("C:\\", "/mnt/1/.root"),
                ("C:\\bar\\foo\\bar.txt", "/mnt/11/bar.txt"),
                ("C:\\bar\\foo\\foo.txt", "/mnt/11/foo.txt"),
                ("C:\\baz", "/mnt/2/baz"),
                ("C:\\foo\\bar\\bar.txt", "/mnt/4/bar.txt"),
                ("C:\\foo\\bar\\foo.txt", "/mnt/4/foo.txt"),
                ("C:\\foo\\baz\\bar.txt", "/mnt/7/bar.txt"),
                ("C:\\foo\\baz\\foo.txt", "/mnt/7/foo.txt"),
            ]
        );
    }
}
