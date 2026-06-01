//! User-level trust store at `<config>/sprocket/modules-trust.toml`.

use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::dependency::DependencyName;
use crate::signing::VerifyingKey;

/// An error reading or writing the trust store.
#[derive(Debug, Error)]
pub enum TrustStoreError {
    /// I/O error.
    #[error("i/o error at `{path}`")]
    Io {
        /// The path involved.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// File is not valid UTF-8.
    #[error("trust store at `{path}` is not valid UTF-8")]
    NonUtf8 {
        /// The offending file.
        path: PathBuf,
    },
    /// TOML parse error.
    #[error("trust store at `{path}` is not valid TOML")]
    Parse {
        /// The offending file.
        path: PathBuf,
        /// The underlying parse error.
        #[source]
        source: toml::de::Error,
    },
    /// TOML serialization error.
    #[error("failed to serialize trust store for `{path}`")]
    Serialize {
        /// The target path.
        path: PathBuf,
        /// The underlying serialization error.
        #[source]
        source: toml::ser::Error,
    },
}

/// The user-level trust store loaded from `modules-trust.toml`.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrustStore {
    /// The list of explicit trust entries.
    #[serde(default, rename = "trust", skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<TrustEntry>,
}

/// One explicit trust entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrustEntry {
    /// The dependency this entry covers.
    pub dep: DependencyName,
    /// The Git URL or local path that this trust pin applies to.
    pub source: String,
    /// The sub-path within the source repository. `None` when the
    /// module sits at the repository root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The required signer public key in OpenSSH format.
    pub key: VerifyingKey,
}

impl TrustStore {
    /// Reads the trust store from `path`, or returns the default (empty)
    /// store if the file does not exist.
    pub fn load_or_default(path: &Path) -> Result<Self, TrustStoreError> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => {
                return Err(TrustStoreError::Io {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        let s = std::str::from_utf8(&bytes).map_err(|_| TrustStoreError::NonUtf8 {
            path: path.to_path_buf(),
        })?;
        toml::from_str(s).map_err(|source| TrustStoreError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Writes the trust store to `path`, creating any missing parent
    /// directories.
    pub fn save(&self, path: &Path) -> Result<(), TrustStoreError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|source| TrustStoreError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let s = toml::to_string_pretty(self).map_err(|source| TrustStoreError::Serialize {
            path: path.to_path_buf(),
            source,
        })?;
        std::fs::write(path, s).map_err(|source| TrustStoreError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Looks up an explicit trust entry for the given dependency,
    /// source URL, and sub-path within the source.
    pub fn lookup(
        &self,
        dep: &DependencyName,
        source_url: &str,
        path: Option<&str>,
    ) -> Option<&VerifyingKey> {
        self.entries
            .iter()
            .find(|e| e.dep == *dep && e.source == source_url && e.path.as_deref() == path)
            .map(|e| &e.key)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    fn test_key() -> VerifyingKey {
        crate::signing::test_utils::signing_key_from_seed(0xA7).verifying_key()
    }

    #[test]
    fn parses_empty_file() {
        let store: TrustStore = toml::from_str("").unwrap();
        assert!(store.entries.is_empty());
    }

    const TEST_SOURCE: &str = "https://github.com/openwdl/tasks";

    #[test]
    fn round_trips_via_toml() {
        let dep = DependencyName::try_from("openwdl".to_string()).unwrap();
        let key = test_key();
        let store = TrustStore {
            entries: vec![TrustEntry {
                dep: dep.clone(),
                source: TEST_SOURCE.to_string(),
                path: None,
                key,
            }],
        };
        let s = toml::to_string_pretty(&store).unwrap();
        let parsed: TrustStore = toml::from_str(&s).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        assert!(parsed.lookup(&dep, TEST_SOURCE, None).is_some());
    }

    #[test]
    fn loads_default_when_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("trust.toml");
        let store = TrustStore::load_or_default(&path).unwrap();
        assert!(store.entries.is_empty());
    }

    #[test]
    fn save_and_reload_round_trips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested").join("trust.toml");
        let dep = DependencyName::try_from("openwdl".to_string()).unwrap();
        let key = test_key();
        let store = TrustStore {
            entries: vec![TrustEntry {
                dep: dep.clone(),
                source: TEST_SOURCE.to_string(),
                path: None,
                key,
            }],
        };
        store.save(&path).unwrap();
        assert!(path.exists());

        let reloaded = TrustStore::load_or_default(&path).unwrap();
        assert_eq!(
            reloaded, store,
            "reloaded store should exactly match the original"
        );
    }

    #[test]
    fn lookup_requires_matching_source() {
        let dep = DependencyName::try_from("openwdl".to_string()).unwrap();
        let store = TrustStore {
            entries: vec![TrustEntry {
                dep: dep.clone(),
                source: TEST_SOURCE.to_string(),
                path: None,
                key: test_key(),
            }],
        };
        assert!(store.lookup(&dep, TEST_SOURCE, None).is_some());
        assert!(
            store
                .lookup(&dep, "https://example.com/other", None)
                .is_none(),
            "trust pin for one source should not match a different source"
        );
    }

    #[test]
    fn lookup_distinguishes_paths_within_same_source() {
        let dep = DependencyName::try_from("dep".to_string()).unwrap();
        let store = TrustStore {
            entries: vec![TrustEntry {
                dep: dep.clone(),
                source: TEST_SOURCE.to_string(),
                path: Some("csvcut".to_string()),
                key: test_key(),
            }],
        };
        assert!(
            store.lookup(&dep, TEST_SOURCE, Some("csvcut")).is_some(),
            "exact path match should succeed"
        );
        assert!(
            store.lookup(&dep, TEST_SOURCE, Some("csvgrep")).is_none(),
            "trust pin for `csvcut` should not match `csvgrep` in the same repo"
        );
        assert!(
            store.lookup(&dep, TEST_SOURCE, None).is_none(),
            "trust pin for `csvcut` should not match root-level module in the same repo"
        );
    }

    #[test]
    fn lookup_matches_local_path_source() {
        let dep = DependencyName::try_from("utils".to_string()).unwrap();
        let local_source = "/home/user/projects/shared/utils";
        let store = TrustStore {
            entries: vec![TrustEntry {
                dep: dep.clone(),
                source: local_source.to_string(),
                path: None,
                key: test_key(),
            }],
        };
        assert!(
            store.lookup(&dep, local_source, None).is_some(),
            "local-path trust entry should match"
        );
        assert!(
            store
                .lookup(&dep, "/home/user/projects/other/utils", None)
                .is_none(),
            "different local path should not match"
        );
    }

    #[test]
    fn parse_error_names_path() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        fs::write(&path, b"not valid toml [[[ {").unwrap();
        let err = TrustStore::load_or_default(&path).unwrap_err();
        assert!(err.to_string().contains(path.to_str().unwrap()));
    }
}
