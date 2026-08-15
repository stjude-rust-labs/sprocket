//! Git remote connection and reference discovery operations.

use std::path::Path;

use url::Url;

use super::GitError;
use super::creds::default_callbacks;
use super::creds::default_fetch_options;
use super::error::classify;

/// Creates a detached remote at `url` and connects it in the given
/// `direction` using [`default_callbacks`]. Proxy is disabled
/// (`GIT_PROXY_NONE`). The caller is responsible for `disconnect`ing
/// (via [`disconnect_remote`]) when finished.
pub(crate) fn connect_remote(
    url: &Url,
    direction: git2::Direction,
    policy: super::FetchPolicy,
) -> Result<git2::Remote<'_>, GitError> {
    let mut remote = git2::Remote::create_detached(url.as_str()).map_err(|error| {
        classify(url, policy, error, |source| GitError::Connect {
            url: url.to_string(),
            source,
        })
    })?;
    remote
        .connect_auth(direction, Some(default_callbacks(policy).0), None)
        .map_err(|error| {
            classify(url, policy, error, |source| GitError::Connect {
                url: url.to_string(),
                source,
            })
        })?;
    Ok(remote)
}

/// Best-effort disconnect, swallowing the `git2` error since the remote
/// may have been closed already by the time the caller is done.
pub(crate) fn disconnect_remote(remote: &mut git2::Remote<'_>) {
    let _ = remote.disconnect();
}

/// Connects to the remote at `url` and returns the advertised refs as
/// `(refname, oid_hex)` pairs. Rejects remotes advertising more than
/// `max_refs` entries.
pub(crate) fn list_advertised_refs(
    url: &Url,
    max_refs: usize,
    policy: super::FetchPolicy,
) -> Result<Vec<(String, String)>, GitError> {
    let mut remote = connect_remote(url, git2::Direction::Fetch, policy)?;
    let advertised = remote.list().map_err(|error| {
        classify(url, policy, error, |source| GitError::ListRefs {
            url: url.to_string(),
            source,
        })
    })?;
    if advertised.len() > max_refs {
        let count = advertised.len();
        disconnect_remote(&mut remote);
        return Err(GitError::RefLimitExceeded {
            url: url.to_string(),
            count,
            limit: max_refs,
        });
    }
    let pairs = advertised
        .iter()
        .map(|h| (h.name().to_string(), h.oid().to_string()))
        .collect();
    disconnect_remote(&mut remote);
    Ok(pairs)
}

/// Connects to the remote and returns its default branch name without the
/// `refs/heads/` prefix. Rejects remotes advertising more than `max_refs`
/// entries and remotes with no advertised default branch.
pub(crate) fn discover_default_branch(
    url: &Url,
    policy: super::FetchPolicy,
    max_refs: usize,
) -> Result<String, GitError> {
    let mut remote = connect_remote(url, git2::Direction::Fetch, policy)?;
    let advertised = remote.list().map_err(|error| {
        classify(url, policy, error, |source| GitError::ListRefs {
            url: url.to_string(),
            source,
        })
    })?;
    if advertised.len() > max_refs {
        let count = advertised.len();
        disconnect_remote(&mut remote);
        return Err(GitError::RefLimitExceeded {
            url: url.to_string(),
            count,
            limit: max_refs,
        });
    }

    let branch = match remote.default_branch() {
        Ok(branch) => {
            let name = std::str::from_utf8(branch.as_ref()).map_err(|source| {
                GitError::DefaultBranchUtf8 {
                    url: url.to_string(),
                    source,
                }
            })?;
            name.strip_prefix("refs/heads/")
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .ok_or_else(|| GitError::NoDefaultBranch {
                    url: url.to_string(),
                })
        }
        Err(_) => Err(GitError::NoDefaultBranch {
            url: url.to_string(),
        }),
    };
    disconnect_remote(&mut remote);
    branch
}

/// Returns the full SHA that a commit prefix uniquely matches among a
/// set of advertised `(ref, sha)` pairs, or `None` when no ref matches
/// or when two distinct SHAs share the prefix (an ambiguous match).
///
/// The same SHA advertised under several refs is not ambiguous.
pub(crate) fn unique_ref_prefix_match<'a>(
    refs: &'a [(String, String)],
    prefix: &str,
) -> Option<&'a str> {
    let mut matched: Option<&str> = None;
    for (_, sha) in refs {
        if sha.starts_with(prefix) {
            if matched.is_some_and(|m| m != sha.as_str()) {
                return None;
            }
            matched = Some(sha);
        }
    }
    matched
}

