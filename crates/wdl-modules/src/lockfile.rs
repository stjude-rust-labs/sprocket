//! `module-lock.json` lockfile parsing and validation.

use std::collections::BTreeMap;
use std::fmt;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;
use url::Url;

#[cfg(feature = "git-resolver")]
use crate::Manifest;
use crate::dependency::DependencyName;
use crate::dependency::DependencyNameError;
use crate::dependency::GitModulePath;
use crate::dependency::GitSelector;
use crate::hash::ContentHash;
use crate::signing::VerifyingKey;

/// The current lockfile schema version.
pub const LOCKFILE_VERSION: u32 = 1;

/// An error parsing a [`Lockfile`].
#[derive(Debug, Error)]
pub enum LockfileError {
    /// The bytes did not parse as JSON or did not match the lockfile
    /// schema.
    #[error("invalid `module-lock.json` JSON")]
    InvalidJson(#[from] serde_json::Error),

    /// The lockfile declares a `version` other than [`LOCKFILE_VERSION`].
    #[error(
        "unsupported lockfile version `{0}`; this build only supports version `{LOCKFILE_VERSION}`"
    )]
    UnsupportedVersion(u32),

    /// A `dependencies` key is not a valid WDL identifier.
    #[error(transparent)]
    DependencyName(#[from] DependencyNameError),

    /// A Git-sourced entry is missing its required `checksum`.
    #[error("lockfile entry for `{0}` has a Git source but no `checksum`")]
    MissingChecksum(String),

    /// A local path entry carries a `checksum` or `signer`, which are only
    /// valid for Git sources.
    #[error(
        "lockfile entry for `{0}` has a local path source but carries a `checksum` or `signer`"
    )]
    PathSourceIntegrity(String),
}

/// A parsed `module-lock.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lockfile {
    /// The lockfile schema version. Currently always [`LOCKFILE_VERSION`].
    pub version: u32,
    /// The top-level dependency map, keyed by consumer-chosen name.
    pub dependencies: DependencyMap,
}

impl Default for Lockfile {
    fn default() -> Self {
        Self {
            version: LOCKFILE_VERSION,
            dependencies: DependencyMap::new(),
        }
    }
}

impl Lockfile {
    /// Parses a `module-lock.json` from raw bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, LockfileError> {
        let lockfile: Lockfile = crate::strict_json::from_slice(bytes)?;
        if lockfile.version != LOCKFILE_VERSION {
            return Err(LockfileError::UnsupportedVersion(lockfile.version));
        }
        validate_integrity_fields(&lockfile.dependencies)?;
        Ok(lockfile)
    }

    /// Writes the lockfile as pretty-printed JSON.
    pub fn write(&self, w: impl Write) -> std::io::Result<()> {
        serde_json::to_writer_pretty(w, self).map_err(std::io::Error::other)
    }

    /// Looks up a dependency entry by walking the nested `dependencies`
    /// tree along `scope` (the chain of consumer dependency names from the
    /// top-level consumer down to the entry's parent), then resolving
    /// `name` in that scope. An empty `scope` looks up a top-level entry.
    pub fn find_scoped(
        &self,
        scope: &[DependencyName],
        name: &DependencyName,
    ) -> Option<&DependencyEntry> {
        let mut current = &self.dependencies;
        for parent in scope {
            current = &current.get(parent)?.dependencies;
        }
        current.get(name)
    }

    /// Returns true when this lockfile's top-level dependencies exactly
    /// match `manifest.dependencies` and each locked source still
    /// satisfies the manifest declaration.
    #[cfg(feature = "git-resolver")]
    pub fn satisfies_manifest(&self, manifest: &Manifest) -> bool {
        manifest.dependencies.iter().all(|(name, source)| {
            self.find_scoped(&[], name)
                .is_some_and(|entry| crate::resolver::lock::satisfies(entry, source))
        }) && self
            .dependencies
            .keys()
            .all(|name| manifest.dependencies.contains_key(name))
    }
}

/// Recursively enforces that Git-sourced entries carry a `checksum` and
/// that local path entries carry neither `checksum` nor `signer`.
fn validate_integrity_fields(deps: &DependencyMap) -> Result<(), LockfileError> {
    for (name, entry) in deps {
        match &entry.source {
            ResolvedSource::Git { .. } => {
                if entry.checksum.is_none() {
                    return Err(LockfileError::MissingChecksum(name.manifest().to_string()));
                }
            }
            ResolvedSource::Path { .. } => {
                if entry.checksum.is_some() || entry.signer.is_some() {
                    return Err(LockfileError::PathSourceIntegrity(
                        name.manifest().to_string(),
                    ));
                }
            }
        }
        validate_integrity_fields(&entry.dependencies)?;
    }
    Ok(())
}

