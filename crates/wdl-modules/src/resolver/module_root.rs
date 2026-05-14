//! Ownership-aware module root types.
//!
//! A resolved dependency can come from two places: a Git cache leaf
//! (sparse checkout under `cache_root`) or a local filesystem path.
//! The resolver needs to know which case it is dealing with so it can
//! produce diagnostic messages that reference the cache leaf path on
//! verification failure, while still exposing a uniform `&Path` to
//! the module's content for hashing, signing, and tree validation.
//!
//! [`ModuleRoot`] wraps the content path. [`MaterializedRoot`] pairs
//! it with optional cache metadata so callers can distinguish cached
//! from local modules without threading that context separately.

use std::path::Path;
use std::path::PathBuf;

use crate::DependencyName;
use crate::hash::NON_MODULE_CONTENT;
use crate::resolver::error::ResolverError;

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

/// Resolves a relative content path under `root`, enforcing the same
/// metadata exclusions and containment rules used by
/// [`module_walk`](crate::module_walk). Returns the canonical absolute
/// path on success.
pub(crate) fn resolve_content_file(
    root: &ModuleRoot,
    rel: &Path,
    dep: &DependencyName,
) -> Result<PathBuf, ResolverError> {
    if rel.components().any(|c| {
        let name = c.as_os_str().to_str().unwrap_or("");
        NON_MODULE_CONTENT.contains(&name)
    }) {
        return Err(ResolverError::Hash(
            crate::HashError::SymlinkTargetsMetadata(rel.display().to_string()),
        ));
    }

    let candidate = root.as_ref().join(rel);
    if !candidate.exists() {
        return Err(ResolverError::Io {
            path: candidate,
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "materialized content file does not exist",
            ),
        });
    }

    let meta = std::fs::symlink_metadata(&candidate).map_err(|source| ResolverError::Io {
        path: candidate.clone(),
        source,
    })?;

    if meta.file_type().is_symlink() {
        let canonical_root =
            std::fs::canonicalize(root.as_ref()).map_err(|source| ResolverError::Io {
                path: root.as_ref().to_path_buf(),
                source,
            })?;
        let target = std::fs::canonicalize(&candidate).map_err(|source| ResolverError::Io {
            path: candidate.clone(),
            source,
        })?;

        if !target.starts_with(&canonical_root) {
            return Err(ResolverError::MaterializedSymlinkEscape {
                dep: dep.manifest().to_string(),
                path: candidate,
            });
        }

        if let Ok(target_rel) = target.strip_prefix(&canonical_root)
            && target_rel.components().any(|c| {
                let name = c.as_os_str().to_str().unwrap_or("");
                NON_MODULE_CONTENT.contains(&name)
            })
        {
            return Err(ResolverError::Hash(
                crate::HashError::SymlinkTargetsMetadata(rel.display().to_string()),
            ));
        }

        Ok(target)
    } else {
        candidate
            .canonicalize()
            .map_err(|source| ResolverError::Io {
                path: candidate,
                source,
            })
    }
}
