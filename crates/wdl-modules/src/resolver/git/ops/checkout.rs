//! Sparse checkout and materialization operations for Git cache leaves.

use std::collections::BTreeSet;
use std::path::Path;

use git2::Repository;
use serde::Deserialize;
use serde::Serialize;
use url::Url;

use super::GitError;
use super::cache_store::CacheLocation;
use super::cache_store::clear_cache_leaf;
use super::cache_store::lock_cache_leaf;
use super::cache_store::lock_cache_root_shared;
use super::cache_store::remove_worktree_path;
use super::cache_store::sparse_meta_path;
use super::creds::default_fetch_options;
use super::error::classify;

/// The module folders currently materialized in a sparse-checkout cache
/// leaf.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(transparent)]
struct SparseMeta(BTreeSet<String>);

/// Statistics about a Git tree object collected without checkout by
/// walking the tree's blob entries.
#[derive(Clone, Debug, Default)]
pub(crate) struct GitTreeStats {
    /// Total blob entries.
    pub files: usize,
    /// Total bytes across all blobs.
    pub bytes: u64,
}

/// Per-module materialized tree limits.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TreeLimits {
    /// Maximum number of files.
    pub max_files: Option<usize>,
    /// Maximum total file bytes.
    pub max_bytes: Option<u64>,
}

/// Inspects a subtree at `path` within the commit identified by `oid`,
/// counting blob entries and summing their sizes without materializing
/// any content to disk.
pub(crate) fn inspect_subtree_stats(
    repo: &Repository,
    oid: git2::Oid,
    path: &str,
) -> Result<GitTreeStats, GitError> {
    let commit = repo
        .find_commit(oid)
        .map_err(|source| GitError::Object { source })?;
    let root_tree = commit
        .tree()
        .map_err(|source| GitError::Object { source })?;
    let subtree = if path.is_empty() || path == "." {
        root_tree
    } else {
        let entry = root_tree
            .get_path(Path::new(path))
            .map_err(|source| GitError::Object { source })?;
        repo.find_tree(entry.id())
            .map_err(|source| GitError::Object { source })?
    };
    let mut blob_oids = Vec::new();
    subtree
        .walk(git2::TreeWalkMode::PreOrder, |_, entry| {
            if entry.kind() == Some(git2::ObjectType::Blob) {
                blob_oids.push(entry.id());
            }
            git2::TreeWalkResult::Ok
        })
        .map_err(|source| GitError::Object { source })?;

    let odb = repo.odb().map_err(|source| GitError::Object { source })?;
    let mut stats = GitTreeStats {
        files: blob_oids.len(),
        ..GitTreeStats::default()
    };
    for blob_oid in blob_oids {
        let (size, kind) = odb
            .read_header(blob_oid)
            .map_err(|source| GitError::Object { source })?;
        if kind != git2::ObjectType::Blob {
            let source = git2::Error::new(
                git2::ErrorCode::GenericError,
                git2::ErrorClass::Object,
                "tree entry is not a blob",
            );
            return Err(GitError::Object { source });
        }
        stats.bytes = stats.bytes.saturating_add(size as u64);
    }
    Ok(stats)
}

/// Checks that the tree statistics at each of the given `paths` fall within
/// configured limits.
pub(crate) fn enforce_tree_limits(
    repo: &Repository,
    oid: git2::Oid,
    paths: &[String],
    limits: TreeLimits,
) -> Result<(), GitError> {
    if limits.max_files.is_none() && limits.max_bytes.is_none() {
        return Ok(());
    }
    for path in paths {
        let stats = inspect_subtree_stats(repo, oid, path)?;
        let files_exceeded = limits.max_files.is_some_and(|limit| stats.files > limit);
        let bytes_exceeded = limits.max_bytes.is_some_and(|limit| stats.bytes > limit);
        if files_exceeded || bytes_exceeded {
            return Err(GitError::TreeLimitExceeded {
                path: path.clone(),
                files: stats.files,
                bytes: stats.bytes,
                max_files: limits.max_files,
                max_bytes: limits.max_bytes,
            });
        }
    }
    Ok(())
}