/// A `dependencies` map keyed by consumer-chosen dependency names.
pub type DependencyMap = BTreeMap<DependencyName, DependencyEntry>;

/// One entry in a [`DependencyMap`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyEntry {
    /// The resolved source for the dependency.
    pub source: ResolvedSource,
    /// The module's content hash.
    ///
    /// Required for Git sources and absent for local path sources, whose
    /// content is read as-is at execution time and carries no checksum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<ContentHash>,
    /// The signer's public key, if the module was signed at lock time.
    ///
    /// Absent for unsigned modules and for local path sources, which are
    /// not subject to signature verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer: Option<VerifyingKey>,
    /// The module's transitive dependencies.
    pub dependencies: DependencyMap,
}

/// The resolved source of a dependency.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum ResolvedSource {
    /// A Git source resolved to a specific commit.
    Git {
        /// The Git repository URL.
        git: Url,
        /// The full 40-character lowercase hex commit SHA the selector
        /// resolved to at lock time.
        sha: GitCommit,
        /// The selector from `module.json` that produced this entry.
        ///
        /// Tag and branch selectors carry mutable refs that cannot be
        /// validated from the resolved commit alone, so this field is
        /// required to allow integrity checks without a full relock.
        selector: GitSelector,
        /// The sub-path within the repository where the module lives.
        ///
        /// Omitted when the module sits at the repository root.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<GitModulePath>,
    },
    /// A local filesystem source.
    Path {
        /// The local path to the module directory.
        path: PathBuf,
    },
}

impl ResolvedSource {
    /// Returns the source URL as a string suitable for trust-store
    /// lookups.
    pub fn source_url(&self) -> String {
        match self {
            Self::Git { git, .. } => git.to_string(),
            Self::Path { path } => path.display().to_string(),
        }
    }

    /// Returns the sub-path within the source, or `None` when the
    /// module sits at the source root.
    pub fn source_path(&self) -> Option<&str> {
        match self {
            Self::Git { path: Some(p), .. } => Some(p.as_str()),
            _ => None,
        }
    }

    /// Returns the source's identity coordinates for cycle detection.
    ///
    /// For Git sources this is the repository URL and sub-path; for
    /// local path sources it is the resolved directory. The resolved
    /// commit and the selector are deliberately excluded so that a
    /// module cannot transitively depend on itself even at a different
    /// version or via a different selector.
    pub fn coordinates(&self) -> SourceCoordinates<'_> {
        match self {
            Self::Git { git, path, .. } => SourceCoordinates::Git {
                git: git.as_str(),
                path: path.as_ref().map(GitModulePath::as_str),
            },
            Self::Path { path } => SourceCoordinates::Path(path.as_path()),
        }
    }
}

/// The identity coordinates of a [`ResolvedSource`], used for cycle
/// detection. Excludes version and selector information.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceCoordinates<'a> {
    /// A Git source, identified by repository URL and optional sub-path.
    Git {
        /// The Git repository URL.
        git: &'a str,
        /// The sub-path within the repository, if any.
        path: Option<&'a str>,
    },
    /// A local path source, identified by its resolved directory.
    Path(&'a std::path::Path),
}

/// A 40-character lowercase hex Git commit SHA.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct GitCommit(String);

impl GitCommit {
    /// Returns the commit SHA as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GitCommit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for GitCommit {
    type Error = GitCommitError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s.len() == 40
            && s.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            Ok(Self(s))
        } else {
            Err(GitCommitError(s))
        }
    }
}

impl FromStr for GitCommit {
    type Err = GitCommitError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_string())
    }
}

/// An error parsing a [`GitCommit`].
#[derive(Debug, Error)]
#[error("git commit `{0}` must be exactly 40 lowercase hex characters")]
pub struct GitCommitError(String);

/// A Git commit selector: any unique prefix of a commit SHA, from 4 to
/// 40 lowercase hex characters (Git's own minimum abbreviation length).
/// The resolver expands the prefix to a full [`GitCommit`] at lock time.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct GitCommitish(String);

impl GitCommitish {
    /// Returns the commit-ish as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns true when the selector is a full 40-character SHA.
    pub fn is_full(&self) -> bool {
        self.0.len() == 40
    }
}

impl fmt::Display for GitCommitish {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for GitCommitish {
    type Error = GitCommitishError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if (4..=40).contains(&s.len())
            && s.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            Ok(Self(s))
        } else {
            Err(GitCommitishError(s))
        }
    }
}

impl FromStr for GitCommitish {
    type Err = GitCommitishError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_string())
    }
}

