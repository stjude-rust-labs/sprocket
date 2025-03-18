//! Module for evaluation.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Component;
use std::path::MAIN_SEPARATOR;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use indexmap::IndexMap;
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
use crate::TaskExecutionRoot;
use crate::Value;

pub mod v1;

/// Represents an error that may occur when evaluating a workflow or task.
#[derive(Debug)]
pub enum EvaluationError {
    /// The error came from WDL source evaluation.
    Source(Diagnostic),
    /// The error came from another source.
    Other(anyhow::Error),
}

impl From<Diagnostic> for EvaluationError {
    fn from(diagnostic: Diagnostic) -> Self {
        Self::Source(diagnostic)
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
    fn work_dir(&self) -> &Path;

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
    fn translate_path(&self, path: &Path) -> Option<Cow<'_, Path>>;
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
    pub fn for_each(&self, mut cb: impl FnMut(&str, &Value)) {
        let mut current = Some(self.index);

        while let Some(index) = current {
            for (n, v) in self.scopes[index.0].local() {
                cb(n, v);
            }

            current = self.scopes[index.0].parent;
        }
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
    /// The evaluated task's status code.
    status_code: i32,
    /// The working directory of the executed task.
    work_dir: PathBuf,
    /// The command file of the executed task.
    command: PathBuf,
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
    fn new(root: &TaskExecutionRoot, status_code: i32) -> anyhow::Result<Self> {
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
            status_code,
            work_dir: root.work_dir().into(),
            command: root.command().into(),
            stdout,
            stderr,
            outputs: Ok(Default::default()),
        })
    }

    /// Gets the status code of the evaluated task.
    pub fn status_code(&self) -> i32 {
        self.status_code
    }

    /// Gets the working directory of the evaluated task.
    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    /// Gets the command file of the evaluated task.
    pub fn command(&self) -> &Path {
        &self.command
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
                    if self.status_code == i32::try_from(*ok).unwrap_or_default() {
                        error = false;
                    }
                }
                Value::Compound(CompoundValue::Array(codes)) => {
                    error = !codes.as_slice().iter().any(|v| {
                        v.as_integer()
                            .map(|i| i32::try_from(i).unwrap_or_default() == self.status_code)
                            .unwrap_or(false)
                    });
                }
                _ => unreachable!("unexpected return codes value"),
            }
        } else {
            error = self.status_code != 0;
        }

        if error {
            bail!(
                "task process has terminated with status code {code}; see the `stdout` and \
                 `stderr` files in execution directory `{dir}{MAIN_SEPARATOR}` for task command \
                 output",
                code = self.status_code,
                dir = Path::new(self.stderr.as_file().unwrap().as_str())
                    .parent()
                    .expect("parent should exist")
                    .display(),
            );
        }

        Ok(())
    }
}

/// Represents a mount of a file or directory for backends that use containers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    /// The host path for the mount.
    pub host: PathBuf,
    /// The guest path for the mount.
    pub guest: PathBuf,
    /// Whether or not the mount should be read-only.
    pub read_only: bool,
}

impl Mount {
    /// Creates a new mount with the given host and guest paths.
    pub fn new(host: impl Into<PathBuf>, guest: impl Into<PathBuf>, read_only: bool) -> Self {
        Self {
            host: host.into(),
            guest: guest.into(),
            read_only,
        }
    }
}

/// Represents a collection of mounts for mapping host and guest paths for task
/// execution backends that use containers.
#[derive(Debug, Default)]
pub struct Mounts(Vec<Mount>);

impl Mounts {
    /// Gets the guest path for the given host path.
    ///
    /// Returns `None` if there is no guest path mapped for the given path.
    pub fn guest(&self, host: impl AsRef<Path>) -> Option<Cow<'_, Path>> {
        let host = host.as_ref();

        for mp in &self.0 {
            if let Ok(stripped) = host.strip_prefix(&mp.host) {
                if stripped.as_os_str().is_empty() {
                    return Some(mp.guest.as_path().into());
                }

                return Some(Path::new(&mp.guest).join(stripped).into());
            }
        }

        None
    }

    /// Gets the host path for the given guest path.
    ///
    /// Returns `None` if there is no host path mapped for the given path.
    pub fn host(&self, guest: impl AsRef<Path>) -> Option<Cow<'_, Path>> {
        let guest = guest.as_ref();

        for mp in &self.0 {
            if let Ok(stripped) = guest.strip_prefix(&mp.guest) {
                if stripped.as_os_str().is_empty() {
                    return Some(mp.host.as_path().into());
                }

                return Some(Path::new(&mp.host).join(stripped).into());
            }
        }

        None
    }

    /// Returns an iterator of mounts within the collection.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Mount> {
        self.0.iter()
    }

    /// Returns the number of mounts in the collection.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the collection is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Inserts a mount into the collection.
    pub fn insert(&mut self, mp: impl Into<Mount>) {
        self.0.push(mp.into());
    }
}

