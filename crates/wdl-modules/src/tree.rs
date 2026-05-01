//! Validation rules for the module file tree.
//!
//! These checks complement [`Hasher`](crate::Hasher) by validating
//! structural rules that span the whole tree rather than any single path.
//! Per-path validity is already a guarantee of [`RelativePath`]; this
//! module only enforces the cross-path rules: reserved filename placement
//! and uniqueness under Unicode Normalization Form C.

use std::collections::HashSet;

use thiserror::Error;

use crate::LOCKFILE_FILENAME;
use crate::MANIFEST_FILENAME;
use crate::RelativePath;
use crate::SIGNATURE_FILENAME;

/// An error reported by [`validate_tree`].
#[derive(Debug, Error)]
pub enum TreeError {
    /// A reserved filename was found at a non-root location. The names
    /// `module.json`, `module-lock.json`, and `module.sig` may appear only
    /// at the module root.
    #[error("reserved filename `{name}` is only permitted at the module root; found at `{path}`")]
    ReservedFilename {
        /// The reserved filename that was misplaced.
        name: &'static str,
        /// The path under which the reserved filename appeared.
        path: String,
    },

    /// Two distinct paths normalize to the same Unicode Normalization Form
    /// C (NFC) form. The spec requires the module's set of relative paths
    /// to be unique under NFC.
    #[error("paths collapse to the same NFC form `{nfc}`")]
    AmbiguousPath {
        /// The shared NFC form.
        nfc: String,
    },
}

/// Validates the cross-path rules of a module file tree.
///
/// Two checks are performed.
///
/// - The reserved filenames `module.json`, `module-lock.json`, and `module.sig`
///   may appear only at the module root (i.e. as a single path component, not
///   nested in any subdirectory).
/// - No two distinct paths may collapse to the same Unicode Normalization Form
///   C (NFC).
pub fn validate_tree<I>(paths: I) -> Result<(), TreeError>
where
    I: IntoIterator<Item = RelativePath>,
{
    let mut seen: HashSet<RelativePath> = HashSet::new();
    for path in paths {
        if let Some((_, basename)) = path.as_str().rsplit_once('/')
            && let Some(reserved) = [MANIFEST_FILENAME, LOCKFILE_FILENAME, SIGNATURE_FILENAME]
                .into_iter()
                .find(|r| *r == basename)
        {
            return Err(TreeError::ReservedFilename {
                name: reserved,
                path: path.into_inner(),
            });
        }

        if !seen.insert(path.clone()) {
            return Err(TreeError::AmbiguousPath {
                nfc: path.into_inner(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn rel(s: &str) -> RelativePath {
        RelativePath::from_str(s).unwrap()
    }

    #[test]
    fn accepts_root_reserved_filenames() {
        validate_tree([
            rel(MANIFEST_FILENAME),
            rel(LOCKFILE_FILENAME),
            rel(SIGNATURE_FILENAME),
            rel("index.wdl"),
        ])
        .unwrap();
    }

    #[test]
    fn rejects_nested_manifest() {
        let err = validate_tree([rel("src/module.json")]).unwrap_err();
        assert!(matches!(
            err,
            TreeError::ReservedFilename {
                name: MANIFEST_FILENAME,
                ..
            }
        ));
    }

    #[test]
    fn rejects_nested_lockfile() {
        let err = validate_tree([rel("nested/dir/module-lock.json")]).unwrap_err();
        assert!(matches!(
            err,
            TreeError::ReservedFilename {
                name: LOCKFILE_FILENAME,
                ..
            }
        ));
    }

    #[test]
    fn rejects_nested_signature() {
        let err = validate_tree([rel("sub/module.sig")]).unwrap_err();
        assert!(matches!(
            err,
            TreeError::ReservedFilename {
                name: SIGNATURE_FILENAME,
                ..
            }
        ));
    }

    #[test]
    fn rejects_paths_colliding_under_nfc() {
        let err = validate_tree([rel("caf\u{00E9}.wdl"), rel("cafe\u{0301}.wdl")]).unwrap_err();
        assert!(matches!(err, TreeError::AmbiguousPath { .. }));
    }

    #[test]
    fn accepts_distinct_unicode_paths() {
        validate_tree([rel("alpha.wdl"), rel("beta.wdl"), rel("caf\u{00E9}.wdl")]).unwrap();
    }
}