/// Expands a commit-SHA prefix to the full 40-character SHA by cloning.
///
/// This is the fallback for a prefix that does not name a ref tip (see
/// [`GitFetcher::resolve_commit_prefix`](crate::resolver::fetch::GitFetcher::resolve_commit_prefix),
/// which tries `ls-remote` first). The Git wire protocol has no
/// prefix-expansion operation, so the objects must be fetched locally
/// and resolved with `git rev-parse` semantics. `git2`/libgit2 does not
/// support partial-clone filters, so this is a full bare clone of the
/// repository into `work_dir`; a prefix matching no commit or more than
/// one commit is rejected. `work_dir` must not already exist; the caller
/// removes it afterward.
pub(crate) fn resolve_commit_prefix(
    work_dir: &Path,
    url: &Url,
    prefix: &str,
    policy: super::FetchPolicy,
) -> Result<String, GitError> {
    let (mut fetch_opts, watch) = default_fetch_options(policy);
    fetch_opts.download_tags(git2::AutotagOption::All);
    let mut builder = git2::build::RepoBuilder::new();
    builder.bare(true).fetch_options(fetch_opts);
    let repo = builder
        .clone(url.as_str(), work_dir)
        .map_err(|error| match watch.aborted_at() {
            Some(received) => GitError::TransferLimitExceeded {
                url: url.to_string(),
                limit: policy.max_transfer_bytes.unwrap_or(0),
                received,
            },
            None => classify(url, policy, error, |source| GitError::Clone {
                url: url.to_string(),
                source,
            }),
        })?;

    // `revparse_single` on a hex prefix disambiguates against the object
    // database; peel to a commit so a prefix that resolves to a tag or
    // tree is rejected rather than silently accepted.
    match repo.revparse_single(prefix) {
        Ok(obj) => {
            let commit = obj.peel_to_commit().map_err(|_| GitError::CommitPrefix {
                url: url.to_string(),
                prefix: prefix.to_string(),
            })?;
            Ok(commit.id().to_string())
        }
        Err(_) => Err(GitError::CommitPrefix {
            url: url.to_string(),
            prefix: prefix.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use git2::Repository;
    use tempfile::tempdir;

    use super::*;
    use crate::resolver::git::ops::CredentialMode;
    use crate::resolver::git::ops::FetchPolicy;
    use crate::resolver::git::ops::test_support::build_upstream;

    #[test]
    fn ref_count_limit_is_enforced() {
        let (upstream, _sha) =
            build_upstream(&[("module.json", br#"{"name":"x","license":"MIT"}"#)]);
        let url = Url::from_directory_path(upstream.path()).unwrap();
        let err = list_advertised_refs(
            &url,
            0,
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: None,
            },
        )
        .unwrap_err();
        assert!(
            matches!(err, GitError::RefLimitExceeded { .. }),
            "got: {err}"
        );
    }

    #[test]
    fn discovers_default_branch_for_file_url() {
        let (upstream, _sha) =
            build_upstream(&[("module.json", br#"{"name":"x","license":"MIT"}"#)]);
        let repo = Repository::open(upstream.path()).unwrap();
        let expected = repo
            .head()
            .unwrap()
            .shorthand()
            .expect("HEAD should resolve to a branch")
            .to_string();
        let url = Url::from_file_path(upstream.path()).unwrap();
        let observed = discover_default_branch(
            &url,
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: None,
            },
            1024,
        )
        .unwrap();
        assert_eq!(observed, expected);
    }

    #[test]
    fn resolve_commit_prefix_expands_unique_prefix() {
        let (upstream, sha) = build_upstream(&[("index.wdl", b"workflow w {}")]);
        let url = Url::from_directory_path(upstream.path()).unwrap();
        let dest = tempdir().unwrap();

        let prefix = &sha[..8];
        let full = resolve_commit_prefix(
            &dest.path().join("expand"),
            &url,
            prefix,
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: None,
            },
        )
        .unwrap();
        assert_eq!(full, sha);
    }

    #[test]
    fn unique_ref_prefix_match_handles_ambiguity_and_aliases() {
        let refs = vec![
            ("refs/heads/main".to_string(), "a".repeat(40)),
            ("refs/tags/v1".to_string(), "a".repeat(40)), // same SHA, another ref
            ("refs/heads/dev".to_string(), format!("b{}", "0".repeat(39))),
        ];
        // Unique prefix that also happens to be advertised under two refs.
        assert_eq!(
            unique_ref_prefix_match(&refs, "aaaa"),
            Some("a".repeat(40).as_str())
        );
        // A prefix shared by two distinct SHAs is ambiguous.
        let ambiguous = vec![
            ("refs/heads/x".to_string(), format!("ab{}", "0".repeat(38))),
            ("refs/heads/y".to_string(), format!("ab{}", "1".repeat(38))),
        ];
        assert_eq!(unique_ref_prefix_match(&ambiguous, "ab"), None);
        // No ref matches.
        assert_eq!(unique_ref_prefix_match(&refs, "cccc"), None);
    }

    #[test]
    fn resolve_commit_prefix_rejects_unknown_prefix() {
        let (upstream, _sha) = build_upstream(&[("index.wdl", b"workflow w {}")]);
        let url = Url::from_directory_path(upstream.path()).unwrap();
        let dest = tempdir().unwrap();

        // A prefix that matches no commit in the repository.
        let err = resolve_commit_prefix(
            &dest.path().join("expand"),
            &url,
            "0123456",
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: None,
            },
        )
        .unwrap_err();
        assert!(
            matches!(err, GitError::CommitPrefix { .. }),
            "expected `CommitPrefix`, got: {err}"
        );
    }

    #[test]
    fn resolve_commit_prefix_maps_transfer_cap_failure() {
        let (upstream, _sha) = build_upstream(&[("index.wdl", &[b'x'; 65_536])]);
        let url = Url::from_directory_path(upstream.path()).unwrap();
        let destination = tempdir().unwrap();
        let error = resolve_commit_prefix(
            &destination.path().join("expand"),
            &url,
            "deadbeef",
            FetchPolicy {
                credentials: CredentialMode::Disabled,
                max_transfer_bytes: Some(1),
            },
        )
        .unwrap_err();
        assert!(matches!(error, GitError::TransferLimitExceeded { .. }));
    }
}