/// An error parsing a [`GitCommitish`].
#[derive(Debug, Error)]
#[error("git commit `{0}` must be 4 to 40 lowercase hex characters")]
pub struct GitCommitishError(String);

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<Lockfile, LockfileError> {
        Lockfile::parse(s.as_bytes())
    }

    #[cfg(feature = "git-resolver")]
    fn parse_manifest(s: &str) -> Manifest {
        Manifest::parse(s.as_bytes()).unwrap()
    }

    #[test]
    fn parses_minimal_lockfile() {
        let l = parse(r#"{"version": 1, "dependencies": {}}"#).unwrap();
        assert_eq!(l.version, 1);
        assert!(l.dependencies.is_empty());
    }

    #[test]
    fn parses_recursive_lockfile() {
        let l = parse(
            r#"{
                "version": 1,
                "dependencies": {
                    "spellbook": {
                        "source": {
                            "git": "https://github.com/openwdl/spellbook",
                            "sha": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
                            "selector": {"version": "^1"}
                        },
                        "checksum": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                        "dependencies": {
                            "common": {
                                "source": {
                                    "git": "https://github.com/openwdl/common",
                                    "sha": "d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5",
                                    "selector": {"version": "^0.3"}
                                },
                                "checksum": "sha256:4355a46b19d348dc2f57c046f8ef63d4538ebb936000f3c9ee954a27460dd865",
                                "dependencies": {}
                            }
                        }
                    },
                    "local_utils": {
                        "source": { "path": "../utils" },
                        "dependencies": {}
                    }
                }
            }"#,
        )
        .unwrap();

        assert_eq!(l.dependencies.len(), 2);
        let spellbook = l.dependencies.get(&"spellbook".parse().unwrap()).unwrap();
        assert!(matches!(spellbook.source, ResolvedSource::Git { .. }));
        assert_eq!(spellbook.dependencies.len(), 1);
    }

    #[test]
    fn round_trips_lockfile() {
        let original = parse(
            r#"{
                "version": 1,
                "dependencies": {
                    "local_utils": {
                        "source": { "path": "../utils" },
                        "dependencies": {}
                    }
                }
            }"#,
        )
        .unwrap();

        let mut buf = Vec::new();
        original.write(&mut buf).unwrap();
        let parsed = Lockfile::parse(&buf).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn rejects_duplicate_keys() {
        let err = parse(
            r#"{
                "version": 1,
                "version": 2,
                "dependencies": {}
            }"#,
        )
        .unwrap_err();
        assert!(
            matches!(err, LockfileError::InvalidJson(e) if e.to_string().contains("duplicate"))
        );
    }

    #[test]
    fn rejects_unknown_top_level_fields() {
        let err = parse(r#"{"version": 1, "dependencies": {}, "extra": 42}"#).unwrap_err();
        assert!(matches!(err, LockfileError::InvalidJson(_)));
    }

    #[test]
    fn rejects_wrong_version() {
        let err = parse(r#"{"version": 2, "dependencies": {}}"#).unwrap_err();
        assert!(matches!(err, LockfileError::UnsupportedVersion(2)));
    }

    #[test]
    fn rejects_bad_commit_sha() {
        let err = parse(
            r#"{
                "version": 1,
                "dependencies": {
                    "spellbook": {
                        "source": {
                            "git": "https://x/y",
                            "sha": "not-a-sha",
                            "selector": {"tag": "v1"}
                        },
                        "checksum": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                        "dependencies": {}
                    }
                }
            }"#,
        )
        .unwrap_err();
        assert!(matches!(err, LockfileError::InvalidJson(_)));
    }

    #[test]
    fn rejects_bad_checksum() {
        let err = parse(
            r#"{
                "version": 1,
                "dependencies": {
                    "local": {
                        "source": { "path": "../utils" },
                        "checksum": "md5:abc",
                        "dependencies": {}
                    }
                }
            }"#,
        )
        .unwrap_err();
        assert!(matches!(err, LockfileError::InvalidJson(_)));
    }

    #[test]
    fn parses_git_source_with_path() {
        let l = parse(
            r#"{
                "version": 1,
                "dependencies": {
                    "csvcut": {
                        "source": {
                            "git": "https://github.com/openwdl/tasks",
                            "sha": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
                            "selector": {"tag": "v1.2.0"},
                            "path": "csvcut"
                        },
                        "checksum": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                        "dependencies": {}
                    }
                }
            }"#,
        )
        .unwrap();
        let csvcut = l.dependencies.get(&"csvcut".parse().unwrap()).unwrap();
        match &csvcut.source {
            ResolvedSource::Git { path, .. } => {
                assert_eq!(path.as_ref().map(|p| p.as_str()), Some("csvcut"));
            }
            _ => panic!("expected `Git` source"),
        }
    }

    #[cfg(feature = "git-resolver")]
    #[test]
    fn satisfies_manifest_present_and_satisfied() {
        let manifest = parse_manifest(
            r#"{
                "name":"consumer",
                "license":"MIT",
                "dependencies":{
                    "foo":{"git":"https://github.com/openwdl/foo","version":"^1"}
                }
            }"#,
        );
        let lock = parse(
            r#"{
                "version":1,
                "dependencies":{
                    "foo":{
                        "source":{
                            "git":"https://github.com/openwdl/foo",
                            "sha":"0000000000000000000000000000000000000001",
                            "selector":{"version":"^1"}
                        },
                        "checksum":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                        "dependencies":{}
                    }
                }
            }"#,
        )
        .unwrap();
        assert!(lock.satisfies_manifest(&manifest));
    }

    #[cfg(feature = "git-resolver")]
    #[test]
    fn satisfies_manifest_true_for_same_branch_selector() {
        let manifest = parse_manifest(
            r#"{
                "name":"consumer",
                "license":"MIT",
                "dependencies":{
                    "foo":{
                        "git":"https://github.com/openwdl/foo",
                        "branch":"main",
                        "path":"modules/foo"
                    }
                }
            }"#,
        );
        let lock = parse(
            r#"{
                "version":1,
                "dependencies":{
                    "foo":{
                        "source":{
                            "git":"https://github.com/openwdl/foo",
                            "sha":"0000000000000000000000000000000000000001",
                            "selector":{"branch":"main"},
                            "path":"modules/foo"
                        },
                        "checksum":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                        "dependencies":{}
                    }
                }
            }"#,
        )
        .unwrap();
        assert!(lock.satisfies_manifest(&manifest));
    }

    #[cfg(feature = "git-resolver")]
    #[test]
    fn satisfies_manifest_false_when_branch_path_changes() {
        let manifest = parse_manifest(
            r#"{
                "name":"consumer",
                "license":"MIT",
                "dependencies":{
                    "foo":{
                        "git":"https://github.com/openwdl/foo",
                        "branch":"main",
                        "path":"modules/new"
                    }
                }
            }"#,
        );
        let lock = parse(
            r#"{
                "version":1,
                "dependencies":{
                    "foo":{
                        "source":{
                            "git":"https://github.com/openwdl/foo",
                            "sha":"0000000000000000000000000000000000000001",
                            "selector":{"branch":"main"},
                            "path":"modules/old"
                        },
                        "checksum":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                        "dependencies":{}
                    }
                }
            }"#,
        )
        .unwrap();
        assert!(!lock.satisfies_manifest(&manifest));
    }

    #[cfg(feature = "git-resolver")]
    #[test]
    fn satisfies_manifest_false_when_dep_missing_from_lock() {
        let manifest = parse_manifest(
            r#"{
                "name":"consumer",
                "license":"MIT",
                "dependencies":{
                    "foo":{"git":"https://github.com/openwdl/foo","version":"^1"}
                }
            }"#,
        );
        let lock = parse(r#"{"version":1,"dependencies":{}}"#).unwrap();
        assert!(!lock.satisfies_manifest(&manifest));
    }

    #[cfg(feature = "git-resolver")]
    #[test]
    fn satisfies_manifest_false_with_orphan_top_level_entry() {
        let manifest = parse_manifest(
            r#"{
                "name":"consumer",
                "license":"MIT"
            }"#,
        );
        let lock = parse(
            r#"{
                "version":1,
                "dependencies":{
                    "orphan":{
                        "source":{"path":"../orphan"},
                        "dependencies":{}
                    }
                }
            }"#,
        )
        .unwrap();
        assert!(!lock.satisfies_manifest(&manifest));
    }

    #[cfg(feature = "git-resolver")]
    #[test]
    fn satisfies_manifest_true_with_nested_transitives_under_satisfied_top_level() {
        let manifest = parse_manifest(
            r#"{
                "name":"consumer",
                "license":"MIT",
                "dependencies":{
                    "foo":{"git":"https://github.com/openwdl/foo","version":"^1"}
                }
            }"#,
        );
        let lock = parse(
            r#"{
                "version":1,
                "dependencies":{
                    "foo":{
                        "source":{
                            "git":"https://github.com/openwdl/foo",
                            "sha":"0000000000000000000000000000000000000001",
                            "selector":{"version":"^1"}
                        },
                        "checksum":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                        "dependencies":{
                            "bar":{
                                "source":{"path":"../bar"},
                                "dependencies":{}
                            }
                        }
                    }
                }
            }"#,
        )
        .unwrap();
        assert!(lock.satisfies_manifest(&manifest));
    }
}
