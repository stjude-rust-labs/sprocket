//! Module for evaluation.

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
use wdl_ast::Ident;
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
pub trait EvaluationContext {
    /// Gets the supported version of the document being evaluated.
    fn version(&self) -> SupportedVersion;

    /// Gets the value of the given name in scope.
    fn resolve_name(&self, name: &Ident) -> Result<Value, Diagnostic>;

    /// Resolves a type name to a type.
    fn resolve_type_name(&mut self, name: &Ident) -> Result<Type, Diagnostic>;

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

/// Represents a mount point for use in task execution backends that use
/// containers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountPoint {
    /// The host path for the mount point.
    pub host: PathBuf,
    /// The guest path for the mount point.
    pub guest: PathBuf,
    /// Whether or not the mount should be read-only.
    pub read_only: bool,
}

impl MountPoint {
    /// Creates a new mount point with the given host and guest paths.
    pub fn new(host: impl Into<PathBuf>, guest: impl Into<PathBuf>, read_only: bool) -> Self {
        Self {
            host: host.into(),
            guest: guest.into(),
            read_only,
        }
    }
}

/// Represents mount points for mapping host and guest paths for task execution
/// backends that use containers.
#[derive(Debug, Default)]
pub struct MountPoints(Vec<MountPoint>);

impl MountPoints {
    /// Gets the guest path for the given host path.
    ///
    /// Returns `None` if there is no guest path mapped for the given path.
    pub fn guest(&self, host: impl AsRef<Path>) -> Option<PathBuf> {
        let host = host.as_ref();

        for mp in &self.0 {
            if let Ok(stripped) = host.strip_prefix(&mp.host) {
                return Some(Path::new(&mp.guest).join(stripped));
            }
        }

        None
    }

    /// Gets the host path for the given guest path.
    ///
    /// Returns `None` if there is no host path mapped for the given path.
    pub fn host(&self, guest: impl AsRef<Path>) -> Option<PathBuf> {
        let guest = guest.as_ref();

        for mp in &self.0 {
            if let Ok(stripped) = guest.strip_prefix(&mp.guest) {
                return Some(Path::new(&mp.host).join(stripped));
            }
        }

        None
    }

    /// Returns an iterator of mount point host path to mount point guest path
    /// and whether or not the mounting should be read-only.
    pub fn iter(&self) -> impl Iterator<Item = &MountPoint> {
        self.0.iter()
    }

    /// Returns the number of mount points in the collection.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the collection contains no mount points.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Inserts a mount point.
    pub fn insert(&mut self, mp: impl Into<MountPoint>) {
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
}

impl<'a> PathTrieNode<'a> {
    /// Constructs a new path trie node with the given normal path component.
    fn new(component: Component<'a>) -> Self {
        Self {
            component,
            children: Default::default(),
        }
    }

    /// Determines if the node is considered a mount point.
    ///
    /// A mount point is a non-root node that:
    ///
    /// * has more than one child
    /// * has a single terminal child
    /// * is a terminal node
    fn is_mount_point(&self) -> bool {
        !matches!(self.component, Component::RootDir | Component::Prefix(_))
            && (self.children.is_empty()
                || self.children.len() > 1
                || self
                    .children
                    .first_key_value()
                    .map(|(_, v)| v.children.is_empty())
                    .unwrap_or(false))
    }

    /// Inserts any mount points for the node.
    fn insert_mount_points(&self, root: &'a Path, host: &mut PathBuf, mounts: &mut MountPoints) {
        // Push the component onto the host path and pop it after any traversals
        host.push(self.component);

        if let Component::Prefix(_) = self.component {
            // Because we store the root and prefix in reverse order in the trie, push a
            // root following a prefix
            host.push(Component::RootDir);
        }

        // If this node is a mount point, insert it
        if self.is_mount_point() {
            // Use format! for the path so that it always appears unix-style
            mounts.0.push(MountPoint {
                host: host.clone(),
                guest: format!(
                    "{root}{sep}{num}",
                    root = root.display(),
                    sep = if root.as_os_str().as_encoded_bytes().last() == Some(&b'/') {
                        ""
                    } else {
                        "/"
                    },
                    num = mounts.0.len()
                )
                .into(),
                read_only: true, // All mounts from the path trie are always read-only
            });
        } else {
            // Otherwise, traverse into the children
            for child in self.children.values() {
                child.insert_mount_points(root, host, mounts);
            }
        }

        host.pop();

        if let Component::Prefix(_) = self.component {
            host.pop();
        }
    }
}

impl Default for PathTrieNode<'_> {
    fn default() -> Self {
        Self {
            component: Component::RootDir,
            children: Default::default(),
        }
    }
}

/// Represents a prefix trie based on file system paths.
///
/// This is used to determine container mount points.
///
/// From the root to the terminal node represents a host path.
///
/// A mount point is any non-root node in the tree that is either:
///
/// * The ancestor closest to the root that contains more than one child node
///
/// or
///
/// * The immediate parent of a terminal node where no ancestor has more than
///   one child node
///
/// Paths are mapped according to their mount points.
#[derive(Debug, Default)]
pub struct PathTrie<'a> {
    /// The root node of the trie.
    root: PathTrieNode<'a>,
}

impl<'a> PathTrie<'a> {
    /// Inserts a new path into the trie.
    ///
    /// # Panics
    ///
    /// Panics if the provided path is not absolute or if it contains `.` or
    /// `..` components.
    pub fn insert(&mut self, path: &'a Path) {
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
            };

