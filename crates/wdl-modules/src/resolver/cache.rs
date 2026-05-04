//! On-disk cache layout for resolved modules.

use std::path::Path;
use std::path::PathBuf;

use sha2::Digest;
use sha2::Sha256;
use thiserror::Error;
use url::Url;

use crate::ContentHash;
use crate::GitCommit;

/// The cache layout key for a `(repository, commit)` pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheKey {
    /// The leading directory components, derived from the Git URL.
    prefix: KeyPrefix,
    /// The commit SHA.
    commit: GitCommit,
}

/// The shape of a cache key's leading directory components.
#[derive(Clone, Debug, PartialEq, Eq)]
enum KeyPrefix {
    /// `<host>/<org>/<repo>` derived from a Git URL whose path has at
    /// least two segments.
    Structured {
        /// The host name.
        host: String,
        /// The first path segment (organization or user).
        org: String,
        /// The second path segment (repository name) with `.git` stripped.
        repo: String,
    },
    /// `_opaque/<sha256(url)>` for URLs that don't fit the structured
    /// shape (IP-only hosts, deeply nested groups, etc.).
    Opaque {
        /// Lowercase hex SHA-256 digest of the canonical URL.
        digest_hex: String,
    },
}

impl CacheKey {
    /// Derives a `CacheKey` from a Git URL and a commit SHA.
    pub fn from_url(url: &Url, commit: &GitCommit) -> Self {
        let prefix = match url.host_str() {
            Some(host) => {
                let segments: Vec<&str> = url.path().split('/').filter(|s| !s.is_empty()).collect();
                if segments.len() >= 2 {
                    let repo = segments[1].trim_end_matches(".git").to_string();
                    KeyPrefix::Structured {
                        host: host.to_string(),
                        org: segments[0].to_string(),
                        repo,
                    }
                } else {
                    KeyPrefix::Opaque {
                        digest_hex: hash_url(url),
                    }
                }
            }
            None => KeyPrefix::Opaque {
                digest_hex: hash_url(url),
            },
        };
        Self {
            prefix,
            commit: commit.clone(),
        }
    }

    /// Returns the cache-root-relative path for this key.
    pub(crate) fn relative_path(&self) -> PathBuf {
        let mut p = PathBuf::new();
        match &self.prefix {
            KeyPrefix::Structured { host, org, repo } => {
                p.push(host);
                p.push(org);
                p.push(repo);
            }
            KeyPrefix::Opaque { digest_hex } => {
                p.push("_opaque");
                p.push(digest_hex);
            }
        }
        p.push(self.commit.inner());
        p
    }

    /// Joins the cache key under `cache_root` to produce an absolute
    /// path to the cache leaf.
    pub fn absolute_path(&self, cache_root: &Path) -> PathBuf {
        cache_root.join(self.relative_path())
    }
}

/// Hashes a URL with SHA-256 and returns the lowercase hex digest.
fn hash_url(url: &Url) -> String {
    let mut h = Sha256::new();
    h.update(url.as_str().as_bytes());
    let bytes: [u8; 32] = h.finalize().into();
    hex::encode(bytes)
}

/// Removes the cache leaf at `path`. No-op if the leaf does not exist.
// NOTE: `#[expect(dead_code)]` would error under tests where these items are
// used; cannot expect the lint to fire across all configurations.
#[allow(dead_code)]
pub(crate) fn evict(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Re-hashes a cached module folder and compares against the expected
/// content hash.
// NOTE: `#[expect(dead_code)]` would error under tests where these items are
// used; cannot expect the lint to fire across all configurations.
#[allow(dead_code)]
pub(crate) fn verify_integrity(
    leaf: &Path,
    expected: &ContentHash,
) -> Result<(), IntegrityError> {
    let observed =
        crate::hash::hash_directory(leaf).map_err(|source| IntegrityError::Hash { source })?;
    if observed != *expected {
        return Err(IntegrityError::Mismatch {
            expected: *expected,
            observed,
        });
    }
    Ok(())
}

/// An error produced by [`verify_integrity`].
// NOTE: `#[expect(dead_code)]` would error under tests where these items are
// used; cannot expect the lint to fire across all configurations.
#[allow(dead_code)]
#[derive(Debug, Error)]
pub(crate) enum IntegrityError {
    /// The cached module's content hash does not match the expected
    /// digest.
    #[error("content hash mismatch: expected `{expected}`, observed `{observed}`")]
    Mismatch {
        /// The hash recorded in the lockfile.
        expected: ContentHash,
        /// The hash observed in the cache.
        observed: ContentHash,
    },

    /// Re-hashing the cache leaf failed.
    #[error(transparent)]
    Hash {
        /// The underlying hashing error.
        #[from]
        source: crate::HashError,
    },
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    fn commit() -> GitCommit {
        GitCommit::try_from("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string()).unwrap()
    }

    #[test]
    fn structured_layout_for_github_url() {
        let url = Url::parse("https://github.com/openwdl/tasks").unwrap();
        let key = CacheKey::from_url(&url, &commit());
        let parts: Vec<_> = key
            .relative_path()
            .iter()
            .map(|c| c.to_str().unwrap().to_string())
            .collect();
        assert_eq!(
            parts,
            vec![
                "github.com",
                "openwdl",
                "tasks",
                "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
            ]
        );
    }

    #[test]
    fn opaque_layout_when_url_lacks_org_repo() {
        let url = Url::parse("https://example.com/").unwrap();
        let key = CacheKey::from_url(&url, &commit());
        let parts: Vec<_> = key
            .relative_path()
            .iter()
            .map(|c| c.to_str().unwrap().to_string())
            .collect();
        assert_eq!(parts[0], "_opaque");
        assert_eq!(parts[2], "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2");
        assert_eq!(parts[1].len(), 64);
        assert!(parts[1].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn strips_dot_git_suffix() {
        let url = Url::parse("https://github.com/openwdl/tasks.git").unwrap();
        let key = CacheKey::from_url(&url, &commit());
        let parts: Vec<_> = key
            .relative_path()
            .iter()
            .map(|c| c.to_str().unwrap().to_string())
            .collect();
        assert_eq!(parts[2], "tasks");
    }

    #[test]
    fn evict_removes_leaf() {
        let dir = tempdir().unwrap();
        let leaf = dir.path().join("leaf");
        fs::create_dir_all(&leaf).unwrap();
        fs::write(leaf.join("file"), b"x").unwrap();
        evict(&leaf).unwrap();
        assert!(!leaf.exists());
    }

    #[test]
    fn evict_is_noop_when_missing() {
        let dir = tempdir().unwrap();
        evict(&dir.path().join("never-existed")).unwrap();
    }

    #[test]
    fn verify_integrity_passes_on_match() {
        let dir = tempdir().unwrap();
        let leaf = dir.path().join("leaf");
        fs::create_dir_all(&leaf).unwrap();
        fs::write(leaf.join("a.wdl"), b"hello").unwrap();
        let hash = crate::hash::hash_directory(&leaf).unwrap();
        verify_integrity(&leaf, &hash).unwrap();
    }

    #[test]
    fn verify_integrity_fails_on_mismatch() {
        let dir = tempdir().unwrap();
        let leaf = dir.path().join("leaf");
        fs::create_dir_all(&leaf).unwrap();
        fs::write(leaf.join("a.wdl"), b"hello").unwrap();
        let bad: ContentHash =
            "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                .parse()
                .unwrap();
        let err = verify_integrity(&leaf, &bad).unwrap_err();
        assert!(matches!(err, IntegrityError::Mismatch { .. }));
    }
}
