//! Safe module-content tree walk shared by hashing, verification,
//! resource-limit checking, and materialization.
//!
//! One traversal implementation enforces all module-content rules.
//! Symbolic links are not permitted anywhere in a module tree: any
//! symlink encountered during the walk makes the module invalid, per
//! the module specification.

use std::io;
use std::path::Path;
use std::path::PathBuf;

use thiserror::Error;

use crate::hash::NON_MODULE_CONTENT;

/// An error encountered while walking a module tree.
#[derive(Debug, Error)]
pub enum ModuleWalkError {
    /// A symbolic link was found in the module tree. Symbolic links are
    /// not permitted anywhere in a module.
    #[error("symbolic link `{0}` is not permitted in a module")]
    Symlink(String),

    /// I/O failure during the walk.
    #[error("i/o error at `{path}`")]
    Io {
        /// The path involved.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },
}

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
/// The walk enforces these rules.
///
/// - Entries named `.git` or `.sprocket` are not visited or counted.
/// - Any symbolic link is rejected with [`ModuleWalkError::Symlink`], including
///   one inside a `.sprocket` directory.
/// - Only regular files are visited.
pub fn walk_module_tree<E>(
    root: &Path,
    visitor: &mut dyn FnMut(&Path, u64) -> Result<(), E>,
) -> Result<TreeStats, WalkError<E>> {
    let mut stats = TreeStats::default();
    walk_recursive(root, visitor, &mut stats)?;
    Ok(stats)
}

/// The error type for [`walk_module_tree`]. Wraps both walk-layer
/// errors and visitor errors.
#[derive(Debug)]
pub enum WalkError<E> {
    /// An error encountered by the walker itself.
    Walk(ModuleWalkError),
    /// An error returned by the visitor callback.
    Visitor(E),
}

impl<E> From<ModuleWalkError> for WalkError<E> {
    fn from(e: ModuleWalkError) -> Self {
        Self::Walk(e)
    }
}

/// The name of the per-module Sprocket metadata directory.
const SPROCKET_DIR: &str = ".sprocket";

/// Rejects any symbolic link at or below the directory `dir`, skipping
/// nested `.git` directories.
fn reject_symlinks(dir: &Path) -> Result<(), ModuleWalkError> {
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|source| ModuleWalkError::Io {
            path: dir.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| ModuleWalkError::Io {
                path: dir.clone(),
                source,
            })?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|source| ModuleWalkError::Io {
                path: path.clone(),
                source,
            })?;
            if file_type.is_symlink() {
                return Err(ModuleWalkError::Symlink(path.display().to_string()));
            }
            if file_type.is_dir() && entry.file_name() != ".git" {
                pending.push(path);
            }
        }
    }
    Ok(())
}

/// Recursive directory walker. Rejects any symbolic link encountered.
fn walk_recursive<E>(
    dir: &Path,
    visitor: &mut dyn FnMut(&Path, u64) -> Result<(), E>,
    stats: &mut TreeStats,
) -> Result<(), WalkError<E>> {
    let entries = std::fs::read_dir(dir).map_err(|source| {
        WalkError::Walk(ModuleWalkError::Io {
            path: dir.to_path_buf(),
            source,
        })
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| {
            WalkError::Walk(ModuleWalkError::Io {
                path: dir.to_path_buf(),
                source,
            })
        })?;
        let name = entry.file_name();
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path).map_err(|source| {
            WalkError::Walk(ModuleWalkError::Io {
                path: path.to_path_buf(),
                source,
            })
        })?;
        // Symbolic links are not permitted anywhere in a module tree.
        if meta.file_type().is_symlink() {
            return Err(WalkError::Walk(ModuleWalkError::Symlink(
                path.display().to_string(),
            )));
        }
        if NON_MODULE_CONTENT.iter().any(|s| *s == name) {
            // `.sprocket` is excluded from module content but is still part of
            // a materialized module, so it must not hide a symbolic link.
            if name == SPROCKET_DIR && meta.is_dir() {
                reject_symlinks(&path)?;
            }
            continue;
        }
        if meta.is_dir() {
            walk_recursive(&path, visitor, stats)?;
        } else if meta.is_file() {
            stats.files += 1;
            stats.bytes = stats.bytes.saturating_add(meta.len());
            visitor(&path, meta.len()).map_err(WalkError::Visitor)?;
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::convert::Infallible;
    use std::os::unix::fs::symlink;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn rejects_symlink_in_excluded_directory() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        let outside = tempdir()?;
        symlink(outside.path(), root.path().join(".sprocket"))?;

        let result = walk_module_tree(root.path(), &mut |_, _| -> Result<(), Infallible> {
            Ok(())
        });

        assert!(matches!(
            result,
            Err(WalkError::Walk(ModuleWalkError::Symlink(_)))
        ));
        Ok(())
    }

    #[test]
    fn rejects_symlink_inside_sprocket_directory() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        let outside = tempdir()?;
        std::fs::create_dir_all(root.path().join(".sprocket/nested"))?;
        std::fs::write(root.path().join(".sprocket/nested/file"), b"x")?;
        symlink(outside.path(), root.path().join(".sprocket/nested/link"))?;

        let mut visited = Vec::new();
        let result = walk_module_tree(root.path(), &mut |path, _| -> Result<(), Infallible> {
            visited.push(path.to_path_buf());
            Ok(())
        });

        assert!(matches!(
            result,
            Err(WalkError::Walk(ModuleWalkError::Symlink(path))) if path.ends_with("link")
        ));
        assert!(visited.is_empty());
        Ok(())
    }

    #[test]
    fn sprocket_directory_is_not_module_content() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        std::fs::create_dir_all(root.path().join(".sprocket/nested"))?;
        std::fs::write(root.path().join(".sprocket/nested/file"), b"x")?;
        std::fs::write(root.path().join("index.wdl"), b"version 1.3\n")?;

        let stats = walk_module_tree(root.path(), &mut |_, _| -> Result<(), Infallible> {
            Ok(())
        })
        .map_err(|_| "walk failed")?;

        assert_eq!(stats.files, 1);
        Ok(())
    }
}
