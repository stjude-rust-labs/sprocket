//! Small utility functions used across the resolver layer.

use std::path::Path;

use semver::Version;

use crate::Manifest;
use crate::DependencySource;
use crate::ResolvedSource;
use crate::resolver::error::ResolverError;

/// Compiles a manifest's `exclude` patterns into a [`globset::GlobSet`].
pub(crate) fn exclude_set(
    patterns: &[crate::RelativePath],
) -> Result<globset::GlobSet, ResolverError> {
    if patterns.is_empty() {
        return Ok(globset::GlobSet::empty());
    }
    let mut builder = globset::GlobSetBuilder::new();
    for p in patterns {
        let s: &str = p.as_ref();
        let glob = globset::Glob::new(s).map_err(|source| ResolverError::InvalidExclude {
            pattern: s.to_string(),
            source,
        })?;
        builder.add(glob);
    }
    // SAFETY: `GlobSetBuilder::build` only consolidates already-compiled
    // globs; `Glob::new` above is the validating step, so by the time
    // we reach this call there is nothing left for `build` to reject.
    Ok(builder.build().unwrap())
}

/// Returns `Err(TagManifestMismatch)` when a Git tag's selected
/// semver `expected` does not equal the manifest's `declared` version.
pub(crate) fn check_tag_manifest_match(
    path_prefix: Option<&str>,
    expected: Option<&Version>,
    declared: &Version,
) -> Result<(), ResolverError> {
    if let Some(exp) = expected
        && exp != declared
    {
        let tag = crate::resolver::versions::VersionTag::new(
            path_prefix.map(str::to_string),
            exp.clone(),
        )
        .to_string();
        return Err(ResolverError::TagManifestMismatch {
            tag,
            declared: declared.clone(),
        });
    }
    Ok(())
}

/// Returns `true` if `child` is a local-path source declared by a
/// non-local parent.
pub(crate) fn is_transitive_local_disallowed(
    parent: Option<&ResolvedSource>,
    child: &DependencySource,
) -> bool {
    matches!(child, DependencySource::LocalPath { .. })
        && matches!(parent, Some(ResolvedSource::Git { .. }))
}

/// Reads and parses `module.json` from `dir`.
pub(crate) fn read_manifest(dir: &Path) -> Result<Manifest, ResolverError> {
    let path = dir.join(crate::MANIFEST_FILENAME);
    let bytes = std::fs::read(&path).map_err(|source| ResolverError::Io {
        path: path.clone(),
        source,
    })?;
    Manifest::parse(&bytes).map_err(ResolverError::from)
}

#[cfg(test)]
mod tests {
    use semver::Version;

    use super::*;

    #[test]
    fn tag_manifest_mismatch_errors_on_disagreement() {
        let v_expected = Version::parse("2.0.0").unwrap();
        let v_declared = Version::parse("1.0.0").unwrap();
        let err = check_tag_manifest_match(None, Some(&v_expected), &v_declared).unwrap_err();
        let ResolverError::TagManifestMismatch { tag, declared } = err else {
            panic!("got: {err:?}");
        };
        assert_eq!(tag, "v2.0.0");
        assert_eq!(declared, v_declared);
    }

    #[test]
    fn tag_manifest_mismatch_ok_when_agree() {
        let v = Version::parse("1.2.3").unwrap();
        check_tag_manifest_match(Some("csvkit"), Some(&v), &v).unwrap();
    }

    #[test]
    fn tag_manifest_mismatch_ok_when_no_expected() {
        check_tag_manifest_match(None, None, &Version::parse("0.0.1").unwrap()).unwrap();
    }

    #[test]
    fn local_in_transitive_classifies_correctly() {
        let local = ResolvedSource::Path {
            path: "/tmp/local".into(),
        };
        let git = ResolvedSource::Git {
            git: "https://github.com/x/y".parse().unwrap(),
            commit: "0000000000000000000000000000000000000000".parse().unwrap(),
            path: None,
        };
        let local_dep = DependencySource::LocalPath {
            path: "/tmp/dep".into(),
            extra: Default::default(),
        };
        let git_dep = DependencySource::Git {
            url: "https://github.com/x/y".parse().unwrap(),
            selector: crate::GitSelector::Tag("v1".into()),
            path: None,
            extra: Default::default(),
        };
        assert!(!is_transitive_local_disallowed(Some(&local), &local_dep));
        assert!(is_transitive_local_disallowed(Some(&git), &local_dep));
        assert!(!is_transitive_local_disallowed(None, &local_dep));
        assert!(!is_transitive_local_disallowed(Some(&git), &git_dep));
    }
}
