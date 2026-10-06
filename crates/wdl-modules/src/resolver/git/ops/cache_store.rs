//! Cache-root and cache-leaf storage operations.

use std::fs::File;
use std::fs::TryLockError;
use std::path::Path;
use std::path::PathBuf;

use super::GitError;

/// Extension appended to `.<leaf_name>` in the parent directory to
/// form the per-leaf sparse-checkout metadata path.
const SPARSE_META_EXT: &str = ".sparse.json";

/// Extension appended to `.<leaf_name>` in the parent directory to
/// form the per-leaf advisory lock path.
const LOCK_EXT: &str = ".lock";

/// Filename that marks a directory as an owned WDL module cache.
pub(crate) const CACHE_MARKER_FILENAME: &str = ".sprocket-wdl-module-cache";

/// Versioned contents of the WDL module cache ownership marker.
const CACHE_MARKER_CONTENT: &[u8] = b"sprocket-wdl-module-cache-v1\n";
/// Root and leaf paths participating in one cache operation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CacheLocation<'a> {
    /// Owned WDL module cache root.
    pub root: &'a Path,
    /// Commit-specific cache leaf.
    pub leaf: &'a Path,
}

/// Initializes an empty cache root or validates its ownership marker.
pub(crate) fn initialize_cache_root(root: &Path) -> Result<(), GitError> {
    let _lock = lock_cache_root_unchecked(root)?;
    initialize_cache_root_locked(root)
}

/// Acquires a shared cache-root lock for a materialization or scoped cleanup.
pub(crate) fn lock_cache_root_shared(root: &Path) -> Result<File, GitError> {
    initialize_cache_root(root)?;
    let lock_path = cache_root_lock_path(root)?;
    let file = open_cache_root_lock(root)?;
    file.lock_shared().map_err(|source| GitError::Io {
        path: lock_path,
        source,
    })?;
    validate_cache_marker(root)?;
    Ok(file)
}

/// Acquires an exclusive cache-root lock for a full cleanup.
pub(crate) fn lock_cache_root_exclusive(root: &Path) -> Result<File, GitError> {
    initialize_cache_root(root)?;
    let file = lock_cache_root_unchecked(root)?;
    validate_cache_marker(root)?;
    Ok(file)
}

/// Removes one cache leaf while holding its advisory lock.
pub(crate) fn remove_cache_leaf(leaf: &Path) -> Result<bool, GitError> {
    let _lock = lock_cache_leaf(leaf)?;
    if !leaf.exists() {
        return Ok(false);
    }
    std::fs::remove_dir_all(leaf).map_err(|source| GitError::Io {
        path: leaf.to_path_buf(),
        source,
    })?;
    let sparse_meta = sparse_meta_path(leaf);
    match std::fs::remove_file(&sparse_meta) {
        Ok(()) => {}
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(GitError::Io {
                path: sparse_meta,
                source,
            });
        }
    }
    Ok(true)
}

/// Removes every owned cache entry while holding the root lock exclusively.
pub(crate) fn remove_cache_root(root: &Path) -> Result<(usize, u64), GitError> {
    let _lock = lock_cache_root_exclusive(root)?;
    let stats = cache_tree_stats(root)?;
    std::fs::remove_dir_all(root).map_err(|source| GitError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    Ok(stats)
}

/// Removes selected leaves while excluding concurrent full-cache cleanup.
pub(crate) fn remove_cache_leaves(
    root: &Path,
    leaves: &[PathBuf],
) -> Result<(usize, u64), GitError> {
    let _root_lock = lock_cache_root_shared(root)?;
    let mut modules = 0usize;
    let mut bytes = 0u64;
    for leaf in leaves {
        if !leaf.starts_with(root) {
            return Err(GitError::UnsafeCacheRoot {
                path: leaf.clone(),
                reason: "a cache leaf escaped its cache root",
            });
        }
        if !leaf.exists() {
            continue;
        }
        bytes = bytes.saturating_add(cache_tree_stats(leaf)?.1);
        modules += usize::from(remove_cache_leaf(leaf)?);
    }
    Ok((modules, bytes))
}

/// Opens and exclusively locks the cache-root advisory lock.
fn lock_cache_root_unchecked(root: &Path) -> Result<File, GitError> {
    let path = cache_root_lock_path(root)?;
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|source| GitError::Io {
            path: path.clone(),
            source,
        })?;
    file.lock()
        .map_err(|source| GitError::Io { path, source })?;
    Ok(file)
}

/// Opens the cache-root advisory lock without acquiring it.
fn open_cache_root_lock(root: &Path) -> Result<File, GitError> {
    let path = cache_root_lock_path(root)?;
    File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|source| GitError::Io { path, source })
}

/// Returns the advisory lock path placed next to a cache root.
fn cache_root_lock_path(root: &Path) -> Result<PathBuf, GitError> {
    let Some(name) = root.file_name().filter(|name| !name.is_empty()) else {
        return Err(GitError::UnsafeCacheRoot {
            path: root.to_path_buf(),
            reason: "the path has no final directory component",
        });
    };
    let parent = root
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|source| GitError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    Ok(parent.join(format!(".{}.lock", name.to_string_lossy())))
}