/// Represents a node in a path trie.
#[derive(Debug)]
struct PathTrieNode<'a> {
    /// The path component represented by this node.
    component: Component<'a>,
    /// The children of this node.
    ///
    /// A `BTreeMap` is used here to get a consistent walk of the tree.
    children: BTreeMap<&'a OsStr, Self>,
    /// The identifier of the node in the trie.
    ///
    /// A node's identifier is used when formatting guest paths of children.
    id: usize,
    /// Whether or not the node is terminal.
    ///
    /// A value of `true` indicates that the path was explicitly inserted into
    /// the trie.
    terminal: bool,
    /// Whether or not a mount at a terminal path is considered read-only.
    read_only: bool,
}

impl<'a> PathTrieNode<'a> {
    /// Constructs a new path trie node with the given normal path component.
    fn new(component: Component<'a>, id: usize) -> Self {
        Self {
            component,
            children: Default::default(),
            id,
            terminal: false,
            read_only: false,
        }
    }

    /// Inserts any mounts for the node.
    fn insert_mounts(
        &self,
        root: &'a Path,
        host: &mut PathBuf,
        mounts: &mut Mounts,
        parent_id: usize,
    ) {
        // Push the component onto the host path and pop it after any traversals
        host.push(self.component);

        if let Component::Prefix(_) = self.component {
            // Because we store the root and prefix in reverse order in the trie, push a
            // root following a prefix
            host.push(Component::RootDir);
        }

        // For terminal nodes, we add a mount and stop recursing
        // Any terminal nodes that are descendant from this node will be treated as
        // relative to this node in any mappings
        if self.terminal {
            let filename = host.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Use format! for the path so that it always appears unix-style
            // The parent id is used so that children of the same parent share the same
            // parent directory
            mounts.0.push(Mount {
                host: host.clone(),
                guest: format!(
                    "{root}{sep}{parent_id}{sep2}{filename}",
                    root = root.display(),
                    sep = if root.as_os_str().as_encoded_bytes().last() == Some(&b'/') {
                        ""
                    } else {
                        "/"
                    },
                    sep2 = if filename.is_empty() { "" } else { "/" },
                )
                .into(),
                read_only: self.read_only,
            });
        } else {
            // Otherwise, traverse into the children
            for child in self.children.values() {
                child.insert_mounts(root, host, mounts, self.id);
            }
        }

        if let Component::Prefix(_) = self.component {
            host.pop();
        }

        host.pop();
    }
}

impl Default for PathTrieNode<'_> {
    fn default() -> Self {
        Self {
            component: Component::RootDir,
            children: Default::default(),
            id: 0,
            terminal: false,
            read_only: false,
        }
    }
}

/// Represents a prefix trie based on file system paths.
///
/// This is used to determine container mounts.
///
/// From the root to a terminal node represents a host path.
///
/// If a terminal path has descendants that are also terminal, only the ancestor
/// nearest the root will be added as a mount; its descendants will be mapped as
/// relative paths.
///
/// Host and guest paths are mapped according to the mounts.
#[derive(Debug)]
pub struct PathTrie<'a> {
    /// The root node of the trie.
    ///
    /// `None` indicates an empty trie.
    root: PathTrieNode<'a>,
    /// The number of nodes in the trie.
    ///
    /// Used to provide an identifier to each node.
    ///
    /// The trie always has at least one node (the root).
    count: usize,
}

impl<'a> PathTrie<'a> {
    /// Inserts a new path into the trie.
    ///
    /// # Panics
    ///
    /// Panics if the provided path is not absolute or if it contains `.` or
    /// `..` components.
    pub fn insert(&mut self, path: &'a Path, read_only: bool) {
        assert!(
            path.is_absolute(),
            "a path must be absolute to add to the trie"
        );

        let mut node = &mut self.root;

        for component in path.components() {
            match component {
                Component::RootDir => {
                    // Skip the root directory as we already have it in the trie
                    continue;
                }
                Component::Prefix(_) | Component::Normal(_) => {
                    // Accept the component
                }
                Component::CurDir | Component::ParentDir => {
                    panic!("path may not contain `.` or `..`");
                }
            }

            node = node
                .children
                .entry(component.as_os_str())
                .or_insert_with(|| {
                    let node = PathTrieNode::new(component, self.count);
                    self.count += 1;
                    node
                });
        }

        node.terminal = true;
        node.read_only = read_only;
    }

