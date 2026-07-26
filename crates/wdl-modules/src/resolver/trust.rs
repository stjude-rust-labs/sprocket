//! User-level trust store at `<config>/sprocket/modules-trust.toml`.
//!
//! This state lives outside any module project. Callers choose the config path
//! to read or write, and lockfile signer acceptance never writes inside the
//! project tree.

use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;

use tempfile::NamedTempFile;
use thiserror::Error;
use toml_spanner::Toml;

use crate::lockfile::DependencyMap;
use crate::lockfile::Lockfile;
use crate::signing::SignerIdentity;
use crate::signing::VerifyingKey;

/// An error reading or writing the user-level trust store.
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
///
/// This file is separate from `module.json` and `module-lock.json`, so trusted
/// signer state remains outside the project tree.
#[derive(Clone, Debug, Default, Eq, PartialEq, Toml)]
#[toml(Toml)]
pub struct TrustStore {
    /// The globally trusted signer public keys stored in
    /// `modules-trust.toml`.
    #[toml(default, rename = "trust", skip_if = Vec::is_empty)]
    pub keys: Vec<VerifyingKey>,
    /// Optional signer identity metadata keyed by public key in the same
    /// trust-store file.
    #[toml(default, rename = "identity", skip_if = Vec::is_empty)]
    pub identities: Vec<TrustedIdentity>,
}

/// Optional human metadata associated with one trusted key in the user-level
/// trust store.
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
    /// Reads the trust store from `path`.
    ///
    /// This returns the default empty store when `path` does not exist.
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

    /// Returns `true` when `key` is globally trusted in this user-level store.
    pub fn contains_key(&self, key: &VerifyingKey) -> bool {
        self.keys.contains(key)
    }

    /// Adds `key` if it is not already trusted in `modules-trust.toml`.
    pub fn insert_key(&mut self, key: VerifyingKey) -> bool {
        if self.contains_key(&key) {
            return false;
        }
        self.keys.push(key);
        self.keys.sort_by_key(VerifyingKey::to_openssh);
        true
    }

    /// Adds signer trust for `key` and records authenticated identity metadata
    /// when present.
    ///
    /// This updates only the in-memory trust store. Call [`Self::save`] to
    /// persist the user-level trust file.
    pub fn trust_signer(&mut self, key: VerifyingKey, identity: Option<SignerIdentity>) -> bool {
        let inserted = self.insert_key(key);
        match identity {
            Some(SignerIdentity::Signer { name, email }) => {
                self.upsert_identity(key, Some(name), Some(email), None);
            }
            Some(SignerIdentity::Comment { comment }) => {
                self.upsert_identity(key, None, None, Some(comment));
            }
            None => {}
        }
        inserted
    }

    /// Removes `key` from the in-memory trust store.
    pub fn remove_key(&mut self, key: &VerifyingKey) -> bool {
        let before = self.keys.len();
        self.keys.retain(|trusted| trusted != key);
        self.identities.retain(|identity| &identity.key != key);
        self.keys.len() != before
    }

    /// Removes every trusted key and its identity metadata from memory.
    pub fn clear(&mut self) {
        self.keys.clear();
        self.identities.clear();
    }

    /// Iterates over globally trusted signer keys from this user-level store.
    pub fn trusted_keys(&self) -> impl Iterator<Item = &VerifyingKey> {
        self.keys.iter()
    }

    /// Upserts optional metadata for a trusted key in memory.
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

    /// Returns optional metadata for a trusted key from this store.
    pub fn identity(&self, key: &VerifyingKey) -> Option<&TrustedIdentity> {
        self.identities.iter().find(|identity| &identity.key == key)
    }

    /// Trusts signer keys recorded across a lockfile dependency tree.
    ///
    /// This reads signer information from the supplied `module-lock.json`
    /// model, inserts any new keys into the in-memory user-level store, and
    /// returns the number of newly inserted keys. It does not modify the
    /// project manifest or lockfile.
    ///
    /// Call [Self::save] to persist the user-level trust file.
    pub fn trust_lockfile_signers(&mut self, lockfile: &Lockfile) -> usize {
        fn visit(store: &mut TrustStore, dependencies: &DependencyMap) -> usize {
            dependencies
                .values()
                .map(|dependency| {
                    usize::from(
                        dependency
                            .signer
                            .is_some_and(|key| store.trust_signer(key, None)),
                    ) + visit(store, &dependency.dependencies)
                })
                .sum()
        }

        visit(self, &lockfile.dependencies)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::lockfile::DependencyEntry;
    use crate::lockfile::DependencyMap;
    use crate::lockfile::Lockfile;
    use crate::lockfile::ResolvedSource;
    use crate::signing::SignerIdentity;

    fn test_key() -> VerifyingKey {
        crate::signing::test_utils::signing_key_from_seed(0xA7).verifying_key()
    }

    fn signed_lockfile_with_nested_duplicate(key: VerifyingKey) -> Lockfile {
        let nested = DependencyEntry {
            source: ResolvedSource::Git {
                git: "https://example.com/nested".parse().unwrap(),
                sha: "0000000000000000000000000000000000000000".parse().unwrap(),
                selector: crate::dependency::GitSelector::Version("^1".parse().unwrap()),
                path: None,
            },
            checksum: Some(
                "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                    .parse()
                    .unwrap(),
            ),
            signer: Some(key),
            dependencies: DependencyMap::new(),
        };

        let mut dependencies = DependencyMap::new();
        dependencies.insert(
            "root".parse().unwrap(),
            DependencyEntry {
                source: ResolvedSource::Git {
                    git: "https://example.com/root".parse().unwrap(),
                    sha: "1111111111111111111111111111111111111111".parse().unwrap(),
                    selector: crate::dependency::GitSelector::Version("^1".parse().unwrap()),
                    path: None,
                },
                checksum: Some(
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                        .parse()
                        .unwrap(),
                ),
                signer: Some(key),
                dependencies: DependencyMap::from([("nested".parse().unwrap(), nested)]),
            },
        );
        Lockfile {
            version: crate::lockfile::LOCKFILE_VERSION,
            dependencies,
        }
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
    fn trust_signer_adds_key_and_identity() {
        let key = test_key();
        let mut store = TrustStore::default();

        assert!(store.trust_signer(
            key,
            Some(SignerIdentity::Signer {
                name: "Ada".to_string(),
                email: "ada@example.com".to_string(),
            }),
        ));
        assert!(!store.trust_signer(key, None));
        // SAFETY: `trust_signer` inserted the key and structured identity above.
        let identity = store.identity(&key).unwrap();
        assert_eq!(identity.name.as_deref(), Some("Ada"));
        assert_eq!(identity.email.as_deref(), Some("ada@example.com"));
    }

    #[test]
    fn trust_lockfile_signers_recurses_and_deduplicates() {
        let key = test_key();
        let lockfile = signed_lockfile_with_nested_duplicate(key);
        let mut store = TrustStore::default();

        assert_eq!(store.trust_lockfile_signers(&lockfile), 1);
        assert_eq!(store.trust_lockfile_signers(&lockfile), 0);
        assert!(store.contains_key(&key));
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