/// Creates an ownership marker only when the cache root is empty.
fn initialize_cache_root_locked(root: &Path) -> Result<(), GitError> {
    if root.exists() {
        let metadata = std::fs::symlink_metadata(root).map_err(|source| GitError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(GitError::UnsafeCacheRoot {
                path: root.to_path_buf(),
                reason: "the path is not a regular directory",
            });
        }
        let marker = root.join(CACHE_MARKER_FILENAME);
        if marker.exists() {
            return validate_cache_marker(root);
        }
        let mut entries = std::fs::read_dir(root).map_err(|source| GitError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        if entries.next().is_some() {
            return Err(GitError::UnsafeCacheRoot {
                path: root.to_path_buf(),
                reason: "the directory is non-empty and has no ownership marker",
            });
        }
    } else {
        std::fs::create_dir_all(root).map_err(|source| GitError::Io {
            path: root.to_path_buf(),
            source,
        })?;
    }

    let marker = root.join(CACHE_MARKER_FILENAME);
    std::fs::write(&marker, CACHE_MARKER_CONTENT).map_err(|source| GitError::Io {
        path: marker,
        source,
    })
}

/// Validates the cache-root ownership marker without following symlinks.
fn validate_cache_marker(root: &Path) -> Result<(), GitError> {
    let marker = root.join(CACHE_MARKER_FILENAME);
    let metadata = std::fs::symlink_metadata(&marker).map_err(|source| GitError::Io {
        path: marker.clone(),
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(GitError::UnsafeCacheRoot {
            path: root.to_path_buf(),
            reason: "the ownership marker is not a regular file",
        });
    }
    let contents = std::fs::read(&marker).map_err(|source| GitError::Io {
        path: marker,
        source,
    })?;
    if contents != CACHE_MARKER_CONTENT {
        return Err(GitError::UnsafeCacheRoot {
            path: root.to_path_buf(),
            reason: "the ownership marker has unexpected contents",
        });
    }
    Ok(())
}

/// Counts commit directories and regular-file bytes without following symlinks.
fn cache_tree_stats(path: &Path) -> Result<(usize, u64), GitError> {
    let mut modules = 0usize;
    let mut bytes = 0u64;
    let entries = std::fs::read_dir(path).map_err(|source| GitError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| GitError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let child = entry.path();
        let metadata = std::fs::symlink_metadata(&child).map_err(|source| GitError::Io {
            path: child.clone(),
            source,
        })?;
        if metadata.is_dir() {
            if is_commit_directory(&child) {
                modules += 1;
            }
            let (nested_modules, nested_bytes) = cache_tree_stats(&child)?;
            modules = modules.saturating_add(nested_modules);
            bytes = bytes.saturating_add(nested_bytes);
        } else if metadata.is_file() {
            bytes = bytes.saturating_add(metadata.len());
        }
    }
    Ok((modules, bytes))
}

/// Returns whether a path names a full Git commit cache directory.
fn is_commit_directory(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.len() == 40
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
}

/// Removes a cache leaf and its sparse metadata while its lock is held.
pub(super) fn clear_cache_leaf(leaf: &Path) -> Result<(), GitError> {
    if leaf.exists() {
        std::fs::remove_dir_all(leaf).map_err(|source| GitError::Io {
            path: leaf.to_path_buf(),
            source,
        })?;
    }
    let sparse_meta = sparse_meta_path(leaf);
    match std::fs::remove_file(&sparse_meta) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(GitError::Io {
            path: sparse_meta,
            source,
        }),
    }
}

/// Acquires the advisory lock guarding one cache leaf, creating the leaf's
/// parent directory and lock file when they are absent. Logs before blocking
/// on a lock another process already holds.
pub(super) fn lock_cache_leaf(leaf: &Path) -> Result<File, GitError> {
    // SAFETY: cache leaves are always `<parent>/<commit_sha>`, so `parent`
    // and `file_name` are always present.
    let parent = leaf.parent().unwrap();
    std::fs::create_dir_all(parent).map_err(|source| GitError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    // SAFETY: cache leaves are always `<parent>/<commit_sha>`.
    let name = leaf.file_name().unwrap().to_string_lossy();
    let lock_path = parent.join(format!(".{name}{LOCK_EXT}"));
    let file = File::create(&lock_path).map_err(|source| GitError::Io {
        path: lock_path.clone(),
        source,
    })?;
    // Try the lock first so we can tell the user why `sprocket` appears to
    // hang when another process already holds it before blocking on it.
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            tracing::info!(
                "waiting to acquire exclusive lock on Git cache leaf `{leaf}`",
                leaf = leaf.display(),
            );
            file.lock().map_err(|source| GitError::Io {
                path: lock_path.clone(),
                source,
            })?;
        }
        Err(TryLockError::Error(source)) => {
            return Err(GitError::Io {
                path: lock_path,
                source,
            });
        }
    }
    Ok(file)
}

/// Returns the path to the sparse-checkout metadata file for a cache
/// leaf. The file is placed in the leaf's parent directory as
/// `.<leaf_name>.sparse.json`.
pub(super) fn sparse_meta_path(leaf: &Path) -> PathBuf {
    let name = leaf
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    leaf.with_file_name(format!(".{name}{SPARSE_META_EXT}"))
}

/// Removes a worktree path without following symbolic links.
pub(super) fn remove_worktree_path(path: &Path) -> Result<(), GitError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(GitError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let result = if metadata.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    result.map_err(|source| GitError::Io {
        path: path.to_path_buf(),
        source,
    })
}
