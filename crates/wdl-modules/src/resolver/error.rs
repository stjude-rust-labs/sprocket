//! Top-level error type for the resolver layer.

use std::path::PathBuf;

use semver::Version;
use thiserror::Error;

use crate::ContentHash;
use crate::DependencyName;
use crate::LockfileError;
use crate::ManifestError;
use crate::VerifyingKey;
use crate::VersionRequirement;

/// An error returned by the [`Resolver`](crate::Resolver) trait or any
/// resolver-layer operation.
#[derive(Debug, Error)]
pub enum ResolverError {
    /// The symbolic path's head does not appear in the consumer's
    /// `dependencies` map.
    #[error("`{name}` is not a declared dependency")]
    NotADependency {
        /// The undeclared dependency name.
        name: String,
    },

    /// A required file was not found, or was excluded.
    #[error("{}", missing_file_message(.dep, .path, .kind))]
    MissingFile {
        /// The owning dependency.
        dep: DependencyName,
        /// The relative path that was looked up.
        path: PathBuf,
        /// Which kind of lookup failed.
        kind: MissingFileKind,
    },

    /// A path-prefixed Git tag's `module.json` declares a different
    /// version than the tag itself.
    #[error("tag `{tag}` points to a `module.json` declaring version `{declared}`")]
    TagManifestMismatch {
        /// The tag name (after stripping any path prefix).
        tag: String,
        /// The version declared in the tagged commit's `module.json`.
        declared: Version,
    },

    /// The dependency graph contains a cycle.
    #[error("dependency cycle: {}", format_cycle(.path))]
    Cycle {
        /// The cycle path, in resolution order.
        path: Vec<DependencyName>,
    },

    /// No discovered version satisfies the dependency's version
    /// requirement.
    #[error(
        "no version satisfies `{dep}` requirement `{requirement}` (considered: {})",
        format_versions(.considered)
    )]
    NoSatisfyingVersion {
        /// The dependency name.
        dep: DependencyName,
        /// The unmet version requirement.
        requirement: VersionRequirement,
        /// The versions discovered before filtering by the requirement.
        considered: Vec<Version>,
    },

    /// A cached module's content hash does not match the lockfile's
    /// recorded checksum.
    #[error(
        "cached `{dep}` content hash does not match the lockfile (expected `{expected}`, observed \
         `{observed}`)"
    )]
    ChecksumMismatch {
        /// The owning dependency.
        dep: DependencyName,
        /// The hash recorded in the lockfile.
        expected: ContentHash,
        /// The hash observed in the cache.
        observed: ContentHash,
    },

    /// A cached module's signature key does not match the lockfile's
    /// recorded signer.
    #[error(
        "signer for `{dep}` has changed since the lockfile was written (run `sprocket module \
         trust {dep}` to accept the new key)"
    )]
    SignerKeyMismatch {
        /// The owning dependency.
        dep: DependencyName,
        /// The signer key recorded in the lockfile.
        expected: Box<VerifyingKey>,
        /// The signer key observed in the cache.
        observed: Box<VerifyingKey>,
    },

    /// `require_signed` is enabled and the dependency is unsigned.
    #[error("`{dep}` is unsigned but `require_signed` is enabled")]
    RequireSignedViolation {
        /// The unsigned dependency.
        dep: DependencyName,
    },

    /// A `git2` operation failed.
    #[error("Git operation failed")]
    Git(#[source] git2::Error),

    /// An I/O error.
    #[error("I/O error at `{path}`")]
    Io {
        /// The path involved.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A `Manifest` parse or validation error.
    #[error(transparent)]
    Manifest(#[from] ManifestError),

    /// A `Lockfile` parse or validation error.
    #[error(transparent)]
    Lockfile(#[from] LockfileError),
}

/// Discriminator for [`ResolverError::MissingFile`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingFileKind {
    /// The dependency's entrypoint file (manifest `entrypoint` or default
    /// `index.wdl`) was missing.
    Entrypoint,
    /// The symbolic-import sub-path resolved to a file that does not
    /// exist.
    SubPath,
    /// The path exists but the manifest's `exclude` masks it.
    Excluded,
}

/// Renders the message for a [`ResolverError::MissingFile`].
fn missing_file_message(
    dep: &DependencyName,
    path: &std::path::Path,
    kind: &MissingFileKind,
) -> String {
    let p = path.display();
    match kind {
        MissingFileKind::Entrypoint => {
            format!("`{dep}` declares entrypoint `{p}` but the file does not exist")
        }
        MissingFileKind::SubPath => format!("`{dep}/{p}` not found"),
        MissingFileKind::Excluded => format!("`{dep}/{p}` is excluded by the module manifest"),
    }
}

/// Renders a cycle path as a chain of arrows for error display.
fn format_cycle(path: &[DependencyName]) -> String {
    path.iter()
        .map(DependencyName::inner)
        .collect::<Vec<_>>()
        .join(" → ")
}

/// Renders a list of versions for error display, or `<none>` when empty.
fn format_versions(versions: &[Version]) -> String {
    if versions.is_empty() {
        return "<none>".to_string();
    }
    versions
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dep() -> DependencyName {
        DependencyName::try_from("foo".to_string()).unwrap()
    }

    #[test]
    fn missing_file_kind_renders_distinctly() {
        let entry = ResolverError::MissingFile {
            dep: dep(),
            path: "index.wdl".into(),
            kind: MissingFileKind::Entrypoint,
        };
        let sub = ResolverError::MissingFile {
            dep: dep(),
            path: "missing.wdl".into(),
            kind: MissingFileKind::SubPath,
        };
        let excl = ResolverError::MissingFile {
            dep: dep(),
            path: "internal/x.wdl".into(),
            kind: MissingFileKind::Excluded,
        };

        assert!(entry.to_string().contains("entrypoint"));
        assert!(sub.to_string().contains("not found"));
        assert!(excl.to_string().contains("excluded"));
    }
}
