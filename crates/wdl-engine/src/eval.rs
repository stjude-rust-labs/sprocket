//! Module for expression evaluation.

use indexmap::IndexMap;

use crate::Value;

pub mod v1;

/// Represents an index of a scope in a collection of scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeIndex(usize);

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
#[derive(Debug)]
pub struct Scope {
    /// The index of the parent scope.
    ///
    /// This is `None` for task and workflow scopes.
    parent: Option<ScopeIndex>,
    /// The map of names in scope to their values.
    names: IndexMap<String, Value>,
}

impl Scope {
    /// Creates a new scope given the parent scope.
    pub fn new(parent: Option<ScopeIndex>) -> Self {
        Self {
            parent,
            names: Default::default(),
        }
    }

    /// Inserts a name into the scope.
    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        self.names.insert(name.into(), value.into());
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
