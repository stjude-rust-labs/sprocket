//! Manifest-driven module content exclusions.

use std::io;
use std::path::Path;
use std::path::PathBuf;

use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

use crate::manifest::Manifest;
use crate::manifest::ManifestError;
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

impl Default for ExclusionSet {
    fn default() -> Self {
        Self::empty()
    }
}

impl ExclusionSet {
    /// Creates an empty exclusion set that excludes no manifest paths.
    pub fn empty() -> Self {
        Self {
            set: globset::GlobSet::empty(),
        }
    }

    /// Compiles manifest `exclude` patterns.
    pub fn new(patterns: &[RelativePath]) -> Result<Self, ExcludePatternError> {
        if patterns.is_empty() {
            return Ok(Self::empty());
        }

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

    /// Returns whether `path` is excluded by manifest `exclude` patterns.
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

    /// Returns whether `path` is outside logical module content.
    ///
    /// A module-relative path is outside logical module content if it is a
    /// spec-excluded metadata path (`module.sig`, `module-lock.json`, `.git`,
    /// or `.sprocket`) or matches a manifest `exclude` pattern.
    pub(crate) fn excludes_content_path(&self, path: &Path) -> bool {
        crate::hash::path_is_excluded_from_hash(path) || self.is_excluded(path)
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

/// An error encountered while loading exclusions from a module root.
#[derive(Debug, Error)]
pub(crate) enum LoadExclusionsError {
    /// An I/O error reading the manifest path.
    #[error("i/o error at `{path}`")]
    Io {
        /// The manifest path.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// The root manifest path is a symbolic link.
    #[error("symbolic link `{0}` is not allowed in a module")]
    Symlink(String),
    /// The root manifest could not be parsed.
    #[error("invalid module manifest at `{path}`")]
    Manifest {
        /// The root manifest path.
        path: PathBuf,
        /// The manifest parse error.
        #[source]
        source: ManifestError,
    },
    /// A manifest `exclude` pattern is not a valid glob.
    #[error(transparent)]
    InvalidExclude(#[from] ExcludePatternError),
}

/// Loads the root manifest and compiles its content exclusions when a regular
/// `module.json` file is present.
pub(crate) fn load_from_root(
    root: &Path,
) -> Result<(Option<Manifest>, ExclusionSet), LoadExclusionsError> {
    let path = root.join(crate::MANIFEST_FILENAME);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Ok((None, ExclusionSet::empty()));
        }
        Err(source) => return Err(LoadExclusionsError::Io { path, source }),
    };
    if metadata.file_type().is_symlink() {
        return Err(LoadExclusionsError::Symlink(path.display().to_string()));
    }
    if !metadata.is_file() {
        return Ok((None, ExclusionSet::empty()));
    }
    let bytes = std::fs::read(&path).map_err(|source| LoadExclusionsError::Io {
        path: path.clone(),
        source,
    })?;
    let manifest = Manifest::parse(&bytes).map_err(|source| LoadExclusionsError::Manifest {
        path: path.clone(),
        source,
    })?;
    let exclusions = ExclusionSet::new(&manifest.exclude)?;
    Ok((Some(manifest), exclusions))
}

#[cfg(test)]
mod tests {
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
    fn excludes_content_path_combines_metadata_and_globs() {
        let exclusions = ExclusionSet::new(&[rel("private/**")]).unwrap();
        assert!(exclusions.excludes_content_path(Path::new(crate::LOCKFILE_FILENAME)));
        assert!(exclusions.excludes_content_path(Path::new(crate::SIGNATURE_FILENAME)));
        assert!(exclusions.excludes_content_path(Path::new(".git/config")));
        assert!(exclusions.excludes_content_path(Path::new("private/helper.wdl")));
        assert!(!exclusions.excludes_content_path(Path::new("public.wdl")));
        assert!(!exclusions.excludes_content_path(Path::new(crate::MANIFEST_FILENAME)));
    }
}
