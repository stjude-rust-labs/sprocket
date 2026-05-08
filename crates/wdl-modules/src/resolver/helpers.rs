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