/// Materializes only the listed module folders from the repo's HEAD
/// tree using libgit2's path-filtered checkout.
fn apply_sparse_checkout(repo: &Repository, paths: &[String]) -> Result<(), GitError> {
    let head_commit = repo
        .head()
        .map_err(|source| GitError::Object { source })?
        .peel_to_commit()
        .map_err(|source| GitError::Object { source })?;
    let tree = head_commit
        .tree()
        .map_err(|source| GitError::Object { source })?;

    let mut checkout = git2::build::CheckoutBuilder::new();
    // Disable all libgit2 filters (CRLF/LF conversion, `ident`, clean/smudge)
    // so the checked-out bytes are identical to the stored blob bytes on every
    // platform. Module content addressing hashes the on-disk files, so a
    // filter that rewrote line endings (e.g. a Windows `core.autocrlf=true`, or
    // a repo `.gitattributes` demanding `eol=crlf`) would produce a different
    // digest per platform and break signature/lock verification. This is the
    // sole checkout site that materializes module content, so it is the only
    // place the guarantee must be enforced.
    checkout
        .disable_filters(true)
        .force()
        .recreate_missing(true);
    for p in paths {
        // Match every entry under the given module folder.
        checkout.path(format!("{p}/**"));
    }
    repo.checkout_tree(tree.as_object(), Some(&mut checkout))
        .map_err(|source| GitError::Checkout {
            path: repo
                .workdir()
                .map_or_else(|| Path::new(".").to_path_buf(), Path::to_path_buf),
            source,
        })?;

    Ok(())
}

/// Writes the sparse-checkout metadata next to the cache leaf.
fn save_sparse_meta(leaf: &Path, paths: &[String]) -> Result<(), GitError> {
    let meta = SparseMeta(paths.iter().cloned().collect());
    let path = sparse_meta_path(leaf);
    let bytes = serde_json::to_vec_pretty(&meta).map_err(|source| GitError::Json {
        path: path.clone(),
        source,
    })?;
    let temp_path = path.with_extension("json.tmp");
    let cleanup = |original: GitError| {
        if let Err(error) = std::fs::remove_file(&temp_path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                path = %temp_path.display(),
                %error,
                "failed to clean up sparse metadata temporary file",
            );
        }
        original
    };
    if let Err(source) = std::fs::write(&temp_path, bytes) {
        return Err(cleanup(GitError::Io {
            path: temp_path.clone(),
            source,
        }));
    }
    std::fs::rename(&temp_path, &path).map_err(|source| {
        cleanup(GitError::Io {
            path: path.clone(),
            source,
        })
    })
}

/// Parsed state of a sparse-checkout metadata file.
#[derive(Debug)]
enum SparseMetaState {
    /// The metadata file does not exist.
    Missing,
    /// The metadata file contains a valid sparse path set.
    Valid,
    /// The metadata file exists but is not valid JSON.
    Corrupt,
}

/// Reads and classifies sparse-checkout metadata without hiding corruption.
fn sparse_meta_state(leaf: &Path) -> Result<SparseMetaState, GitError> {
    let path = sparse_meta_path(leaf);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SparseMetaState::Missing);
        }
        Err(source) => return Err(GitError::Io { path, source }),
    };
    Ok(if serde_json::from_slice::<SparseMeta>(&bytes).is_ok() {
        SparseMetaState::Valid
    } else {
        SparseMetaState::Corrupt
    })
}

/// Reads the sparse-checkout metadata for a cache leaf, returning the
/// default empty meta if the file is missing.
fn load_sparse_meta(leaf: &Path) -> Result<SparseMeta, GitError> {
    let path = sparse_meta_path(leaf);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SparseMeta::default());
        }
        Err(source) => return Err(GitError::Io { path, source }),
    };
    serde_json::from_slice(&bytes).map_err(|source| GitError::Json { path, source })
}