            node = node
                .children
                .entry(component.as_os_str())
                .or_insert_with(|| PathTrieNode::new(component));
        }
    }

    /// Converts the path trie into mount points based on the provided guest
    /// root directory.
    pub fn into_mount_points(self, root: impl AsRef<Path>) -> MountPoints {
        let mut mounts = MountPoints::default();
        let mut host = PathBuf::new();
        self.root
            .insert_mount_points(root.as_ref(), &mut host, &mut mounts);
        mounts
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn empty_trie() {
        let empty = PathTrie::default();
        let mounts = empty.into_mount_points("/mnt/");
        assert_eq!(mounts.iter().count(), 0);
        assert_eq!(mounts.len(), 0);
        assert!(mounts.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn root_only_trie_unix() {
        let mut trie = PathTrie::default();
        trie.insert(Path::new("/"));
        let mounts = trie.into_mount_points("/mnt/");
        assert_eq!(mounts.iter().count(), 0);
        assert_eq!(mounts.len(), 0);
        assert!(mounts.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn root_only_trie_windows() {
        let mut trie = PathTrie::default();
        trie.insert(Path::new("C:\\"));
        let mounts = trie.into_mount_points("/mnt/");
        assert_eq!(mounts.iter().count(), 0);
        assert_eq!(mounts.len(), 0);
        assert!(mounts.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn trie_with_common_paths_unix() {
        let mut trie = PathTrie::default();
        trie.insert(Path::new("/foo/bar/foo.txt"));
        trie.insert(Path::new("/foo/bar/bar.txt"));
        trie.insert(Path::new("/foo/baz/foo.txt"));
        trie.insert(Path::new("/foo/baz/bar.txt"));
        trie.insert(Path::new("/bar/foo/foo.txt"));
        trie.insert(Path::new("/bar/foo/bar.txt"));
        trie.insert(Path::new("/baz"));

        let mounts = trie.into_mount_points("/mnt");

        // Note: the mount points are always in lexical order
        let mapped: Vec<_> = mounts.iter().collect();
        assert_eq!(
            mapped,
            [
                &MountPoint::new("/bar/foo", "/mnt/0", true),
                &MountPoint::new("/baz", "/mnt/1", true),
                &MountPoint::new("/foo", "/mnt/2", true),
            ]
        );

        for (host, guest) in [
            ("/foo", "/mnt/2"),
            ("/foo/bar/foo.txt", "/mnt/2/bar/foo.txt"),
            ("/foo/bar/bar.txt", "/mnt/2/bar/bar.txt"),
            ("/foo/bar/bar.txt", "/mnt/2/bar/bar.txt"),
            ("/foo/baz/foo.txt", "/mnt/2/baz/foo.txt"),
            ("/foo/baz/bar.txt", "/mnt/2/baz/bar.txt"),
            ("/bar/foo/", "/mnt/0"),
            ("/bar/foo/foo.txt", "/mnt/0/foo.txt"),
            ("/bar/foo/bar.txt", "/mnt/0/bar.txt"),
            ("/baz", "/mnt/1"),
            ("/baz/any/other/path", "/mnt/1/any/other/path"),
        ] {
            assert_eq!(
                mounts.guest(host),
                Some(PathBuf::from(guest)),
                "unexpected guest path for host path `{host}`"
            );
            assert_eq!(
                mounts.host(guest),
                Some(PathBuf::from(host)),
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
        trie.insert(Path::new("C:\\foo\\bar\\foo.txt"));
        trie.insert(Path::new("C:\\foo\\bar\\bar.txt"));
        trie.insert(Path::new("C:\\foo\\baz\\foo.txt"));
        trie.insert(Path::new("C:\\foo\\baz\\bar.txt"));
        trie.insert(Path::new("C:\\bar\\foo\\foo.txt"));
        trie.insert(Path::new("C:\\bar\\foo\\bar.txt"));
        trie.insert(Path::new("D:\\baz"));

        let mounts = trie.into_mount_points("/mnt");

        // Note: the mount points are always in lexical order
        let mapped: Vec<_> = mounts.iter().collect();
        assert_eq!(
            mapped,
            [
                &MountPoint::new("C:\\bar\\foo", "/mnt/0", true),
                &MountPoint::new("C:\\foo", "/mnt/1", true),
                &MountPoint::new("D:\\baz", "/mnt/2", true),
            ]
        );

        for (host, guest) in [
            ("C:\\foo", "/mnt/1"),
            ("C:\\foo\\bar\\foo.txt", "/mnt/1/bar/foo.txt"),
            ("C:\\foo\\bar\\bar.txt", "/mnt/1/bar/bar.txt"),
            ("C:\\foo\\bar\\bar.txt", "/mnt/1/bar/bar.txt"),
            ("C:\\foo\\baz\\foo.txt", "/mnt/1/baz/foo.txt"),
            ("C:\\foo\\baz\\bar.txt", "/mnt/1/baz/bar.txt"),
            ("C:\\bar\\foo\\", "/mnt/0"),
            ("C:\\bar\\foo\\foo.txt", "/mnt/0/foo.txt"),
            ("C:\\bar\\foo\\bar.txt", "/mnt/0/bar.txt"),
            ("D:\\baz", "/mnt/2"),
            ("D:\\baz\\any\\other\\path", "/mnt/2/any/other/path"),
        ] {
            assert_eq!(
                mounts.guest(host),
                Some(PathBuf::from(guest)),
                "unexpected guest path for host path `{host}`"
            );
            assert_eq!(
                mounts.host(guest),
                Some(PathBuf::from(host)),
                "unexpected host path for guest path `{guest}`"
            );
        }

        // Check for paths not in the host or guest mapping
        assert!(mounts.guest("/tmp/foo.txt").is_none());
        assert!(mounts.host("/tmp/bar.txt").is_none());
    }
}
