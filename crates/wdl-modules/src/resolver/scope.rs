//! Dependency scope types for the resolver.

/// Whether a dependency is declared directly by the consumer or
/// reached transitively through another dependency.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyScope {
    /// Declared in the consumer's own `module.json`.
    TopLevel,
    /// Reached through a transitive dependency chain.
    Transitive,
}

/// Whether to resolve mutable selectors against the remote or replay
/// a locked commit.
#[cfg(feature = "resolver")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolutionMode<'a> {
    /// Resolve mutable selectors against the remote. Used by
    /// `resolve_tree` when computing a fresh dependency graph.
    Fresh,
    /// Replay the locked commit from the lockfile. Used by
    /// `materialize` when reproducing a previously-locked dependency.
    Locked {
        /// The lockfile path that contains the dependency entry.
        lockfile_scope: &'a [crate::dependency::DependencyName],
    },
}
