//! Safe module-content tree walk shared by hashing, verification,
//! resource-limit checking, and materialization.
//!
//! One traversal implementation enforces all module-content rules.
//! The canonical module root is carried through all recursion so
//! containment checks are never relative to the current directory.
//! Directory symlinks are rejected to prevent cycles. File symlinks
//! are allowed when they resolve inside the module root and do not
//! target non-module content.

use std::path::Path;

use crate::hash::HashError;
use crate::hash::NON_MODULE_CONTENT;

/// Statistics collected during a tree walk.
#[derive(Clone, Debug, Default)]
pub struct TreeStats {
    /// Total regular files encountered.
    pub files: usize,
    /// Total bytes of regular files.
    pub bytes: u64,
}

/// Walks every regular file under `root`, enforcing containment and
/// metadata exclusion. Calls `visitor` for each file with its path
/// and size. Returns aggregate statistics.
///
/// Rules enforced:
/// - Entries named `.git` or `.sparse.json` are skipped.
/// - Symlinks whose canonical target is outside the module root are rejected
///   with `HashError::SymlinkEscapesRoot`.
/// - Symlinks targeting non-module content (`.git`, `.sparse.json`) are
///   rejected with `HashError::SymlinkTargetsMetadata`.
/// - Directory symlinks are rejected to prevent cycles.
/// - Only regular files (and file symlinks to valid targets) are visited.
pub fn walk_module_tree<E>(
    root: &Path,
    visitor: &mut dyn FnMut(&Path, u64) -> Result<(), E>,
) -> Result<TreeStats, WalkError<E>> {
    let canonical_root = std::fs::canonicalize(root).map_err(|source| {
        WalkError::Hash(HashError::Io {
            path: root.to_path_buf(),
            source,
        })
    })?;
    let mut stats = TreeStats::default();
    walk_recursive(&canonical_root, root, visitor, &mut stats)?;
    Ok(stats)
}

/// The error type for [`walk_module_tree`]. Wraps both hash-layer
/// errors and visitor errors.
#[derive(Debug)]
pub enum WalkError<E> {
    /// A hash/containment/metadata error.
    Hash(HashError),
    /// An error returned by the visitor callback.
    Visitor(E),
}

impl<E> From<HashError> for WalkError<E> {
    fn from(e: HashError) -> Self {
        Self::Hash(e)
    }
}

/// Recursive directory walker carrying the canonical module root.
fn walk_recursive<E>(
    module_root: &Path,
    dir: &Path,
    visitor: &mut dyn FnMut(&Path, u64) -> Result<(), E>,
    stats: &mut TreeStats,
) -> Result<(), WalkError<E>> {
    let entries = std::fs::read_dir(dir).map_err(|source| {
        WalkError::Hash(HashError::Io {
            path: dir.to_path_buf(),
            source,
        })
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| {
            WalkError::Hash(HashError::Io {
                path: dir.to_path_buf(),
                source,
            })
        })?;
        let name = entry.file_name();
        if NON_MODULE_CONTENT.iter().any(|s| *s == name) {
            continue;
        }
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path).map_err(|source| {
            WalkError::Hash(HashError::Io {
                path: path.to_path_buf(),
                source,
            })
        })?;
        if meta.file_type().is_symlink() {
            handle_symlink(module_root, &path, visitor, stats)?;
            continue;
        }
        if meta.is_dir() {
            walk_recursive(module_root, &path, visitor, stats)?;
        } else if meta.is_file() {
            stats.files += 1;
            stats.bytes = stats.bytes.saturating_add(meta.len());
            visitor(&path, meta.len()).map_err(WalkError::Visitor)?;
        }
    }
    Ok(())
}

/// Validates and processes a symlink entry against containment rules.
fn handle_symlink<E>(
    module_root: &Path,
    path: &Path,
    visitor: &mut dyn FnMut(&Path, u64) -> Result<(), E>,
    stats: &mut TreeStats,
) -> Result<(), WalkError<E>> {
    let target = std::fs::canonicalize(path).map_err(|source| {
        WalkError::Hash(HashError::Io {
            path: path.to_path_buf(),
            source,
        })
    })?;
    if !target.starts_with(module_root) {
        return Err(WalkError::Hash(HashError::SymlinkEscapesRoot(
            path.display().to_string(),
        )));
    }
    if let Ok(rel) = target.strip_prefix(module_root) {
        let targets_metadata = rel.components().any(|component| {
            let name = component.as_os_str().to_str().unwrap_or("");
            NON_MODULE_CONTENT.contains(&name)
        });
        if targets_metadata {
            return Err(WalkError::Hash(HashError::SymlinkTargetsMetadata(
                path.display().to_string(),
            )));
        }
    }
    let target_meta = std::fs::metadata(path).map_err(|source| {
        WalkError::Hash(HashError::Io {
            path: path.to_path_buf(),
            source,
        })
    })?;
    if target_meta.is_dir() {
        return Err(WalkError::Hash(HashError::SymlinkEscapesRoot(format!(
            "{} (directory symlinks are not supported)",
            path.display()
        ))));
    }
    if target_meta.is_file() {
        stats.files += 1;
        stats.bytes = stats.bytes.saturating_add(target_meta.len());
        visitor(path, target_meta.len()).map_err(WalkError::Visitor)?;
    }
    Ok(())
}
