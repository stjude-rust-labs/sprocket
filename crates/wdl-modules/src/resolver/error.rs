//! Top-level error type for the resolver layer.

use std::path::PathBuf;

#[cfg(feature = "git-resolver")]
use semver::Version;
use thiserror::Error;

#[cfg(feature = "git-resolver")]
use crate::hash::ContentHash;
#[cfg(feature = "git-resolver")]
use crate::hash::HashError;
use crate::lockfile::LockfileError;
use crate::manifest::ManifestError;
#[cfg(feature = "git-resolver")]
use crate::module_walk::ModuleWalkError;
use crate::signing::SignerIdentity;
use crate::signing::VerifyingKey;
#[cfg(feature = "git-resolver")]
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

    /// A symbolic sub-path component matched more than one directory
    /// entry after hyphen-to-underscore normalization.
    #[error(
        "symbolic path `{path}` in `{dep}` is ambiguous: it matches multiple entries ({})",
        .entries.join(", ")
    )]
    AmbiguousSubPath {
        /// The owning dependency.
        dep: String,
        /// The symbolic sub-path being resolved.
        path: String,
        /// The competing on-disk entry names.
        entries: Vec<String>,
    },

    /// A dependency directory did not contain a `module.json`.
    #[error(
        "no `module.json` found at `{path}`; the dependency is not a WDL module (or the `path` is \
         wrong)"
    )]
    MissingManifest {
        /// The missing manifest path.
        path: PathBuf,
    },

    /// The dependency graph contains a cycle.
    #[cfg(feature = "git-resolver")]
    #[error("dependency cycle: {}", format_cycle(.path))]
    Cycle {
        /// The cycle path, in resolution order.
        path: Vec<String>,
    },

    /// No discovered version satisfies the dependency's version
    /// requirement.
    #[cfg(feature = "git-resolver")]
    #[error(
        "{}",
        no_satisfying_version_message(.dep, .requirement, .considered, .path.as_deref())
    )]
    NoSatisfyingVersion {
        /// The dependency name.
        dep: String,
        /// The unmet version requirement.
        requirement: VersionRequirement,
        /// The versions discovered before filtering by the requirement.
        considered: Vec<Version>,
        /// Optional path prefix used for path-scoped tags.
        path: Option<String>,
    },

    /// A dependency is not present in the lockfile. Run
    /// `sprocket module lock` to update it.
    #[error("`{dep}` is not in `module-lock.json`; run `sprocket module lock` to update")]
    NotInLockfile {
        /// The missing dependency.
        dep: String,
    },

    /// A locked Git dependency has not been fetched into the cache yet.
    #[cfg(feature = "git-resolver")]
    #[error("`{dep}` is not fetched in the module cache; run `sprocket module fetch`")]
    NotFetched {
        /// The dependency that is missing from cache.
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
    #[cfg(feature = "git-resolver")]
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
        "signer for `{dep}` has changed since the lockfile was written ({})",
        trust_all_hint(.observed.as_ref(), None)
    )]
    SignerKeyMismatch {
        /// The owning dependency.
        dep: String,
        /// The source URL or path to trust.
        source_url: Option<String>,
        /// The subdirectory module path, when present.
        path: Option<String>,
        /// The signer key recorded in the lockfile.
        expected: Box<VerifyingKey>,
        /// The signer key observed in the cache.
        observed: Box<VerifyingKey>,
    },

    /// A locked dependency signer is not present in the trust store.
    #[error(
        "`{dep}` is signed by an untrusted key ({})",
        trust_all_hint(.signer.as_ref(), .identity.as_ref())
    )]
    UntrustedSigner {
        /// The owning dependency.
        dep: String,
        /// The signer key recorded in the lockfile.
        signer: Box<VerifyingKey>,
        /// Optional signer identity metadata.
        identity: Option<SignerIdentity>,
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
    #[cfg(feature = "git-resolver")]
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
    #[cfg(feature = "git-resolver")]
    #[error("`{dep}` `commit` value `{value}` is not a valid Git commit SHA")]
    InvalidCommit {
        /// The owning dependency.
        dep: String,
        /// The unparsable value.
        value: String,
    },

    /// A `module.sig` file was present but failed to verify against the
    /// observed content hash.
    #[cfg(feature = "git-resolver")]
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
    #[cfg(feature = "git-resolver")]
    #[error("`{dep}` `module.sig` failed to parse")]
    SignatureParse {
        /// The owning dependency.
        dep: String,
        /// The underlying parse error.
        #[source]
        source: crate::signing::SignatureFileError,
    },

    /// A manifest `exclude` pattern is not a valid glob.
    #[cfg(feature = "git-resolver")]
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
    #[cfg(feature = "git-resolver")]
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
    #[cfg(feature = "git-resolver")]
    #[error("`{dep}` is declared by the consumer but absent from the freshly-resolved tree")]
    MissingFreshDependency {
        /// The missing dependency name.
        dep: String,
    },

    /// A Git URL violates the configured scheme policy.
    #[cfg(feature = "git-resolver")]
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
    #[cfg(feature = "git-resolver")]
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
    #[cfg(feature = "git-resolver")]
    #[error("`{dep}` git URL `{url}` targets host `{host}` which is not allowed by policy")]
    GitHostPolicyViolation {
        /// The owning dependency.
        dep: String,
        /// The rejected URL.
        url: String,
        /// The rejected host.
        host: String,
    },

    /// A Git URL's host is not in the configured allow list for its scope.
    #[cfg(feature = "git-resolver")]
    #[error(
        "`{dep}` git URL `{url}` targets host `{host}` which is not in the configured allow list; \
         to allow it, add `{host}` to `{config_key}` in the `[modules]` section of your \
         `sprocket.toml`"
    )]
    GitHostNotAllowed {
        /// The owning dependency.
        dep: String,
        /// The rejected URL.
        url: String,
        /// The rejected host.
        host: String,
        /// The config key that would permit the host for this scope.
        config_key: &'static str,
    },

    /// A materialized module tree exceeded configured resource limits.
    #[cfg(feature = "git-resolver")]
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
    #[cfg(feature = "git-resolver")]
    #[error(transparent)]
    Git(#[from] crate::resolver::git::GitError),

    /// A materialized module contains a symbolic link, which is not
    /// permitted anywhere in a module tree.
    #[cfg(feature = "git-resolver")]
    #[error("`{dep}` contains a symbolic link, which is not permitted in a module: `{path}`")]
    MaterializedSymlink {
        /// The owning dependency.
        dep: String,
        /// The offending path.
        path: PathBuf,
    },

    /// A quoted `import` inside a module resolves to a file outside the
    /// module root, which makes the module invalid.
    #[error(
        "`{dep}` file `{file}` has a quoted import `{import}` that resolves outside the module \
         root"
    )]
    QuotedImportEscapesRoot {
        /// The owning dependency.
        dep: String,
        /// The `.wdl` file containing the offending import, relative to
        /// the module root.
        file: String,
        /// The offending import target as written.
        import: String,
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
    #[cfg(feature = "git-resolver")]
    #[error(transparent)]
    Walk(#[from] ModuleWalkError),

    /// Hashing a cache leaf or local path failed.
    #[cfg(feature = "git-resolver")]
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
#[cfg(feature = "git-resolver")]
fn format_cycle(path: &[String]) -> String {
    path.join(" → ")
}

/// Renders a list of versions for error display, or `<none>` when empty.
#[cfg(feature = "git-resolver")]
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

/// Renders the `module trust all` command hint for a changed signer.
fn trust_all_hint(observed: &VerifyingKey, identity: Option<&SignerIdentity>) -> String {
    let key = render_signer(observed, identity);
    format!("{key}; run `sprocket module trust all` to accept signer trust changes")
}

/// Renders a signer key with optional identity metadata.
fn render_signer(key: &VerifyingKey, identity: Option<&SignerIdentity>) -> String {
    let key = key.to_openssh();
    if let Some(identity) = identity {
        match (identity.name.as_deref(), identity.email.as_deref()) {
            (Some(name), Some(email)) => format!("{key} {name} <{email}>"),
            (Some(name), None) => format!("{key} {name}"),
            (None, Some(email)) => format!("{key} <{email}>"),
            (None, None) => key,
        }
    } else {
        key
    }
}

/// Renders the no-satisfying-version error with an optional path-scoped hint.
#[cfg(feature = "git-resolver")]
fn no_satisfying_version_message(
    dep: &str,
    requirement: &VersionRequirement,
    considered: &[Version],
    path: Option<&str>,
) -> String {
    let mut message = format!(
        "no version satisfies `{dep}` requirement `{requirement}` (considered: {})",
        format_versions(considered)
    );
    if let Some(path) = path {
        message.push_str(&format!(
            "; for a subdirectory module, Git tags must be named `{path}/v<semver>`"
        ));
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dep() -> String {
        "foo".to_string()
    }

    fn key(s: &str) -> Box<VerifyingKey> {
        // SAFETY: the test passes complete OpenSSH Ed25519 public keys.
        Box::new(s.parse().unwrap())
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

    #[cfg(feature = "git-resolver")]
    #[test]
    fn no_satisfying_version_includes_path_hint_when_present() {
        let err = ResolverError::NoSatisfyingVersion {
            dep: dep(),
            requirement: "^2".parse().unwrap(),
            considered: vec!["1.0.0".parse().unwrap()],
            path: Some("tasks".to_string()),
        };
        assert!(err.to_string().contains("tasks/v<semver>"));
    }

    #[test]
    fn signer_mismatch_includes_key_and_trust_all_command() {
        let err = ResolverError::SignerKeyMismatch {
            dep: "divination".to_string(),
            source_url: Some("file:///spellbook".to_string()),
            path: Some("modules/divination".to_string()),
            expected: key(
                "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAINiRUmfYzFTjksGItM2fSm9s1eCL8NnMJGQgW724Uph1"
            ),
            observed: key(
                "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIX5S41sfLWGBzdeYMeIAT8E96dtk+ymT4WqiY7oq+21"
            ),
        };

        let message = err.to_string();
        assert!(message.contains(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIX5S41sfLWGBzdeYMeIAT8E96dtk+ymT4WqiY7oq+21"
        ));
        assert!(message.contains("sprocket module trust all"));
    }
}
