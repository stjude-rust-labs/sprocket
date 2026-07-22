//! User-level trust store at `<config>/sprocket/modules-trust.toml`.

use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;

use tempfile::NamedTempFile;
use thiserror::Error;
use toml_spanner::Toml;

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
        source: toml_spanner::FromTomlError,
    },
    /// TOML serialization error.
    #[error("failed to serialize trust store for `{path}`")]
    Serialize {
        /// The target path.
        path: PathBuf,
        /// The underlying serialization error.
        #[source]
        source: toml_spanner::ToTomlError,
    },
}

/// The user-level trust store loaded from `modules-trust.toml`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Toml)]
#[toml(Toml)]
pub struct TrustStore {
    /// The globally trusted signer public keys.
    #[toml(default, rename = "trust", skip_if = Vec::is_empty)]
    pub keys: Vec<VerifyingKey>,
    /// Optional signer identity metadata keyed by public key.
    #[toml(default, rename = "identity", skip_if = Vec::is_empty)]
    pub identities: Vec<TrustedIdentity>,
}

/// Optional human metadata associated with a trusted key.
#[derive(Clone, Debug, Eq, PartialEq, Toml)]
#[toml(Toml)]
pub struct TrustedIdentity {
    /// The public key this identity describes.
    pub key: VerifyingKey,
    /// Optional display name for the key owner.
    #[toml(default, skip_if = Option::is_none)]
    pub name: Option<String>,
    /// Optional email for the key owner.
    #[toml(default, skip_if = Option::is_none)]
    pub email: Option<String>,
    /// Optional unstructured OpenSSH public key comment.
    #[toml(default, skip_if = Option::is_none)]
    pub comment: Option<String>,
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
        toml_spanner::from_str(s).map_err(|source| TrustStoreError::Parse {
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
        let s = toml_spanner::to_string(self).map_err(|source| TrustStoreError::Serialize {
            path: path.to_path_buf(),
            source,
        })?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut temp = NamedTempFile::new_in(parent).map_err(|source| TrustStoreError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
        temp.write_all(s.as_bytes())
            .and_then(|()| temp.as_file().sync_all())
            .map_err(|source| TrustStoreError::Io {
                path: temp.path().to_path_buf(),
                source,
            })?;
        temp.persist(path).map_err(|error| TrustStoreError::Io {
            path: path.to_path_buf(),
            source: error.error,
        })?;
        Ok(())
    }

    /// Returns `true` when `key` is globally trusted.
    pub fn contains_key(&self, key: &VerifyingKey) -> bool {
        self.keys.contains(key)
    }

    /// Adds `key` if it is not already trusted.
    pub fn insert_key(&mut self, key: VerifyingKey) -> bool {
        if self.contains_key(&key) {
            return false;
        }
        self.keys.push(key);
        self.keys.sort_by_key(VerifyingKey::to_openssh);
        true
    }

    /// Removes `key` from the trust store.
    pub fn remove_key(&mut self, key: &VerifyingKey) -> bool {
        let before = self.keys.len();
        self.keys.retain(|trusted| trusted != key);
        self.identities.retain(|identity| &identity.key != key);
        self.keys.len() != before
    }

    /// Removes every trusted key and its identity metadata.
    pub fn clear(&mut self) {
        self.keys.clear();
        self.identities.clear();
    }

    /// Iterates over globally trusted signer keys.
    pub fn trusted_keys(&self) -> impl Iterator<Item = &VerifyingKey> {
        self.keys.iter()
    }

    /// Upserts optional metadata for a trusted key.
    pub fn upsert_identity(
        &mut self,
        key: VerifyingKey,
        name: Option<String>,
        email: Option<String>,
        comment: Option<String>,
    ) {
        if name.is_none() && email.is_none() && comment.is_none() {
            return;
        }

        if let Some(existing) = self
            .identities
            .iter_mut()
            .find(|identity| identity.key == key)
        {
            if let Some(comment) = comment {
                existing.name = None;
                existing.email = None;
                existing.comment = Some(comment);
                return;
            }
            existing.comment = None;
            if let Some(name) = name {
                existing.name = Some(name);
            }
            if let Some(email) = email {
                existing.email = Some(email);
            }
            return;
        }

        let (name, email) = if comment.is_some() {
            (None, None)
        } else {
            (name, email)
        };
        self.identities.push(TrustedIdentity {
            key,
            name,
            email,
            comment,
        });
        self.identities
            .sort_by_key(|identity| identity.key.to_openssh());
    }

    /// Returns optional metadata for a trusted key.
    pub fn identity(&self, key: &VerifyingKey) -> Option<&TrustedIdentity> {
        self.identities.iter().find(|identity| &identity.key == key)
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
        let store: TrustStore = toml_spanner::from_str("").unwrap();
        assert!(store.keys.is_empty());
    }

    #[test]
    fn round_trips_via_toml() {
        let key = test_key();
        let mut store = TrustStore::default();
        store.insert_key(key);
        let s = toml_spanner::to_string(&store).unwrap();
        assert!(s.contains("trust = ["));
        assert!(!s.contains("key ="));
        let parsed: TrustStore = toml_spanner::from_str(&s).unwrap();
        assert_eq!(parsed.keys.len(), 1);
        assert!(parsed.contains_key(&key));
    }

    #[test]
    fn loads_default_when_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("trust.toml");
        let store = TrustStore::load_or_default(&path).unwrap();
        assert!(store.keys.is_empty());
    }

    #[test]
    fn save_and_reload_round_trips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested").join("trust.toml");
        let key = test_key();
        let mut store = TrustStore::default();
        store.insert_key(key);
        store.save(&path).unwrap();
        assert!(path.exists());

        let reloaded = TrustStore::load_or_default(&path).unwrap();
        assert_eq!(
            reloaded, store,
            "reloaded store should exactly match the original"
        );
    }

    #[test]
    fn insert_and_remove_key() {
        let key = test_key();
        let mut store = TrustStore::default();
        assert!(store.insert_key(key));
        assert!(!store.insert_key(key));
        assert!(store.contains_key(&key));
        assert!(store.remove_key(&key));
        assert!(!store.remove_key(&key));
    }

    #[test]
    fn clear_removes_keys_and_identities() {
        let key = test_key();
        let mut store = TrustStore::default();
        store.insert_key(key);
        store.upsert_identity(key, Some("Alice".to_string()), None, None);
        store.clear();
        assert!(store.keys.is_empty());
        assert!(
            store.identities.is_empty(),
            "clearing the store must not orphan identity metadata"
        );
    }

    #[test]
    fn comment_identity_round_trips() {
        let key = test_key();
        let mut store = TrustStore::default();
        store.insert_key(key);
        store.upsert_identity(key, None, None, Some("release signer".to_string()));
        // SAFETY: the in-memory trust store contains only serializable values.
        let encoded = toml_spanner::to_string(&store).unwrap();
        // SAFETY: `encoded` was produced from a valid trust store.
        let decoded: TrustStore = toml_spanner::from_str(&encoded).unwrap();
        // SAFETY: the identity was inserted above and survives serialization.
        let identity = decoded.identity(&key).unwrap();

        assert_eq!(identity.comment.as_deref(), Some("release signer"));
        assert!(identity.name.is_none());
        assert!(identity.email.is_none());
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
