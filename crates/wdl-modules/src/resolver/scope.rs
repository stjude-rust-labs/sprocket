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

impl DependencyScope {
    /// Returns `true` if this is a transitive dependency.
    pub fn is_transitive(self) -> bool {
        self == Self::Transitive
    }
}
