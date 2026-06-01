//! Top-level error type for the resolver layer.

use std::path::PathBuf;

use semver::Version;
use thiserror::Error;

use crate::hash::ContentHash;
use crate::hash::HashError;
use crate::lockfile::LockfileError;
use crate::manifest::ManifestError;
use crate::module_walk::ModuleWalkError;
use crate::signing::VerifyingKey;
use crate::version_requirement::VersionRequirement;

/// An error returned by the [`Resolver`](crate::Resolver) trait or
/// any resolver-layer operation.
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
        dep: String,
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
        path: Vec<String>,
    },

    /// No discovered version satisfies the dependency's version
    /// requirement.
    #[error(
        "no version satisfies `{dep}` requirement `{requirement}` (considered: {})",
        format_versions(.considered)
    )]
    NoSatisfyingVersion {
        /// The dependency name.
        dep: String,
        /// The unmet version requirement.
        requirement: VersionRequirement,
        /// The versions discovered before filtering by the requirement.
        considered: Vec<Version>,
    },

    /// A dependency is not present in the lockfile. Run
    /// `sprocket module lock` to update it.
    #[error("`{dep}` is not in `module-lock.json`; run `sprocket module lock` to update")]
    NotInLockfile {
        /// The missing dependency.
        dep: String,
    },

    /// The manifest source for a dependency does not match the
    /// lockfile source. Run `sprocket module lock` to update.
    #[error(
        "`{dep}` manifest source differs from the lockfile; run `sprocket module lock` to update"
    )]
    LockfileSourceMismatch {
        /// The dependency whose source changed.
        dep: String,
    },

    /// A cached module's content hash does not match the lockfile's
    /// recorded checksum.
    #[error(
        "cached `{dep}` content hash does not match the lockfile (expected `{expected}`, observed \
         `{observed}`)"
    )]
    ChecksumMismatch {
        /// The owning dependency.
        dep: String,
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
        dep: String,
        /// The signer key recorded in the lockfile.
        expected: Box<VerifyingKey>,
        /// The signer key observed in the cache.
        observed: Box<VerifyingKey>,
    },

    /// A dependency was signed when the lockfile was written but is now
    /// unsigned. This prevents a supply-chain downgrade where an
    /// attacker strips the signature from a module whose content hash
    /// has not changed (since `module.sig` is excluded from the hash).
    #[error("`{dep}` was signed when locked but is now unsigned; this may indicate tampering")]
    SignatureDowngrade {
        /// The owning dependency.
        dep: String,
        /// The signer key recorded in the lockfile.
        expected_signer: Box<VerifyingKey>,
    },

    /// A Git tag or branch named in a dependency's selector does not
    /// exist on the remote.
    #[error("`{dep}` selector references unknown {kind} `{name}`")]
    UnknownGitRef {
        /// The owning dependency.
        dep: String,
        /// The kind of ref that was missing.
        kind: GitRefKind,
        /// The ref name as it appeared in the manifest.
        name: String,
    },

    /// A `commit` selector did not parse as a valid 40-character lowercase
    /// hex SHA.
    #[error("`{dep}` `commit` value `{value}` is not a valid Git commit SHA")]
    InvalidCommit {
        /// The owning dependency.
        dep: String,
        /// The unparsable value.
        value: String,
    },

    /// A `module.sig` file was present but failed to verify against the
    /// observed content hash.
    #[error(
        "`{dep}` signature does not match observed content (signer: `{}`)",
        signer.to_openssh()
    )]
    SignatureVerificationFailed {
        /// The owning dependency.
        dep: String,
        /// The signer key from the rejected `module.sig`.
        signer: Box<VerifyingKey>,
    },

    /// A `module.sig` file failed to parse.
    #[error("`{dep}` `module.sig` failed to parse")]
    SignatureParse {
        /// The owning dependency.
        dep: String,
        /// The underlying parse error.
        #[source]
        source: crate::signing::SignatureFileError,
    },

    /// A manifest `exclude` pattern is not a valid glob.
    #[error("invalid `exclude` pattern `{pattern}`")]
    InvalidExclude {
        /// The offending pattern.
        pattern: String,
        /// The underlying glob error.
        #[source]
        source: globset::Error,
    },

    /// `require_signed` is enabled and the dependency is unsigned.
    #[error("`{dep}` is unsigned but `require_signed` is enabled")]
    RequireSignedViolation {
        /// The unsigned dependency.
        dep: String,
    },

    /// A transitive dependency declared a local-path source from a
    /// non-local parent.
    #[error(
        "`{dep}` declares a local-path source but is reachable through a non-local parent; only \
         locally-rooted projects may use local-path dependencies"
    )]
    LocalPathInTransitive {
        /// The offending dependency name.
        dep: String,
    },

    /// A dependency declared by the consumer was missing from the
    /// freshly-resolved tree and not satisfied by the prior lockfile.
    #[error("`{dep}` is declared by the consumer but absent from the freshly-resolved tree")]
    MissingFreshDependency {
        /// The missing dependency name.
        dep: String,
    },

    /// A Git URL violates the configured scheme policy.
    #[error("`{dep}` git URL `{url}` uses scheme `{scheme}` which is not allowed by policy")]
    GitUrlPolicyViolation {
        /// The owning dependency.
        dep: String,
        /// The rejected URL.
        url: String,
        /// The rejected scheme.
        scheme: String,
    },

    /// DNS resolution for a Git URL's hostname failed. The resolver
    /// rejects the URL rather than allowing a potentially spoofed host
    /// through.
    #[error("`{dep}` git URL `{url}` host `{host}` could not be resolved")]
    GitHostResolutionFailed {
        /// The owning dependency.
        dep: String,
        /// The URL that failed resolution.
        url: String,
        /// The hostname that could not be resolved.
        host: String,
    },

    /// A Git URL violates the configured host policy.
    #[error("`{dep}` git URL `{url}` targets host `{host}` which is not allowed by policy")]
    GitHostPolicyViolation {
        /// The owning dependency.
        dep: String,
        /// The rejected URL.
        url: String,
        /// The rejected host.
        host: String,
    },

    /// A materialized module tree exceeded configured resource limits.
    #[error("`{dep}` materialized tree exceeds limits (files: {files}, bytes: {bytes})")]
    MaterializedTreeLimitExceeded {
        /// The owning dependency.
        dep: String,
        /// Number of files observed.
        files: usize,
        /// Total bytes observed.
        bytes: u64,
    },

    /// A Git operation failed.
    #[error(transparent)]
    Git(#[from] crate::resolver::git::GitError),

    /// A materialized file resolved through a symlink that escapes the
    /// module root.
    #[error("`{dep}` materialized path escapes module root: `{path}`")]
    MaterializedSymlinkEscape {
        /// The owning dependency.
        dep: String,
        /// The escaping path as observed before canonicalization.
        path: PathBuf,
    },

    /// An I/O error.
    #[error("i/o error at `{path}`")]
    Io {
        /// The path involved.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A module-walk error (symlink containment, metadata target, etc.).
    #[error(transparent)]
    Walk(#[from] ModuleWalkError),

    /// Hashing a cache leaf or local path failed.
    #[error(transparent)]
    Hash(#[from] HashError),

    /// A `Manifest` parse or validation error.
    #[error(transparent)]
    Manifest(#[from] ManifestError),

    /// A `Lockfile` parse or validation error.
    #[error(transparent)]
    Lockfile(#[from] LockfileError),

    /// A `RelativePath` validation error.
    #[error(transparent)]
    RelativePath(#[from] crate::relative_path::RelativePathError),
}

/// The kind of Git reference named in a [`ResolverError::UnknownGitRef`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitRefKind {
    /// The reference was an annotated or lightweight tag.
    Tag,
    /// The reference was a branch (head).
    Branch,
}

impl std::fmt::Display for GitRefKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tag => f.write_str("tag"),
            Self::Branch => f.write_str("branch"),
        }
    }
}

/// The kind of file lookup that failed in a [`ResolverError::MissingFile`].
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
fn missing_file_message(dep: &str, path: &std::path::Path, kind: &MissingFileKind) -> String {
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
fn format_cycle(path: &[String]) -> String {
    path.join(" → ")
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

    fn dep() -> String {
        "foo".to_string()
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
