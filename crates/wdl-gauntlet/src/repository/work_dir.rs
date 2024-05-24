//! WorkDir for storing `Repository` files.

use std::path::Path;

use indexmap::IndexMap;
use temp_dir::TempDir;

use crate::repository::identifier::Identifier;
use crate::repository::Repository;

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
        Self::new()
    }
}

impl WorkDir {
    /// Create a new `WorkDir`.
    pub fn new() -> Self {
        Self {
            root: TempDir::new().expect("failed to create temporary directory"),
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
        let repository = Repository::new(
            identifier.clone(),
            None,
            &self
                .root
                .path()
                .join(identifier.organization())
                .join(identifier.name()),
        );

        self.repositories.insert(identifier.clone(), repository);
    }

    /// Get a repository from the `WorkDir` by its identifier.
    pub fn get_repository(&self, identifier: &Identifier) -> Option<&Repository> {
        self.repositories.get(identifier)
    }
}
