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
use unicode_normalization::UnicodeNormalization;

use crate::hash::NON_MODULE_CONTENT;
use crate::relative_path::RelativePath;

/// A compiled set of manifest `exclude` patterns.
///
/// Patterns use gitignore-style glob semantics: `*` does not cross path
/// separators, `**` does, and matching a directory also excludes its subtree.
/// The root manifest is always module content, even when a pattern matches it.
#[derive(Clone, Debug)]
pub struct ExclusionSet {
    /// The compiled glob matcher.
    set: globset::GlobSet,
}

impl ExclusionSet {
    /// Compiles manifest `exclude` patterns.
    pub fn new(patterns: &[RelativePath]) -> Result<Self, ExcludePatternError> {
        let mut builder = globset::GlobSetBuilder::new();
        for pattern in patterns {
            let source = pattern.as_str();
            let normalized = source.nfc().collect::<String>();
            let compile = |glob: &str| {
                globset::GlobBuilder::new(glob)
                    .literal_separator(true)
                    .build()
                    .map_err(|source_error| ExcludePatternError {
                        pattern: source.to_string(),
                        source: source_error,
                    })
            };
            builder.add(compile(&normalized)?);
            builder.add(compile(&format!(
                "{}/**",
                normalized.trim_end_matches('/')
            ))?);
        }

        // `GlobSetBuilder::build` only consolidates globs already validated by
        // `GlobBuilder::build` above.
        Ok(Self {
            set: builder
                .build()
                .expect("compiled globs should form a glob set"),
        })
    }

    /// Returns whether `path` is excluded from logical module content.
    pub fn is_excluded(&self, path: &Path) -> bool {
        if path == Path::new(crate::MANIFEST_FILENAME) {
            return false;
        }
        let Some(path) = path.to_str() else {
            return false;
        };
        let normalized = path.replace('\\', "/").nfc().collect::<String>();
        self.set.is_match(normalized)
    }
}

/// An invalid manifest `exclude` glob.
#[derive(Debug, Error)]
#[error("invalid `exclude` pattern `{pattern}`")]
pub struct ExcludePatternError {
    /// The invalid pattern.
    pub pattern: String,
    /// The underlying glob parser error.
    #[source]
    pub source: globset::Error,
}

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
/// - Entries named `.git` or `.sprocket` are skipped.
/// - Any symbolic link is rejected with [`ModuleWalkError::Symlink`].
/// - Only regular files are visited.
pub fn walk_module_tree<E>(
    root: &Path,
    visitor: &mut dyn FnMut(&Path, u64) -> Result<(), E>,
) -> Result<TreeStats, WalkError<E>> {
    let mut stats = TreeStats::default();
    walk_recursive(root, visitor, &mut stats)?;
    Ok(stats)
}

/// Walks logical module content, omitting files matched by `exclusions`.
///
/// The underlying physical tree is still traversed in full, so excluded paths
/// cannot conceal symbolic links. Statistics count only visited module-content
/// files; callers enforcing physical materialization limits should use
/// [`walk_module_tree`] directly.
pub fn walk_module_content_tree<E>(
    root: &Path,
    exclusions: &ExclusionSet,
    visitor: &mut dyn FnMut(&Path, u64) -> Result<(), E>,
) -> Result<TreeStats, WalkError<E>> {
    let mut stats = TreeStats::default();
    let mut visit_content = |path: &Path, size: u64| {
        // SAFETY: `walk_module_tree` only yields paths beneath `root`.
        let relative = path.strip_prefix(root).unwrap();
        if exclusions.is_excluded(relative) {
            return Ok(());
        }
        stats.files += 1;
        stats.bytes = stats.bytes.saturating_add(size);
        visitor(path, size)
    };

    // Discard the physical-tree statistics: this function reports logical
    // module-content statistics accumulated by `visit_content`.
    walk_module_tree(root, &mut visit_content)?;
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

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::convert::Infallible;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    #[cfg(unix)]
    use tempfile::tempdir;

    use super::*;

    fn rel(value: &str) -> RelativePath {
        value.parse().unwrap()
    }

    #[test]
    fn exclusions_use_gitignore_style_globs_and_keep_manifest() {
        let exclusions = ExclusionSet::new(&[
            rel("internal"),
            rel("scratch/*.wdl"),
            rel("secret/**"),
            rel("module.json"),
        ])
        .unwrap();

        assert!(exclusions.is_excluded(Path::new("internal/private.wdl")));
        assert!(exclusions.is_excluded(Path::new("internal/deep/nested.wdl")));
        assert!(exclusions.is_excluded(Path::new("scratch/tmp.wdl")));
        assert!(!exclusions.is_excluded(Path::new("scratch/sub/tmp.wdl")));
        assert!(exclusions.is_excluded(Path::new("secret/a/b/c.wdl")));
        assert!(!exclusions.is_excluded(Path::new("public.wdl")));
        assert!(!exclusions.is_excluded(Path::new(crate::MANIFEST_FILENAME)));
    }

    #[test]
    fn rejects_invalid_exclusion_glob() {
        let error = ExclusionSet::new(&[rel("[")]).unwrap_err();
        assert_eq!(error.pattern, "[");
    }

    #[test]
    fn exclusions_match_after_unicode_normalization() {
        let decomposed = "cafe\u{301}.wdl";
        let exclusions = ExclusionSet::new(&[rel(decomposed)]).unwrap();
        assert!(exclusions.is_excluded(Path::new("caf\u{e9}.wdl")));
    }

    #[test]
    #[cfg(unix)]
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
}
