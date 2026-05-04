//! User-level trust store at `<config>/sprocket/modules-trust.toml`.

use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::DependencyName;
use crate::VerifyingKey;

/// The user-level trust store loaded from `modules-trust.toml`.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TrustStore {
    /// The list of explicit trust entries.
    #[serde(default, rename = "trust", skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<TrustEntry>,
}

/// One explicit trust entry.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TrustEntry {
    /// The dependency this entry covers.
    pub dep: DependencyName,
    /// The required signer public key in OpenSSH format.
    pub key: VerifyingKey,
}

impl TrustStore {
    /// Reads the trust store from `path`. Returns the default (empty) store
    /// if the file does not exist.
    pub fn load(path: &Path) -> Result<Self, TrustStoreError> {
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

    /// Looks up an explicit trust entry for the given dependency.
    pub fn lookup(&self, dep: &DependencyName) -> Option<&VerifyingKey> {
        self.entries.iter().find(|e| &e.dep == dep).map(|e| &e.key)
    }

    /// Returns the default user-level trust store path under the same
    /// config root that `sprocket.toml` uses.
    ///
    /// On macOS this is `$HOME/.config/sprocket/modules-trust.toml`; on
    /// Linux it follows `$XDG_CONFIG_HOME` (typically
    /// `~/.config/sprocket/modules-trust.toml`); on Windows it lands in
    /// `%APPDATA%/sprocket/modules-trust.toml`.
    pub fn default_path() -> Option<PathBuf> {
        sprocket_config_dir().map(|d| d.join("modules-trust.toml"))
    }
}

/// Returns the user-level Sprocket config directory, mirroring the
/// resolution `sprocket.toml` uses.
fn sprocket_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let base = dirs::home_dir().map(|p| p.join(".config"));
    #[cfg(not(target_os = "macos"))]
    let base = dirs::config_dir();
    base.map(|d| d.join("sprocket"))
}

/// An error reading or writing the trust store.
#[derive(Debug, Error)]
pub enum TrustStoreError {
    /// I/O error.
    #[error("I/O error at `{path}`")]
    Io {
        /// The path involved.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// File contains non-UTF-8 bytes.
    #[error("trust store at `{path}` contains non-UTF-8 bytes")]
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

    #[test]
    fn round_trips_via_toml() {
        let dep = DependencyName::try_from("openwdl".to_string()).unwrap();
        let key = test_key();
        let store = TrustStore {
            entries: vec![TrustEntry {
                dep: dep.clone(),
                key,
            }],
        };
        let s = toml::to_string_pretty(&store).unwrap();
        let parsed: TrustStore = toml::from_str(&s).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        assert!(parsed.lookup(&dep).is_some());
    }

    #[test]
    fn loads_default_when_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("trust.toml");
        let store = TrustStore::load(&path).unwrap();
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
                key,
            }],
        };
        store.save(&path).unwrap();
        assert!(path.exists());

        let reloaded = TrustStore::load(&path).unwrap();
        assert!(reloaded.lookup(&dep).is_some());
    }

    #[test]
    fn parse_error_names_path() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        fs::write(&path, b"not valid toml [[[ {").unwrap();
        let err = TrustStore::load(&path).unwrap_err();
        assert!(err.to_string().contains(path.to_str().unwrap()));
    }
}