/// Fetches `commit` from `url` into `leaf`, then materializes only the listed
/// `paths` from its tree.
///
/// The primary path fetches the exact commit into `refs/fetched/<commit>`. A
/// server that does not advertise exact-object fetch support falls back to a
/// shallow default-branch clone. `leaf` and any missing parent directories are
/// created. Credentials and the transfer-byte limit come from `fetch`.
///
/// When tree limits are configured, selected module subtrees are inspected
/// after fetch and before sparse checkout.
pub(crate) fn clone_with_sparse_checkout<I, S>(
    url: &Url,
    commit: &str,
    leaf: &Path,
    paths: I,
    fetch: super::creds::FetchPolicy,
    limits: TreeLimits,
) -> Result<(), GitError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let owned: Vec<String> = paths.into_iter().map(|s| s.as_ref().to_string()).collect();
    let parent = leaf
        .parent()
        .ok_or_else(|| GitError::RootLeaf(leaf.to_path_buf()))?;
    std::fs::create_dir_all(parent).map_err(|source| GitError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let oid = git2::Oid::from_str(commit).map_err(|source| GitError::Object { source })?;
    let refspec = format!("+{commit}:refs/fetched/{commit}");

    let repo = Repository::init(leaf).map_err(|source| GitError::Clone {
        url: url.to_string(),
        source,
    })?;
    let mut remote = repo.remote("origin", url.as_str()).map_err(|error| {
        classify(url, fetch, error, |source| GitError::Connect {
            url: url.to_string(),
            source,
        })
    })?;
    let (mut fetch_opts, watch) = default_fetch_options(fetch);
    if url.scheme() != "file" {
        fetch_opts.depth(1);
    }
    let fetch_result = remote.fetch(&[&refspec], Some(&mut fetch_opts), None);
    drop(remote);
    let repo = match fetch_result {
        Ok(()) => repo,
        Err(error) => {
            if let Some(received) = watch.aborted_at() {
                return Err(GitError::TransferLimitExceeded {
                    url: url.to_string(),
                    limit: fetch.max_transfer_bytes.unwrap_or(0),
                    received,
                });
            }
            if error.code() == git2::ErrorCode::Auth {
                return Err(classify(url, fetch, error, |source| {
                    GitError::FetchCommit {
                        url: url.to_string(),
                        commit: commit.to_string(),
                        source,
                    }
                }));
            }
            // libgit2 fetch.c:144-151 rejects an exact OID when the remote does not
            // advertise OID wants.
            if error.class() != git2::ErrorClass::Invalid
                || !error
                    .message()
                    .contains("cannot fetch a specific object from the remote repository")
            {
                return Err(GitError::FetchCommit {
                    url: url.to_string(),
                    commit: commit.to_string(),
                    source: error,
                });
            }
            drop(repo);
            // Local fixtures do not exercise this path because local transport advertises
            // both OID capabilities (libgit2 local.c:263-268).
            std::fs::remove_dir_all(leaf).map_err(|source| GitError::Io {
                path: leaf.to_path_buf(),
                source,
            })?;

            let (mut fallback_opts, fallback_watch) = default_fetch_options(fetch);
            if url.scheme() != "file" {
                fallback_opts.depth(1);
            }
            let mut empty_checkout = git2::build::CheckoutBuilder::new();
            empty_checkout.disable_filters(true).dry_run();
            let mut builder = git2::build::RepoBuilder::new();
            builder
                .fetch_options(fallback_opts)
                .with_checkout(empty_checkout)
                .clone_local(git2::build::CloneLocal::Auto)
                .bare(false);
            let fallback_repo = builder.clone(url.as_str(), leaf).map_err(|error| {
                if let Some(received) = fallback_watch.aborted_at() {
                    GitError::TransferLimitExceeded {
                        url: url.to_string(),
                        limit: fetch.max_transfer_bytes.unwrap_or(0),
                        received,
                    }
                } else {
                    classify(url, fetch, error, |source| GitError::Clone {
                        url: url.to_string(),
                        source,
                    })
                }
            })?;
            if fallback_repo.find_commit(oid).is_err() {
                let (mut oid_opts, oid_watch) = default_fetch_options(fetch);
                if url.scheme() != "file" {
                    oid_opts.depth(1);
                }
                let mut oid_remote = fallback_repo.find_remote("origin").map_err(|error| {
                    classify(url, fetch, error, |source| GitError::Connect {
                        url: url.to_string(),
                        source,
                    })
                })?;
                oid_remote
                    .fetch(&[&refspec], Some(&mut oid_opts), None)
                    .map_err(|error| {
                        if let Some(received) = oid_watch.aborted_at() {
                            GitError::TransferLimitExceeded {
                                url: url.to_string(),
                                limit: fetch.max_transfer_bytes.unwrap_or(0),
                                received,
                            }
                        } else {
                            classify(url, fetch, error, |source| GitError::FetchCommit {
                                url: url.to_string(),
                                commit: commit.to_string(),
                                source,
                            })
                        }
                    })?;
            }
            fallback_repo
        }
    };

    repo.set_head_detached(oid)
        .map_err(|source| GitError::Object { source })?;
    enforce_tree_limits(&repo, oid, &owned, limits)?;
    apply_sparse_checkout(&repo, &owned)?;
    save_sparse_meta(leaf, &owned)?;

    Ok(())
}