    /// Converts the path trie into mounts based on the provided guest root
    /// directory.
    pub fn into_mounts(self, guest_root: impl AsRef<Path>) -> Mounts {
        let mut mounts = Mounts::default();
        let mut host = PathBuf::new();
        self.root
            .insert_mounts(guest_root.as_ref(), &mut host, &mut mounts, 0);
        mounts
    }
}

impl Default for PathTrie<'_> {
    fn default() -> Self {
        Self {
            root: Default::default(),
            count: 1,
        }
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn empty_trie() {
        let empty = PathTrie::default();
        let mounts = empty.into_mounts("/mnt/");
        assert_eq!(mounts.iter().count(), 0);
        assert_eq!(mounts.len(), 0);
        assert!(mounts.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn trie_with_terminal_root_unix() {
        let mut trie = PathTrie::default();
        trie.insert(Path::new("/"), true);
        trie.insert(Path::new("/relative/from/root"), true);
        let mounts = trie.into_mounts("/mnt/");
        assert_eq!(mounts.iter().count(), 1);
        assert_eq!(mounts.len(), 1);
        assert!(!mounts.is_empty());

        // Note: the mounts are always in lexical order
        let collected: Vec<_> = mounts.iter().collect();
        assert_eq!(collected, [&Mount::new("/", "/mnt/0", true)]);

        for (host, guest) in [
            ("/", "/mnt/0"),
            ("/foo/bar/foo.txt", "/mnt/0/foo/bar/foo.txt"),
            ("/bar/foo/foo.txt", "/mnt/0/bar/foo/foo.txt"),
            ("/any/other/path", "/mnt/0/any/other/path"),
        ] {
            assert_eq!(
                mounts.guest(host).as_deref(),
                Some(Path::new(guest)),
                "unexpected guest path for host path `{host}`"
            );
            assert_eq!(
                mounts.host(guest).as_deref(),
                Some(Path::new(host)),
                "unexpected host path for guest path `{guest}`"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn root_with_terminal_root_windows() {
        let mut trie = PathTrie::default();
        trie.insert(Path::new("C:\\"), true);
        trie.insert(Path::new("C:\\relative\\from\\root"), true);
        let mounts = trie.into_mounts("/mnt/");
        assert_eq!(mounts.iter().count(), 1);
        assert_eq!(mounts.len(), 1);
        assert!(!mounts.is_empty());

        // Note: the mounts are always in lexical order
        let collected: Vec<_> = mounts.iter().collect();
        assert_eq!(collected, [&Mount::new("C:\\", "/mnt/0", true)]);

        for (host, guest) in [
            ("C:\\", "/mnt/0"),
            ("C:\\foo\\bar\\foo.txt", "/mnt/0/foo/bar/foo.txt"),
            ("C:\\bar\\foo\\foo.txt", "/mnt/0/bar/foo/foo.txt"),
            ("C:\\any\\other\\path", "/mnt/0/any/other/path"),
        ] {
            assert_eq!(
                mounts.guest(host).as_deref(),
                Some(Path::new(guest)),
                "unexpected guest path for host path `{host}`"
            );
            assert_eq!(
                mounts.host(guest).as_deref(),
                Some(Path::new(host)),
                "unexpected host path for guest path `{guest}`"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn trie_with_common_paths_unix() {
        let mut trie = PathTrie::default();
        trie.insert(Path::new("/foo/bar/foo.txt"), true);
        trie.insert(Path::new("/foo/bar/bar.txt"), true);
        trie.insert(Path::new("/foo/baz/foo.txt"), true);
        trie.insert(Path::new("/foo/baz/bar.txt"), true);
        trie.insert(Path::new("/bar/foo/foo.txt"), true);
        trie.insert(Path::new("/bar/foo/bar.txt"), true);
        trie.insert(Path::new("/baz"), true);

        let mounts = trie.into_mounts("/mnt");

        // Note: the mounts are always in lexical order
        let collected: Vec<_> = mounts.iter().collect();
        assert_eq!(
            collected,
            [
                &Mount::new("/bar/foo/bar.txt", "/mnt/9/bar.txt", true),
                &Mount::new("/bar/foo/foo.txt", "/mnt/9/foo.txt", true),
                &Mount::new("/baz", "/mnt/0/baz", true),
                &Mount::new("/foo/bar/bar.txt", "/mnt/2/bar.txt", true),
                &Mount::new("/foo/bar/foo.txt", "/mnt/2/foo.txt", true),
                &Mount::new("/foo/baz/bar.txt", "/mnt/5/bar.txt", true),
                &Mount::new("/foo/baz/foo.txt", "/mnt/5/foo.txt", true),
            ]
        );

        for (host, guest) in [
            ("/foo/bar/foo.txt", "/mnt/2/foo.txt"),
            ("/foo/bar/bar.txt", "/mnt/2/bar.txt"),
            ("/foo/baz/foo.txt", "/mnt/5/foo.txt"),
            ("/foo/baz/bar.txt", "/mnt/5/bar.txt"),
            ("/bar/foo/foo.txt", "/mnt/9/foo.txt"),
            ("/bar/foo/bar.txt", "/mnt/9/bar.txt"),
            ("/baz", "/mnt/0/baz"),
            ("/baz/any/other/path", "/mnt/0/baz/any/other/path"),
        ] {
            assert_eq!(
                mounts.guest(host).as_deref(),
                Some(Path::new(guest)),
                "unexpected guest path for host path `{host}`"
            );
            assert_eq!(
                mounts.host(guest).as_deref(),
                Some(Path::new(host)),
                "unexpected host path for guest path `{guest}`"
            );
        }

        // Check for paths not in the host or guest mapping
        assert!(mounts.guest("/tmp/foo.txt").is_none());
        assert!(mounts.host("/tmp/bar.txt").is_none());
    }

    #[cfg(windows)]
    #[test]
    fn trie_with_common_paths_windows() {
        let mut trie = PathTrie::default();
        trie.insert(Path::new("C:\\foo\\bar\\foo.txt"), true);
        trie.insert(Path::new("C:\\foo\\bar\\bar.txt"), true);
        trie.insert(Path::new("C:\\foo\\baz\\foo.txt"), true);
        trie.insert(Path::new("C:\\foo\\baz\\bar.txt"), true);
        trie.insert(Path::new("C:\\bar\\foo\\foo.txt"), true);
        trie.insert(Path::new("C:\\bar\\foo\\bar.txt"), true);
        trie.insert(Path::new("C:\\baz"), true);

        let mounts = trie.into_mounts("/mnt");

        // Note: the mounts are always in lexical order
        let collected: Vec<_> = mounts.iter().collect();
        assert_eq!(
            collected,
            [
                &Mount::new("C:\\bar\\foo\\bar.txt", "/mnt/10/bar.txt", true),
                &Mount::new("C:\\bar\\foo\\foo.txt", "/mnt/10/foo.txt", true),
                &Mount::new("C:\\baz", "/mnt/1/baz", true),
                &Mount::new("C:\\foo\\bar\\bar.txt", "/mnt/3/bar.txt", true),
                &Mount::new("C:\\foo\\bar\\foo.txt", "/mnt/3/foo.txt", true),
                &Mount::new("C:\\foo\\baz\\bar.txt", "/mnt/6/bar.txt", true),
                &Mount::new("C:\\foo\\baz\\foo.txt", "/mnt/6/foo.txt", true),
            ]
        );

        for (host, guest) in [
            ("C:\\foo\\bar\\foo.txt", "/mnt/3/foo.txt"),
            ("C:\\foo\\bar\\bar.txt", "/mnt/3/bar.txt"),
            ("C:\\foo\\baz\\foo.txt", "/mnt/6/foo.txt"),
            ("C:\\foo\\baz\\bar.txt", "/mnt/6/bar.txt"),
            ("C:\\bar\\foo\\foo.txt", "/mnt/10/foo.txt"),
            ("C:\\bar\\foo\\bar.txt", "/mnt/10/bar.txt"),
            ("C:\\baz", "/mnt/1/baz"),
            ("C:\\baz\\any\\other\\path", "/mnt/1/baz/any/other/path"),
        ] {
            assert_eq!(
                mounts.guest(host).as_deref(),
                Some(Path::new(guest)),
                "unexpected guest path for host path `{host}`"
            );
            assert_eq!(
                mounts.host(guest).as_deref(),
                Some(Path::new(host)),
                "unexpected host path for guest path `{guest}`"
            );
        }

        // Check for paths not in the host or guest mapping
        assert!(mounts.guest("/tmp/foo.txt").is_none());
        assert!(mounts.host("/tmp/bar.txt").is_none());
    }
}
