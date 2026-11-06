//! On-disk cache layout for resolved modules.

use std::path::Path;
use std::path::PathBuf;

use sha2::Digest;
use sha2::Sha256;
use url::Url;

use crate::lockfile::GitCommit;

/// The cache layout key for a `(repository, commit)` pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheKey {
    /// The leading directory components, derived from the Git URL.
    prefix: PrefixKey,
    /// The commit SHA.
    commit: GitCommit,
}

/// The shape of a cache key's leading directory components.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PrefixKey {
    /// `<host>/<org>/<repo>` derived from a Git URL whose path has at
    /// least two segments.
    GitStructured {
        /// The host name.
        host: String,
        /// The first path segment (organization or user).
        org: String,
        /// `<repo>-<digest8>` where `digest8` is the first 8 hex chars
        /// of the canonical URL SHA-256. The suffix eliminates
        /// collisions between nested repository URLs (e.g.,
        /// `gitlab/x/y` vs `gitlab/x/y/z`) while keeping a
        /// human-readable prefix.
        repo_with_suffix: String,
    },
    /// `_opaque/<sha256(url)>` for URLs without a host or with fewer than
    /// two path segments.
    GitOpaque {
        /// Lowercase hex SHA-256 digest of the canonical URL.
        digest_hex: String,
    },
}

impl CacheKey {
    /// Derives a `CacheKey` from a Git URL and a commit SHA.
    pub fn from_git_url(url: &Url, commit: &GitCommit) -> Self {
        let prefix = match url.host_str() {
            Some(host) => {
                let mut segments = url.path().split('/').filter(|s| !s.is_empty());
                match (segments.next(), segments.next()) {
                    (Some(org), Some(repo)) => {
                        let repo = repo.strip_suffix(".git").unwrap_or(repo);
                        let digest = hash_url(url);
                        let repo_with_suffix = format!("{repo}-{}", &digest[..8]);
                        PrefixKey::GitStructured {
                            host: host.to_string(),
                            org: org.to_string(),
                            repo_with_suffix,
                        }
                    }
                    _ => PrefixKey::GitOpaque {
                        digest_hex: hash_url(url),
                    },
                }
            }
            None => PrefixKey::GitOpaque {
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
            PrefixKey::GitStructured {
                host,
                org,
                repo_with_suffix,
            } => {
                p.push(host);
                p.push(org);
                p.push(repo_with_suffix);
            }
            PrefixKey::GitOpaque { digest_hex } => {
                p.push("_opaque");
                p.push(digest_hex);
            }
        }
        p.push(self.commit.as_str());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn commit() -> GitCommit {
        GitCommit::try_from("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string()).unwrap()
    }

    #[test]
    fn structured_layout_for_github_url() {
        let url = Url::parse("https://github.com/openwdl/tasks").unwrap();
        let key = CacheKey::from_git_url(&url, &commit());
        let parts: Vec<_> = key
            .relative_path()
            .iter()
            .map(|c| c.to_str().unwrap().to_string())
            .collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "github.com");
        assert_eq!(parts[1], "openwdl");
        assert!(
            parts[2].starts_with("tasks-"),
            "expected `tasks-<digest8>`, got: {parts:?}"
        );
        assert_eq!(parts[3], "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2");
    }

    #[test]
    fn opaque_layout_when_url_lacks_org_repo() {
        let url = Url::parse("https://example.com/").unwrap();
        let key = CacheKey::from_git_url(&url, &commit());
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
        let key = CacheKey::from_git_url(&url, &commit());
        let parts: Vec<_> = key
            .relative_path()
            .iter()
            .map(|c| c.to_str().unwrap().to_string())
            .collect();
        assert!(parts[2].starts_with("tasks-"), "got: {parts:?}");
    }

    #[test]
    fn nested_repository_urls_do_not_collide() {
        let url_short = Url::parse("https://gitlab.example/x/y").unwrap();
        let url_long = Url::parse("https://gitlab.example/x/y/z").unwrap();
        let k_short = CacheKey::from_git_url(&url_short, &commit());
        let k_long = CacheKey::from_git_url(&url_long, &commit());
        assert_ne!(
            k_short.relative_path(),
            k_long.relative_path(),
            "nested repository URLs must produce distinct cache keys"
        );
    }
}
