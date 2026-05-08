//! Unified safe tree walk for module roots.
//!
//! One walk abstraction used by hashing, limits, and warnings.
//! Uses `symlink_metadata`, skips `.git` and `.sparse.json`,
//! does not follow directory symlinks.

use std::path::Path;

use crate::hash::NON_MODULE_CONTENT;
use crate::resolver::error::ResolverError;

/// Statistics collected during a tree walk.
#[derive(Clone, Debug, Default)]
pub(crate) struct TreeStats {
    /// Total regular files encountered.
    pub files: usize,
    /// Total bytes of regular files.
    pub bytes: u64,
}

/// Walks every regular file under `root` using `symlink_metadata`.
/// Skips `.git` directories and `.sparse.json`. Does not follow
/// symlinks. Calls `visitor` for each regular file with its path
/// and size. Returns aggregate statistics.
pub(crate) fn walk_module_tree(
    root: &Path,
    visitor: &mut dyn FnMut(&Path, u64) -> Result<(), ResolverError>,
) -> Result<TreeStats, ResolverError> {
    let mut stats = TreeStats::default();
    walk_recursive(root, visitor, &mut stats)?;
    Ok(stats)
}

/// Recursively walks `dir`, collecting stats and invoking `visitor` for each
/// regular file.
fn walk_recursive(
    dir: &Path,
    visitor: &mut dyn FnMut(&Path, u64) -> Result<(), ResolverError>,
    stats: &mut TreeStats,
) -> Result<(), ResolverError> {
    let entries = std::fs::read_dir(dir).map_err(|source| ResolverError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ResolverError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let name = entry.file_name();
        if NON_MODULE_CONTENT.iter().any(|s| *s == name) {
            continue;
        }
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path).map_err(|source| ResolverError::Io {
            path: path.clone(),
            source,
        })?;
        if meta.file_type().is_symlink() {
            let target = std::fs::canonicalize(&path).map_err(|source| ResolverError::Io {
                path: path.clone(),
                source,
            })?;
            let canonical_root =
                std::fs::canonicalize(dir).map_err(|source| ResolverError::Io {
                    path: dir.to_path_buf(),
                    source,
                })?;
            if let Ok(rel) = target.strip_prefix(&canonical_root) {
                let first = rel.components().next();
                if let Some(c) = first {
                    let name = c.as_os_str().to_str().unwrap_or("");
                    if crate::hash::NON_MODULE_CONTENT.contains(&name) {
                        return Err(ResolverError::Hash(
                            crate::HashError::SymlinkTargetsMetadata(path.display().to_string()),
                        ));
                    }
                }
            }
            let target_meta = std::fs::metadata(&path).map_err(|source| ResolverError::Io {
                path: path.clone(),
                source,
            })?;
            if target_meta.is_dir() {
                walk_recursive(&path, visitor, stats)?;
            } else if target_meta.is_file() {
                stats.files += 1;
                stats.bytes = stats.bytes.saturating_add(target_meta.len());
                visitor(&path, target_meta.len())?;
            }
            continue;
        }
        if meta.is_dir() {
            walk_recursive(&path, visitor, stats)?;
        } else if meta.is_file() {
            stats.files += 1;
            stats.bytes = stats.bytes.saturating_add(meta.len());
            visitor(&path, meta.len())?;
        }
    }
    Ok(())
}