/// Extends an existing sparse-checkout cache leaf to additionally materialize
/// paths.
pub(crate) fn extend_sparse_checkout<I, S>(
    leaf: &Path,
    paths: I,
    limits: TreeLimits,
) -> Result<(), GitError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let repo = Repository::open(leaf).map_err(|source| GitError::Object { source })?;
    let existing = load_sparse_meta(leaf)?.0;
    let mut all = existing.clone();
    let mut new_paths = Vec::new();
    for p in paths {
        let s = p.as_ref().to_string();
        if !existing.contains(&s) {
            new_paths.push(s.clone());
        }
        all.insert(s);
    }
    if !new_paths.is_empty() {
        let head_oid = repo
            .head()
            .map_err(|source| GitError::Object { source })?
            .peel_to_commit()
            .map_err(|source| GitError::Object { source })?
            .id();
        enforce_tree_limits(&repo, head_oid, &new_paths, limits)?;
    }
    let all_owned: Vec<String> = all.into_iter().collect();
    clear_sparse_paths(leaf, &all_owned)?;
    apply_sparse_checkout(&repo, &all_owned)?;
    save_sparse_meta(leaf, &all_owned)?;
    Ok(())
}

/// Removes materialized sparse paths before restoring them from Git objects.
fn clear_sparse_paths(leaf: &Path, paths: &[String]) -> Result<(), GitError> {
    for path in paths {
        if path == "." {
            let entries = std::fs::read_dir(leaf).map_err(|source| GitError::Io {
                path: leaf.to_path_buf(),
                source,
            })?;
            for entry in entries {
                let entry = entry.map_err(|source| GitError::Io {
                    path: leaf.to_path_buf(),
                    source,
                })?;
                if entry.file_name() == ".git" {
                    continue;
                }
                remove_worktree_path(&entry.path())?;
            }
        } else {
            remove_worktree_path(&leaf.join(path))?;
        }
    }
    Ok(())
}

/// Returns whether an existing cache leaf is pinned to `commit`.
fn cache_leaf_matches_commit(leaf: &Path, commit: &str) -> Result<bool, GitError> {
    let repo = Repository::open(leaf).map_err(|source| GitError::Object { source })?;
    let observed = repo
        .head()
        .map_err(|source| GitError::Object { source })?
        .peel_to_commit()
        .map_err(|source| GitError::Object { source })?
        .id();
    let expected = git2::Oid::from_str(commit).map_err(|source| GitError::Object { source })?;
    Ok(observed == expected)
}

