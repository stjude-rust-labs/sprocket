//! WorkDir for storing `Repository` files.

use std::path::Path;

use indexmap::IndexMap;
use tempfile::TempDir;

use crate::repository::Repository;
use crate::repository::identifier::Identifier;

/// A working directory for storing `Repository` files.
#[derive(Debug)]
pub struct WorkDir {
    /// The root directory of the `WorkDir`.
    root: TempDir,

    /// The repositories stored in the `WorkDir`.
    repositories: IndexMap<Identifier, Repository>,
}

/// Create a default `WorkDir`.
impl Default for WorkDir {
    fn default() -> Self {
        Self::new(false)
    }
}

impl WorkDir {
    /// Create a new `WorkDir`.
    pub fn new(keep: bool) -> Self {
        Self {
            root: tempfile::Builder::new()
                .disable_cleanup(keep)
                .tempdir()
                .expect("failed to create temporary directory"),
            repositories: IndexMap::new(),
        }
    }

    /// Get the root directory of the `WorkDir`.
    pub fn root(&self) -> &Path {
        self.root.path()
    }

    /// Get the repositories stored in the `WorkDir`.
    pub fn repositories(&self) -> &IndexMap<Identifier, Repository> {
        &self.repositories
    }

    /// Add a repository to the `WorkDir` from an [`Identifier`].
    /// By a guarantee of [`Repository::new()`], the added repository will
    /// _always_ have `Some(commit_hash)`.
    pub fn add_by_identifier(&mut self, identifier: &Identifier) {
        let repository = Repository::new(identifier.clone(), None, self.root());

        self.repositories.insert(identifier.clone(), repository);
    }

    /// Get a repository from the `WorkDir` by its identifier.
    pub fn get_repository(&self, identifier: &Identifier) -> Option<&Repository> {
        self.repositories.get(identifier)
    }
}
