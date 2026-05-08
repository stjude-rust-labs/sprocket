//! Ownership-aware module root types.

use std::path::Path;
use std::path::PathBuf;

/// A path guaranteed to contain only module content (no `.git`,
/// `.sparse.json`, or other resolver metadata). Accepted by hashing,
/// signing, tree validation, and materialization functions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModuleRoot(PathBuf);

impl ModuleRoot {
    /// Wraps a path as a module root.
    pub(crate) fn new(path: PathBuf) -> Self {
        Self(path)
    }
}

impl AsRef<Path> for ModuleRoot {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// Distinguishes resolver-owned cache paths from user-owned local
/// paths. Only `Cached` variants may be evicted.
#[derive(Clone, Debug)]
pub(crate) enum MaterializedRoot {
    /// A user's local module directory. Must never be evicted.
    Local(ModuleRoot),
    /// A resolver-owned cache leaf.
    Cached {
        /// The module content root inside the cache leaf.
        module_root: ModuleRoot,
        // NOTE: `#[expect(dead_code)]` would error when eviction
        // consumes this field; `#[allow]` is used because consumption
        // depends on a later task in this refactor sequence.
        #[allow(dead_code)]
        /// The resolver-owned cache leaf directory for this module.
        cache_leaf: PathBuf,
    },
}

impl MaterializedRoot {
    /// Returns the module root regardless of ownership.
    pub fn module_root(&self) -> &ModuleRoot {
        match self {
            Self::Local(root) => root,
            Self::Cached { module_root, .. } => module_root,
        }
    }
}
