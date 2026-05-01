//! `module-lock.json` lockfile parsing and validation.

use std::collections::BTreeMap;
use std::fmt;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;

use semver::Version;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;
use url::Url;

use crate::ContentHash;
use crate::DependencyName;
use crate::DependencyNameError;
use crate::RelativePath;
use crate::RelativePathError;
use crate::VerifyingKey;

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

impl Lockfile {
    /// Parses a `module-lock.json` from raw bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, LockfileError> {
        let lockfile: Lockfile = crate::strict_json::from_slice(bytes)?;
        if lockfile.version != LOCKFILE_VERSION {
            return Err(LockfileError::UnsupportedVersion(lockfile.version));
        }
        Ok(lockfile)
    }

    /// Writes the lockfile as pretty-printed JSON.
    pub fn write(&self, w: impl Write) -> std::io::Result<()> {
        serde_json::to_writer_pretty(w, self).map_err(std::io::Error::other)
    }
}

/// A `dependencies` map keyed by consumer-chosen dependency names.
pub type DependencyMap = BTreeMap<DependencyName, DependencyEntry>;

/// One entry in a [`DependencyMap`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyEntry {
    /// The resolved source for the dependency.
    pub source: ResolvedSource,
    /// The modules discovered within the source, keyed by their
    /// directory's relative path from the source root (`.` for the source
    /// root itself).
    pub modules: BTreeMap<ModulePath, LockedModule>,
}

/// The resolved source of a dependency.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum ResolvedSource {
    /// A Git source resolved to a specific commit.
    Git {
        /// The Git repository URL.
        git: Url,
        /// The 40-character lowercase hex commit SHA.
        commit: GitCommit,
    },
    /// A local filesystem source.
    Path {
        /// The local path to the module directory.
        path: PathBuf,
    },
}

/// A 40-character lowercase hex Git commit SHA.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct GitCommit(String);

impl GitCommit {
    /// Returns the commit SHA as a string slice.
    pub fn inner(&self) -> &str {
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
#[error("Git commit `{0}` must be exactly 40 lowercase hex characters")]
pub struct GitCommitError(String);

/// One locked module entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedModule {
    /// The module's version at lock time.
    pub version: Version,
    /// The module's content hash.
    pub checksum: ContentHash,
    /// The signer's public key, if the module was signed at lock time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer: Option<VerifyingKey>,
    /// The module's transitive dependencies.
    pub dependencies: DependencyMap,
}

/// The map key under [`DependencyEntry::modules`]. The literal `.`
/// denotes the source root; otherwise the value is a [`RelativePath`]
/// from the source root to the directory containing the module's
/// `module.json`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum ModulePath {
    /// The module sits at the source root.
    Root,
    /// The module sits at a relative path under the source root.
    Sub(RelativePath),
}

impl fmt::Display for ModulePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Root => f.write_str("."),
            Self::Sub(p) => fmt::Display::fmt(p, f),
        }
    }
}

impl From<ModulePath> for String {
    fn from(p: ModulePath) -> Self {
        match p {
            ModulePath::Root => ".".to_string(),
            ModulePath::Sub(p) => p.into_inner(),
        }
    }
}

impl TryFrom<String> for ModulePath {
    type Error = RelativePathError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s == "." {
            Ok(Self::Root)
        } else {
            Ok(Self::Sub(RelativePath::try_from(s)?))
        }
    }
}

impl FromStr for ModulePath {
    type Err = RelativePathError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<Lockfile, LockfileError> {
        Lockfile::parse(s.as_bytes())
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
                            "commit": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"
                        },
                        "modules": {
                            ".": {
                                "version": "1.2.0",
                                "checksum": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                                "dependencies": {
                                    "common": {
                                        "source": {
                                            "git": "https://github.com/openwdl/common",
                                            "commit": "d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5"
                                        },
                                        "modules": {
                                            ".": {
                                                "version": "0.3.0",
                                                "checksum": "sha256:4355a46b19d348dc2f57c046f8ef63d4538ebb936000f3c9ee954a27460dd865",
                                                "dependencies": {}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "local_utils": {
                        "source": { "path": "../utils" },
                        "modules": {
                            ".": {
                                "version": "0.5.0",
                                "checksum": "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
                                "dependencies": {}
                            }
                        }
                    }
                }
            }"#,
        )
        .unwrap();

        assert_eq!(l.dependencies.len(), 2);
        let spellbook = l
            .dependencies
            .get(&"spellbook".to_string().try_into().unwrap())
            .unwrap();
        assert!(matches!(spellbook.source, ResolvedSource::Git { .. }));
        let root = spellbook.modules.get(&ModulePath::Root).unwrap();
        assert_eq!(root.version.to_string(), "1.2.0");
        assert_eq!(root.dependencies.len(), 1);
    }

    #[test]
    fn round_trips_lockfile() {
        let original = parse(
            r#"{
                "version": 1,
                "dependencies": {
                    "local_utils": {
                        "source": { "path": "../utils" },
                        "modules": {
                            ".": {
                                "version": "0.5.0",
                                "checksum": "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
                                "dependencies": {}
                            }
                        }
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
                            "commit": "not-a-sha"
                        },
                        "modules": {}
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
                        "modules": {
                            ".": {
                                "version": "0.1.0",
                                "checksum": "md5:abc",
                                "dependencies": {}
                            }
                        }
                    }
                }
            }"#,
        )
        .unwrap_err();
        assert!(matches!(err, LockfileError::InvalidJson(_)));
    }

    #[test]
    fn module_path_round_trips_root() {
        let p: ModulePath = ".".parse().unwrap();
        assert!(matches!(p, ModulePath::Root));
        let s: String = p.into();
        assert_eq!(s, ".");
    }

    #[test]
    fn module_path_round_trips_sub() {
        let p: ModulePath = "csvkit/cut".parse().unwrap();
        let ModulePath::Sub(rel) = &p else {
            panic!("expected `Sub` variant, got `{p:?}`");
        };
        assert_eq!(rel.as_str(), "csvkit/cut");
        let s: String = p.into();
        assert_eq!(s, "csvkit/cut");
    }

    #[test]
    fn module_path_rejects_invalid_paths() {
        for bad in ["", "..", "/abs", "a/.."] {
            assert!(
                bad.parse::<ModulePath>().is_err(),
                "accepted `{bad}` as a `ModulePath`"
            );
        }
    }
}