/// Ensures `leaf` contains a sparse checkout of `url` at `commit`
/// covering at least `paths`. Clones if `leaf` does not yet exist;
/// otherwise extends the existing leaf's sparse-checkout set.
///
/// Returns `true` when a new cache leaf is cloned and `false` when an
/// existing leaf is reused.
///
/// If the initial clone fails, the partially-written leaf is removed so a
/// corrupt checkout does not persist.
pub(crate) fn ensure_materialized<I, S>(
    cache: CacheLocation<'_>,
    url: &Url,
    commit: &str,
    paths: I,
    fetch: super::creds::FetchPolicy,
    limits: TreeLimits,
) -> Result<bool, GitError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let _cache_lock = lock_cache_root_shared(cache.root)?;
    let leaf = cache.leaf;
    let existed = leaf.exists();
    tracing::debug!(
        cache_leaf = %leaf.display(),
        url = %url,
        commit,
        exists = existed,
        "preparing module cache leaf"
    );
    tracing::trace!(cache_leaf = %leaf.display(), "acquiring module cache leaf lock");
    let _lock = lock_cache_leaf(leaf)?;
    tracing::trace!(cache_leaf = %leaf.display(), "acquired module cache leaf lock");
    if leaf.exists() && !cache_leaf_matches_commit(leaf, commit)? {
        tracing::warn!(
            cache_leaf = %leaf.display(),
            commit,
            "evicting module cache leaf with an unexpected Git HEAD"
        );
        clear_cache_leaf(leaf)?;
    }
    if leaf.exists() {
        match sparse_meta_state(leaf)? {
            SparseMetaState::Corrupt => {
                tracing::warn!(
                    cache_leaf = %leaf.display(),
                    "evicting module cache leaf with corrupt sparse metadata",
                );
                clear_cache_leaf(leaf)?;
            }
            SparseMetaState::Missing | SparseMetaState::Valid => {}
        }
    }
    if leaf.exists() {
        tracing::debug!(
            cache_leaf = %leaf.display(),
            commit,
            "using cached module checkout"
        );
        extend_sparse_checkout(leaf, paths, limits)?;
        Ok(false)
    } else {
        tracing::info!(
            cache_leaf = %leaf.display(),
            url = %url,
            commit,
            "fetching module into cache"
        );
        let result = clone_with_sparse_checkout(url, commit, leaf, paths, fetch, limits);
        if result.is_err()
            && leaf.exists()
            && let Err(error) = std::fs::remove_dir_all(leaf)
        {
            tracing::warn!(
                path = %leaf.display(),
                %error,
                "failed to clean up cache leaf after a failed clone",
            );
        }
        result.map(|()| true)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use git2::Repository;
    use git2::Signature;
    use tempfile::tempdir;

    use super::*;
    use crate::resolver::git::ops::CredentialMode;
    use crate::resolver::git::ops::FetchPolicy;
    use crate::resolver::git::ops::test_support::build_upstream;

    #[test]
    fn materialization_fetches_only_the_pinned_commit() {
        let (upstream, sha) = build_upstream(&[("module/module.json", br#"{"name":"module"}"#)]);
        let destination = tempdir().unwrap();
        let leaf = destination.path().join("leaf");
        let url = Url::from_file_path(upstream.path()).unwrap();

        ensure_materialized(
            CacheLocation {
                root: destination.path(),
                leaf: &leaf,
            },
            &url,
            &sha,
            ["module"],
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: None,
            },
            TreeLimits::default(),
        )
        .unwrap();

        let repo = Repository::open(&leaf).unwrap();
        assert!(repo.find_reference(&format!("refs/fetched/{sha}")).is_ok());
        assert!(repo.find_reference("refs/remotes/origin/HEAD").is_err());
    }

    #[test]
    fn materialization_cap_aborts_before_checkout() {
        let (upstream, sha) = build_upstream(&[("module/data.bin", &[b'x'; 65_536])]);
        let destination = tempdir().unwrap();
        let leaf = destination.path().join("leaf");
        let url = Url::from_file_path(upstream.path()).unwrap();

        let error = ensure_materialized(
            CacheLocation {
                root: destination.path(),
                leaf: &leaf,
            },
            &url,
            &sha,
            ["module"],
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: Some(1),
            },
            TreeLimits::default(),
        )
        .unwrap_err();

        assert!(matches!(error, GitError::TransferLimitExceeded { .. }));
        assert!(!leaf.exists());
    }

    #[test]
    fn clones_with_sparse_checkout_to_subset_of_paths() {
        let (upstream, sha) = build_upstream(&[
            (
                "csvkit/module.json",
                br#"{"name":"csvkit","license":"MIT"}"#,
            ),
            ("csvkit/index.wdl", b"workflow w {}"),
            (
                "spellbook/module.json",
                br#"{"name":"spellbook","license":"MIT"}"#,
            ),
            ("spellbook/index.wdl", b"workflow w {}"),
        ]);

        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();
        clone_with_sparse_checkout(
            &url,
            &sha,
            &leaf,
            ["csvkit"],
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: None,
            },
            TreeLimits::default(),
        )
        .unwrap();

        assert!(leaf.join("csvkit").join("module.json").exists());
        assert!(!leaf.join("spellbook").exists());

        let meta = load_sparse_meta(&leaf).unwrap();
        assert_eq!(
            meta.0.iter().cloned().collect::<Vec<_>>(),
            vec!["csvkit".to_string()]
        );
    }

    #[test]
    fn ensure_materialized_clones_then_extends() {
        let (upstream, sha) = build_upstream(&[
            (
                "csvkit/module.json",
                br#"{"name":"csvkit","license":"MIT"}"#,
            ),
            ("csvkit/index.wdl", b"workflow w {}"),
            (
                "spellbook/module.json",
                br#"{"name":"spellbook","license":"MIT"}"#,
            ),
            ("spellbook/index.wdl", b"workflow w {}"),
        ]);

        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        let fetched = ensure_materialized(
            CacheLocation {
                root: dest.path(),
                leaf: &leaf,
            },
            &url,
            &sha,
            ["csvkit"],
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: None,
            },
            TreeLimits::default(),
        )
        .unwrap();
        assert!(fetched);
        assert!(leaf.join("csvkit").join("module.json").exists());
        assert!(!leaf.join("spellbook").exists());
        std::fs::write(leaf.join("csvkit").join("index.wdl"), b"tampered").unwrap();
        std::fs::write(leaf.join("csvkit").join("untracked.wdl"), b"untracked").unwrap();

        let fetched = ensure_materialized(
            CacheLocation {
                root: dest.path(),
                leaf: &leaf,
            },
            &url,
            &sha,
            ["spellbook"],
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: None,
            },
            TreeLimits::default(),
        )
        .unwrap();
        assert!(!fetched);
        assert!(leaf.join("csvkit").join("module.json").exists());
        assert_eq!(
            std::fs::read(leaf.join("csvkit").join("index.wdl")).unwrap(),
            b"workflow w {}"
        );
        assert!(!leaf.join("csvkit").join("untracked.wdl").exists());
        assert!(leaf.join("spellbook").join("module.json").exists());

        {
            let cached = Repository::open(&leaf).unwrap();
            let head = cached.head().unwrap().peel_to_commit().unwrap();
            let tree = head.tree().unwrap();
            let signature = Signature::now("test", "test@example.com").unwrap();
            cached
                .commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    "unexpected cache commit",
                    &tree,
                    &[&head],
                )
                .unwrap();
        }

        let fetched = ensure_materialized(
            CacheLocation {
                root: dest.path(),
                leaf: &leaf,
            },
            &url,
            &sha,
            ["csvkit"],
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: None,
            },
            TreeLimits::default(),
        )
        .unwrap();
        assert!(fetched);
        let cached = Repository::open(&leaf).unwrap();
        assert_eq!(
            cached.head().unwrap().peel_to_commit().unwrap().id(),
            git2::Oid::from_str(&sha).unwrap()
        );
    }

    #[test]
    fn corrupt_sparse_meta_evicts_and_reclones() {
        let (upstream, sha) = build_upstream(&[("csvkit/module.json", br#"{"name":"csvkit"}"#)]);
        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();
        let fetch = FetchPolicy {
            credentials: CredentialMode::Enabled,
            max_transfer_bytes: None,
        };

        assert!(
            ensure_materialized(
                CacheLocation {
                    root: dest.path(),
                    leaf: &leaf,
                },
                &url,
                &sha,
                ["csvkit"],
                fetch,
                TreeLimits::default(),
            )
            .unwrap()
        );
        fs::write(sparse_meta_path(&leaf), b"not json").unwrap();

        assert!(
            ensure_materialized(
                CacheLocation {
                    root: dest.path(),
                    leaf: &leaf,
                },
                &url,
                &sha,
                ["csvkit"],
                fetch,
                TreeLimits::default(),
            )
            .unwrap()
        );
        assert!(leaf.join("csvkit/module.json").exists());
        assert!(load_sparse_meta(&leaf).is_ok());
        assert!(!sparse_meta_path(&leaf).with_extension("json.tmp").exists());
    }

    #[test]
    fn extend_rejects_corrupt_sparse_meta() {
        let (upstream, sha) = build_upstream(&[("csvkit/module.json", br#"{"name":"csvkit"}"#)]);
        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();
        clone_with_sparse_checkout(
            &url,
            &sha,
            &leaf,
            ["csvkit"],
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: None,
            },
            TreeLimits::default(),
        )
        .unwrap();
        fs::write(sparse_meta_path(&leaf), b"not json").unwrap();

        let error =
            extend_sparse_checkout(&leaf, ["spellbook"], TreeLimits::default()).unwrap_err();
        assert!(matches!(error, GitError::Json { .. }));
        assert!(leaf.exists());
        assert_eq!(fs::read(sparse_meta_path(&leaf)).unwrap(), b"not json");
    }

    #[test]
    fn extend_adds_a_second_module_folder() {
        let (upstream, sha) = build_upstream(&[
            (
                "csvkit/module.json",
                br#"{"name":"csvkit","license":"MIT"}"#,
            ),
            ("csvkit/index.wdl", b"workflow w {}"),
            (
                "spellbook/module.json",
                br#"{"name":"spellbook","license":"MIT"}"#,
            ),
            ("spellbook/index.wdl", b"workflow w {}"),
        ]);

        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        clone_with_sparse_checkout(
            &url,
            &sha,
            &leaf,
            ["csvkit"],
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: None,
            },
            TreeLimits::default(),
        )
        .unwrap();
        assert!(!leaf.join("spellbook").exists());

        extend_sparse_checkout(&leaf, ["spellbook"], TreeLimits::default()).unwrap();
        assert!(leaf.join("spellbook").join("module.json").exists());
        assert!(leaf.join("csvkit").join("module.json").exists());

        let meta = load_sparse_meta(&leaf).unwrap();
        let mut paths: Vec<_> = meta.0.into_iter().collect();
        paths.sort();
        assert_eq!(paths, vec!["csvkit".to_string(), "spellbook".to_string()]);
    }

    #[test]
    fn inspect_subtree_stats_counts_blobs() {
        let (upstream, sha) = build_upstream(&[
            ("mod/a.wdl", b"task a {}"),
            ("mod/b.wdl", b"task b {}"),
            ("mod/sub/c.wdl", b"task c {}"),
        ]);
        let repo = Repository::open(upstream.path()).unwrap();
        let oid = git2::Oid::from_str(&sha).unwrap();
        let stats = inspect_subtree_stats(&repo, oid, "mod").unwrap();
        assert_eq!(stats.files, 3);
        assert_eq!(
            stats.bytes,
            b"task a {}".len() as u64 + b"task b {}".len() as u64 + b"task c {}".len() as u64
        );
    }

    #[test]
    fn inspect_subtree_stats_reports_missing_blob_object() {
        let (upstream, sha) = build_upstream(&[("mod/a.wdl", b"task a {}")]);
        let repo = Repository::open(upstream.path()).unwrap();
        let oid = git2::Oid::from_str(&sha).unwrap();
        let blob_oid = repo
            .find_commit(oid)
            .unwrap()
            .tree()
            .unwrap()
            .get_path(Path::new("mod/a.wdl"))
            .unwrap()
            .id();
        let blob_hex = blob_oid.to_string();
        let object_path = upstream
            .path()
            .join(".git/objects")
            .join(&blob_hex[..2])
            .join(&blob_hex[2..]);
        fs::remove_file(object_path).unwrap();

        let error = inspect_subtree_stats(&repo, oid, "mod").unwrap_err();
        assert!(matches!(error, GitError::Object { .. }));
    }

    #[test]
    fn tree_file_limit_blocks_clone() {
        let (upstream, sha) = build_upstream(&[
            ("mod/a.wdl", b"task a {}"),
            ("mod/b.wdl", b"task b {}"),
            ("mod/c.wdl", b"task c {}"),
        ]);
        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        let err = clone_with_sparse_checkout(
            &url,
            &sha,
            &leaf,
            ["mod"],
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: None,
            },
            TreeLimits {
                max_files: Some(2),
                max_bytes: None,
            },
        )
        .unwrap_err();
        assert!(
            matches!(err, GitError::TreeLimitExceeded { files: 3, .. }),
            "got: {err}"
        );
    }

    #[test]
    fn tree_byte_limit_blocks_clone() {
        let (upstream, sha) = build_upstream(&[("mod/big.wdl", &[0u8; 1024])]);
        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        let err = clone_with_sparse_checkout(
            &url,
            &sha,
            &leaf,
            ["mod"],
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: None,
            },
            TreeLimits {
                max_files: None,
                max_bytes: Some(512),
            },
        )
        .unwrap_err();
        assert!(
            matches!(err, GitError::TreeLimitExceeded { bytes: 1024, .. }),
            "got: {err}"
        );
    }

    #[test]
    fn tree_limits_pass_when_within_bounds() {
        let (upstream, sha) =
            build_upstream(&[("mod/a.wdl", b"task a {}"), ("mod/b.wdl", b"task b {}")]);
        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        clone_with_sparse_checkout(
            &url,
            &sha,
            &leaf,
            ["mod"],
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: None,
            },
            TreeLimits {
                max_files: Some(100),
                max_bytes: Some(100_000),
            },
        )
        .unwrap();
        assert!(leaf.join("mod").join("a.wdl").exists());
    }

    #[test]
    fn tree_limits_enforced_on_extend() {
        let (upstream, sha) = build_upstream(&[
            ("small/a.wdl", b"x"),
            ("big/a.wdl", b"task a {}"),
            ("big/b.wdl", b"task b {}"),
            ("big/c.wdl", b"task c {}"),
        ]);
        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        clone_with_sparse_checkout(
            &url,
            &sha,
            &leaf,
            ["small"],
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: None,
            },
            TreeLimits::default(),
        )
        .unwrap();

        let err = extend_sparse_checkout(
            &leaf,
            ["big"],
            TreeLimits {
                max_files: Some(2),
                max_bytes: None,
            },
        )
        .unwrap_err();
        assert!(
            matches!(err, GitError::TreeLimitExceeded { files: 3, .. }),
            "got: {err}"
        );
    }

    /// Verifies that `clone_with_sparse_checkout` can materialize a
    /// commit that is not reachable from the remote's default HEAD.
    /// The initial shallow clone fetches only the default branch, so
    /// the selected commit must be fetched explicitly as a fallback.
    #[test]
    fn clones_commit_not_reachable_from_default_head() {
        let upstream = tempdir().unwrap();
        let repo = Repository::init(upstream.path()).unwrap();
        let sig = Signature::now("test", "test@example.com").unwrap();

        // commit on default branch (main) with only `mod_a/`
        let mod_a = upstream.path().join("mod_a");
        fs::create_dir_all(&mod_a).unwrap();
        fs::write(mod_a.join("a.txt"), b"main").unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let main_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "main commit", &tree, &[])
            .unwrap();
        let main_commit = repo.find_commit(main_oid).unwrap();

        // commit on a separate branch adding `mod_b/`
        repo.branch("other", &main_commit, false).unwrap();
        repo.set_head("refs/heads/other").unwrap();
        let mod_b = upstream.path().join("mod_b");
        fs::create_dir_all(&mod_b).unwrap();
        fs::write(mod_b.join("b.txt"), b"other").unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let other_oid = repo
            .commit(
                Some("refs/heads/other"),
                &sig,
                &sig,
                "other commit",
                &tree,
                &[&main_commit],
            )
            .unwrap();

        // reset HEAD back to main so the shallow clone won't include `other`
        repo.set_head("refs/heads/main").unwrap();

        let leaf = tempdir().unwrap();
        let leaf_path = leaf.path().join("checkout");
        let url = Url::from_file_path(upstream.path()).unwrap();
        clone_with_sparse_checkout(
            &url,
            &other_oid.to_string(),
            &leaf_path,
            ["mod_b"],
            FetchPolicy {
                credentials: CredentialMode::Enabled,
                max_transfer_bytes: None,
            },
            TreeLimits::default(),
        )
        .unwrap();

        assert!(
            leaf_path.join("mod_b").join("b.txt").exists(),
            "checkout should contain the file from the non-default branch"
        );
    }

    #[test]
    fn sparse_clone_maps_transfer_cap_failure() {
        let (upstream, sha) = build_upstream(&[("mod/index.wdl", &[b'x'; 65_536])]);
        let url = Url::from_directory_path(upstream.path()).unwrap();
        let destination = tempdir().unwrap();
        let error = clone_with_sparse_checkout(
            &url,
            &sha,
            &destination.path().join("leaf"),
            ["mod"],
            FetchPolicy {
                credentials: CredentialMode::Disabled,
                max_transfer_bytes: Some(1),
            },
            TreeLimits::default(),
        )
        .unwrap_err();
        assert!(matches!(error, GitError::TransferLimitExceeded { .. }));
    }
}
