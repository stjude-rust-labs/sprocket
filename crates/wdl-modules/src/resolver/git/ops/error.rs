//! Git operation errors.

use std::path::PathBuf;

use thiserror::Error;

/// Errors produced by the `git` module.
#[derive(Debug, Error)]
pub enum GitError {
    /// The remote could not be connected.
    #[error("failed to connect to the Git remote at `{url}`")]
    Connect {
        /// The remote URL.
        url: String,
        /// The underlying libgit2 error.
        #[source]
        source: git2::Error,
    },
    /// The remote's advertised refs could not be listed.
    #[error("failed to list refs advertised by the Git remote at `{url}`")]
    ListRefs {
        /// The remote URL.
        url: String,
        /// The underlying libgit2 error.
        #[source]
        source: git2::Error,
    },
    /// The repository could not be cloned.
    #[error("failed to clone the Git repository at `{url}`")]
    Clone {
        /// The remote URL.
        url: String,
        /// The underlying libgit2 error.
        #[source]
        source: git2::Error,
    },
    /// The requested commit could not be fetched.
    #[error("failed to fetch commit `{commit}` from the Git remote at `{url}`")]
    FetchCommit {
        /// The remote URL.
        url: String,
        /// The requested commit identifier.
        commit: String,
        /// The underlying libgit2 error.
        #[source]
        source: git2::Error,
    },
    /// The working tree could not be checked out.
    #[error("failed to check out the Git working tree at `{path}`")]
    Checkout {
        /// The working tree path.
        path: PathBuf,
        /// The underlying libgit2 error.
        #[source]
        source: git2::Error,
    },
    /// A Git object could not be read.
    #[error("failed to read a Git object")]
    Object {
        /// The underlying libgit2 error.
        #[source]
        source: git2::Error,
    },
    /// Authentication failed for a remote operation.
    #[error("authentication failed for the Git remote at `{url}`{suffix}", suffix = if *credentials_disabled { "; credentials were withheld by resolver policy; check credential settings and `allowed_transitive_hosts`" } else { "" })]
    Auth {
        /// The remote URL.
        url: String,
        /// Whether credentials were disabled by resolver policy.
        credentials_disabled: bool,
        /// The underlying authentication error.
        #[source]
        source: git2::Error,
    },
    /// A Git transfer exceeded its configured byte limit.
    #[error(
        "the Git transfer from `{url}` exceeded the limit of {limit} bytes after receiving \
         {received} bytes; raise `max_transfer_bytes`"
    )]
    TransferLimitExceeded {
        /// The remote URL.
        url: String,
        /// The configured byte limit.
        limit: u64,
        /// Bytes received before aborting.
        received: u64,
    },
    /// A filesystem operation failed.
    #[error("i/o error at `{path}`")]
    Io {
        /// The path involved.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Sparse-checkout metadata could not be serialized or parsed.
    #[error("sparse-checkout metadata error at `{path}`")]
    Json {
        /// The metadata path.
        path: PathBuf,
        /// The underlying JSON error.
        #[source]
        source: serde_json::Error,
    },
    /// A cache leaf has no parent directory.
    #[error("cache leaf path `{0}` has no parent directory")]
    RootLeaf(PathBuf),
    /// A cache root failed an ownership or safety check.
    #[error("unsafe WDL module cache root `{path}`; {reason}")]
    UnsafeCacheRoot {
        /// The cache root path.
        path: PathBuf,
        /// The reason the root is unsafe.
        reason: &'static str,
    },
    /// A remote advertised too many refs.
    #[error("remote at `{url}` advertised {count} refs, exceeding the limit of {limit}")]
    RefLimitExceeded {
        /// The remote URL.
        url: String,
        /// The number of advertised refs.
        count: usize,
        /// The configured ref limit.
        limit: usize,
    },
    /// A module subtree exceeded configured limits.
    #[error("module subtree `{path}` exceeds tree limits (files: {files}, bytes: {bytes}, max_files: {}, max_bytes: {})", max_files.map(|v| v.to_string()).as_deref().unwrap_or("unlimited"), max_bytes.map(|v| v.to_string()).as_deref().unwrap_or("unlimited"))]
    TreeLimitExceeded {
        /// The module path.
        path: String,
        /// The observed file count.
        files: usize,
        /// The observed byte count.
        bytes: u64,
        /// The configured file limit.
        max_files: Option<usize>,
        /// The configured byte limit.
        max_bytes: Option<u64>,
    },
    /// The remote did not advertise a default branch.
    #[error("remote at `{url}` did not advertise a default branch")]
    NoDefaultBranch {
        /// The remote URL.
        url: String,
    },
    /// The advertised default branch was not valid UTF-8.
    #[error("remote at `{url}` advertised a non-UTF-8 default branch name")]
    DefaultBranchUtf8 {
        /// The remote URL.
        url: String,
        /// The underlying UTF-8 error.
        #[source]
        source: std::str::Utf8Error,
    },
    /// A commit prefix did not identify one commit.
    #[error("commit prefix `{prefix}` in `{url}` did not resolve to a unique commit")]
    CommitPrefix {
        /// The remote URL.
        url: String,
        /// The unresolved commit prefix.
        prefix: String,
    },
}

/// Classifies an authentication error and delegates all other errors.
///
/// Authentication errors record whether credentials were disabled by policy.
pub(crate) fn classify(
    url: &url::Url,
    policy: super::creds::FetchPolicy,
    error: git2::Error,
    fallback: impl FnOnce(git2::Error) -> GitError,
) -> GitError {
    if error.code() == git2::ErrorCode::Auth {
        GitError::Auth {
            url: url.to_string(),
            credentials_disabled: policy.credentials == super::creds::CredentialMode::Disabled,
            source: error,
        }
    } else {
        fallback(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::git::ops::CredentialMode;
    use crate::resolver::git::ops::FetchPolicy;
    fn policy(credentials: CredentialMode) -> FetchPolicy {
        FetchPolicy {
            credentials,
            max_transfer_bytes: None,
        }
    }

    #[test]
    fn auth_display_mentions_withheld_credentials() {
        let error = git2::Error::new(git2::ErrorCode::Auth, git2::ErrorClass::Net, "denied");
        let classified = classify(
            &url::Url::parse("https://example.test/repo").unwrap(),
            policy(CredentialMode::Disabled),
            error,
            |source| GitError::Object { source },
        );
        assert_eq!(
            classified.to_string(),
            "authentication failed for the Git remote at `https://example.test/repo`; credentials \
             were withheld by resolver policy; check credential settings and \
             `allowed_transitive_hosts`"
        );
    }

    #[test]
    fn auth_display_omits_withheld_credentials_suffix_when_enabled() {
        let error = git2::Error::new(git2::ErrorCode::Auth, git2::ErrorClass::Net, "denied");
        let classified = classify(
            &url::Url::parse("https://example.test/repo").unwrap(),
            policy(CredentialMode::Enabled),
            error,
            |source| GitError::Object { source },
        );
        assert_eq!(
            classified.to_string(),
            "authentication failed for the Git remote at `https://example.test/repo`"
        );
    }

    #[test]
    fn classify_uses_fallback_for_non_auth_errors() {
        let error = git2::Error::new(
            git2::ErrorCode::NotFound,
            git2::ErrorClass::Object,
            "missing",
        );
        let classified = classify(
            &url::Url::parse("https://example.test/repo").unwrap(),
            policy(CredentialMode::Enabled),
            error,
            |source| GitError::Connect {
                url: "x".into(),
                source,
            },
        );
        assert!(matches!(classified, GitError::Connect { .. }));
    }
}
