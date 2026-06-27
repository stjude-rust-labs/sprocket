//! A [`Module`] pairs a parsed manifest with the directory it loaded
//! from and the path through the lockfile that locates its dependency
//! entries.
//!
//! Carrying these together fixes two ambiguities that a bare
//! [`Manifest`] cannot resolve on its own:
//!
//! 1. A relative `LocalPath` in `module.json` is relative to that file, not to
//!    the process's current working directory. [`Module::root`] is the
//!    directory to rebase against.
//! 2. Lockfile lookups must be scoped to the consumer's branch of the nested
//!    `dependencies` tree, not searched globally. The
//!    [`Module::lockfile_scope`] field records the chain of dependency names
//!    from the top-level consumer down to this module.
//!
//! See [`crate::lockfile`] for how the scope is consumed by lookups.
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use path_clean::PathClean;

use crate::Manifest;
use crate::dependency::DependencyName;
use crate::manifest::ManifestError;

/// Returns `true` when `dir` contains a `module.json` file at its root
/// (i.e., `dir` is the on-disk location of a WDL module).
///
/// Directory walkers use this to recognize module boundaries: a
/// directory that owns a `module.json` is the entrypoint of a separate
/// module and should not be analyzed as part of an ancestor module.
///
/// # Examples
///
/// ```
/// use wdl_modules::module::is_module_root;
///
/// let dir = tempfile::tempdir().unwrap();
///
/// // A directory without `module.json` is not a module root.
/// assert!(!is_module_root(dir.path()));
///
/// // Creating `module.json` inside it makes it a module root.
/// std::fs::write(dir.path().join("module.json"), b"{}").unwrap();
/// assert!(is_module_root(dir.path()));
/// ```
pub fn is_module_root(dir: &Path) -> bool {
    dir.join(crate::MANIFEST_FILENAME).is_file()
}

/// A cheap identity for a [`Module`] that combines its root directory with its
/// lockfile scope.
///
/// Two modules with the same root and scope resolve dependencies identically,
/// so this is a stable key for deduplicating and caching resolution work
/// without cloning a module's manifest.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ModuleId {
    /// The directory containing the module's `module.json` file.
    pub root: PathBuf,
    /// The chain of dependency names from the top-level consumer to the module.
    pub scope: Vec<DependencyName>,
}

/// A WDL module — its parsed [`Manifest`], the directory on disk that
/// holds the `module.json` file, and the lockfile scope that locates
/// the module's entry within a top-level lockfile.
#[derive(Clone, Debug)]
pub struct Module {
    /// The parsed manifest.
    pub manifest: Arc<Manifest>,
    /// The directory containing the `module.json` file.
    pub root: PathBuf,
    /// The chain of dependency names from the top-level consumer to
    /// this module. Empty for the top-level consumer itself. A
    /// dependency `coffeeshop` brought in by `cafe_menu` has scope
    /// `[cafe_menu]`.
    pub lockfile_scope: Vec<DependencyName>,
}

impl Module {
    /// Builds a top-level [`Module`] from a manifest and its root
    /// directory. The lockfile scope is empty.
    pub fn new(manifest: Arc<Manifest>, root: PathBuf) -> Self {
        Self {
            manifest,
            root,
            lockfile_scope: Vec::new(),
        }
    }

    /// Returns this module's identity, combining its root directory and
    /// lockfile scope.
    pub fn id(&self) -> ModuleId {
        ModuleId {
            root: self.root.clone(),
            scope: self.lockfile_scope.clone(),
        }
    }

    /// Reads `module.json` from `path` and constructs a top-level
    /// [`Module`] with `path` as the root.
    pub fn load_from_path(path: &Path) -> Result<Self, ManifestError> {
        let manifest_path = path.join(crate::MANIFEST_FILENAME);
        let bytes = std::fs::read(&manifest_path).map_err(|source| ManifestError::Io {
            path: manifest_path,
            source,
        })?;
        let manifest = Arc::new(Manifest::parse(&bytes)?);
        Ok(Self::new(manifest, path.to_path_buf()))
    }

    /// Returns `path` joined to [`root`](Self::root) when `path` is
    /// relative, or `path` itself when it is already absolute.
    pub fn resolve_local_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.clean()
        } else {
            self.root.join(path).clean()
        }
    }

    /// Returns a child [`Module`] in the lockfile scope below this one,
    /// extending the scope by `name`.
    pub fn child(&self, name: DependencyName, manifest: Arc<Manifest>, root: PathBuf) -> Self {
        let mut lockfile_scope = self.lockfile_scope.clone();
        lockfile_scope.push(name);
        Self {
            manifest,
            root,
            lockfile_scope,
        }
    }
}
