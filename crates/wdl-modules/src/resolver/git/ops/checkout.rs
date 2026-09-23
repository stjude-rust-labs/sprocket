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

/// Materializes only the listed repository-relative paths from the repo's
/// HEAD tree using libgit2's path-filtered checkout.
///
/// Each path is matched literally and covers everything beneath it. The
/// path `.` selects the whole tree. An empty list writes nothing.
fn apply_sparse_checkout(repo: &Repository, paths: &[String]) -> Result<(), GitError> {
    if paths.is_empty() {
        return Ok(());
    }
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
        .recreate_missing(true)
        .disable_pathspec_match(true);
    // A literal path matches the entry and everything beneath it. `.` means
    // the whole tree, which libgit2 expresses as no path filter at all.
    if !paths.iter().any(|p| p == ".") {
        for p in paths {
            checkout.path(p.as_str());
        }
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
            // libgit2 fetch.c:144-151 rejects an exact OID when the remote does
            // not advertise OID wants.
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
            // Local fixtures do not exercise this path because local transport
            // advertises both OID capabilities (libgit2
            // local.c:263-268).
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

/// Returns whether the materialized sparse path `existing` already covers
/// `path`.
///
/// Paths are compared by component, so `lib` covers `lib/common` but not
/// `library`. The path `.` covers everything.
fn sparse_path_covers(existing: &str, path: &str) -> bool {
    existing == "."
        || existing == path
        || path
            .strip_prefix(existing)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Joins a repository-relative sparse path with a child entry name.
fn join_sparse_path(parent: &str, name: &str) -> String {
    if parent == "." {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

/// Deduplicates `paths` and drops any path covered by another one.
fn normalize_sparse_paths<I, S>(paths: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let unique: BTreeSet<String> = paths.into_iter().map(|p| p.as_ref().to_string()).collect();
    unique
        .iter()
        .filter(|path| {
            !unique
                .iter()
                .any(|other| other != *path && sparse_path_covers(other, path))
        })
        .cloned()
        .collect()
}

/// Returns the subtree at the sparse path `path` in `tree`.
fn sparse_subtree<'r>(
    repo: &'r Repository,
    tree: &git2::Tree<'r>,
    path: &str,
) -> Result<git2::Tree<'r>, GitError> {
    if path == "." {
        return Ok(tree.clone());
    }
    let entry = tree
        .get_path(Path::new(path))
        .map_err(|source| GitError::Object { source })?;
    repo.find_tree(entry.id())
        .map_err(|source| GitError::Object { source })
}

/// Returns a tree entry's name, rejecting names that are not UTF-8.
fn tree_entry_name(entry: &git2::TreeEntry<'_>) -> Result<String, GitError> {
    entry
        .name()
        .map(str::to_string)
        .map_err(|_| GitError::Object {
            source: git2::Error::new(
                git2::ErrorCode::GenericError,
                git2::ErrorClass::Tree,
                "tree entry name is not valid UTF-8",
            ),
        })
}

/// Computes the paths to write when materializing `new_paths` without
/// touching the already-materialized `existing` paths.
///
/// A new path that contains existing paths (for example `.` when `dep` is
/// already materialized) is expanded into its tree entries so the existing
/// subtrees are skipped.
fn sparse_write_set(
    repo: &Repository,
    tree: &git2::Tree<'_>,
    new_paths: &[String],
    existing: &BTreeSet<String>,
) -> Result<Vec<String>, GitError> {
    fn expand(
        repo: &Repository,
        tree: &git2::Tree<'_>,
        path: &str,
        existing: &BTreeSet<String>,
        out: &mut Vec<String>,
    ) -> Result<(), GitError> {
        let nested: Vec<&String> = existing
            .iter()
            .filter(|e| sparse_path_covers(path, e))
            .collect();
        if nested.is_empty() {
            out.push(path.to_string());
            return Ok(());
        }
        let subtree = sparse_subtree(repo, tree, path)?;
        for entry in subtree.iter() {
            let child = join_sparse_path(path, &tree_entry_name(&entry)?);
            if existing.contains(&child) {
                continue;
            }
            if entry.kind() == Some(git2::ObjectType::Tree)
                && nested.iter().any(|e| sparse_path_covers(&child, e))
            {
                expand(repo, tree, &child, existing, out)?;
            } else {
                out.push(child);
            }
        }
        Ok(())
    }

    let mut out = Vec::new();
    for path in new_paths {
        expand(repo, tree, path, existing, &mut out)?;
    }
    Ok(out)
}

/// Returns the tree of the leaf's checked-out HEAD commit.
fn head_tree(repo: &Repository) -> Result<git2::Tree<'_>, GitError> {
    repo.head()
        .map_err(|source| GitError::Object { source })?
        .peel_to_tree()
        .map_err(|source| GitError::Object { source })
}

/// Extends an existing sparse-checkout cache leaf to additionally materialize
/// paths.
///
/// Paths that are already materialized, or covered by a materialized
/// ancestor, are left untouched: other resolutions may be reading them
/// without holding the leaf lock. When nothing is new this writes nothing.
pub(crate) fn extend_sparse_checkout<I, S>(
    leaf: &Path,
    paths: I,
    limits: TreeLimits,
) -> Result<(), GitError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let existing = load_sparse_meta(leaf)?.0;
    let new_paths: Vec<String> = normalize_sparse_paths(paths)
        .into_iter()
        .filter(|p| !existing.iter().any(|e| sparse_path_covers(e, p)))
        .collect();
    if new_paths.is_empty() {
        return Ok(());
    }

    let repo = Repository::open(leaf).map_err(|source| GitError::Object { source })?;
    let head_oid = repo
        .head()
        .map_err(|source| GitError::Object { source })?
        .peel_to_commit()
        .map_err(|source| GitError::Object { source })?
        .id();
    enforce_tree_limits(&repo, head_oid, &new_paths, limits)?;
    let tree = head_tree(&repo)?;
    let write_set = sparse_write_set(&repo, &tree, &new_paths, &existing)?;
    // Clearing only affects paths absent from the sparse metadata, so no
    // other resolution can be reading them yet. It removes leftovers from an
    // interrupted earlier extension.
    clear_sparse_paths(leaf, &write_set)?;
    apply_sparse_checkout(&repo, &write_set)?;

    let mut all = existing;
    all.extend(new_paths);
    let all: Vec<String> = all.into_iter().collect();
    save_sparse_meta(leaf, &all)
}

/// Restores the materialized sparse path `path` to match the leaf's HEAD
/// tree, returning whether anything was written.
///
/// Each on-disk entry is compared with its Git blob, so edits that keep the
/// file size and timestamp are still found. Only entries that differ are
/// rewritten and only entries absent from the tree are removed; content that
/// already matches is never touched, so this is safe to run while other
/// resolutions read the same folder.
pub(crate) fn reconcile_sparse_path(
    repo: &Repository,
    leaf: &Path,
    path: &str,
) -> Result<bool, GitError> {
    let tree = head_tree(repo)?;
    let subtree = sparse_subtree(repo, &tree, path)?;

    let mut blobs = Vec::new();
    let mut dirs = BTreeSet::new();
    let mut gitlinks = BTreeSet::new();
    let mut walk_error = None;
    subtree
        .walk(git2::TreeWalkMode::PreOrder, |root, entry| {
            let name = match tree_entry_name(entry) {
                Ok(name) => name,
                Err(error) => {
                    walk_error = Some(error);
                    return git2::TreeWalkResult::Abort;
                }
            };
            let rel = join_sparse_path(path, &format!("{root}{name}"));
            match entry.kind() {
                Some(git2::ObjectType::Tree) => {
                    dirs.insert(rel);
                }
                Some(git2::ObjectType::Blob) => {
                    blobs.push((rel, entry.id(), entry.filemode()));
                }
                _ => {
                    gitlinks.insert(rel);
                }
            }
            git2::TreeWalkResult::Ok
        })
        .map_err(|source| walk_error.take().unwrap_or(GitError::Object { source }))?;
    if let Some(error) = walk_error {
        return Err(error);
    }

    // Remove entries absent from the tree first. This includes a file or
    // symlink sitting where the tree has a directory, so the blob comparison
    // below never looks through a replaced parent directory.
    let tracked: BTreeSet<&str> = blobs.iter().map(|(rel, ..)| rel.as_str()).collect();
    let mut untracked = Vec::new();
    let root = if path == "." {
        leaf.to_path_buf()
    } else {
        leaf.join(path)
    };
    match std::fs::symlink_metadata(&root) {
        Ok(metadata) if path != "." && !metadata.is_dir() => untracked.push(root),
        _ => collect_untracked(leaf, path, &tracked, &dirs, &gitlinks, &mut untracked)?,
    }
    for path in &untracked {
        remove_worktree_path(path)?;
    }

    let mut stale = Vec::new();
    for (rel, oid, mode) in &blobs {
        if !worktree_entry_matches(repo, &leaf.join(rel), *oid, *mode)? {
            stale.push(rel.clone());
        }
    }

    if stale.is_empty() && untracked.is_empty() {
        return Ok(false);
    }
    tracing::warn!(
        cache_leaf = %leaf.display(),
        path,
        modified = stale.len(),
        untracked = untracked.len(),
        "restoring module cache content that does not match its Git commit"
    );
    for rel in &stale {
        remove_worktree_path(&leaf.join(rel))?;
    }
    apply_sparse_checkout(repo, &stale)?;
    Ok(true)
}

/// Returns whether the worktree entry at `path` matches a Git blob.
fn worktree_entry_matches(
    repo: &Repository,
    path: &Path,
    oid: git2::Oid,
    mode: i32,
) -> Result<bool, GitError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(false);
        }
        Err(source) => {
            return Err(GitError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    if mode == i32::from(git2::FileMode::Link) {
        if !metadata.file_type().is_symlink() {
            return Ok(false);
        }
        let target = std::fs::read_link(path).map_err(|source| GitError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let blob = repo
            .find_blob(oid)
            .map_err(|source| GitError::Object { source })?;
        return Ok(target.as_os_str().as_encoded_bytes() == blob.content());
    }
    if !metadata.is_file() {
        return Ok(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let executable = metadata.permissions().mode() & 0o111 != 0;
        if executable != (mode == i32::from(git2::FileMode::BlobExecutable)) {
            return Ok(false);
        }
    }
    let observed = git2::Oid::hash_file(git2::ObjectType::Blob, path)
        .map_err(|source| GitError::Object { source })?;
    Ok(observed == oid)
}

/// Collects worktree entries under the sparse path `path` that are absent
/// from its Git tree.
fn collect_untracked(
    leaf: &Path,
    path: &str,
    tracked: &BTreeSet<&str>,
    dirs: &BTreeSet<String>,
    gitlinks: &BTreeSet<String>,
    out: &mut Vec<std::path::PathBuf>,
) -> Result<(), GitError> {
    let dir = if path == "." {
        leaf.to_path_buf()
    } else {
        leaf.join(path)
    };
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(GitError::Io { path: dir, source }),
    };
    for entry in entries {
        let entry = entry.map_err(|source| GitError::Io {
            path: dir.clone(),
            source,
        })?;
        let name = entry.file_name();
        if path == "." && name == ".git" {
            continue;
        }
        let Some(name) = name.to_str() else {
            out.push(entry.path());
            continue;
        };
        let rel = join_sparse_path(path, name);
        if tracked.contains(rel.as_str()) || gitlinks.contains(&rel) {
            continue;
        }
        let file_type = entry.file_type().map_err(|source| GitError::Io {
            path: entry.path(),
            source,
        })?;
        if file_type.is_dir() && dirs.contains(&rel) {
            collect_untracked(leaf, &rel, tracked, dirs, gitlinks, out)?;
        } else {
            out.push(entry.path());
        }
    }
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

/// How [`ensure_materialized`] treats content already in a cache leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MaterializeMode {
    /// Adds missing module folders and never writes existing ones.
    Reuse,
    /// Also restores requested folders whose content differs from the
    /// pinned Git commit.
    Reconcile,
}

/// The outcome of [`ensure_materialized`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Materialized {
    /// A new cache leaf was cloned.
    Cloned,
    /// An existing cache leaf was reused without rewriting its content.
    Reused,
    /// An existing cache leaf was reused and some of its content was
    /// restored from Git.
    Reconciled,
}

/// Ensures `leaf` contains a sparse checkout of `url` at `commit`
/// covering at least `paths`. Clones if `leaf` does not yet exist;
/// otherwise extends the existing leaf's sparse-checkout set.
///
/// Content already materialized in an existing leaf is only rewritten in
/// [`MaterializeMode::Reconcile`], and then only where it differs from Git.
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
    mode: MaterializeMode,
) -> Result<Materialized, GitError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let paths: Vec<String> = paths.into_iter().map(|p| p.as_ref().to_string()).collect();
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
        extend_sparse_checkout(leaf, &paths, limits)?;
        if mode == MaterializeMode::Reconcile {
            let repo = Repository::open(leaf).map_err(|source| GitError::Object { source })?;
            let mut reconciled = false;
            for path in normalize_sparse_paths(&paths) {
                reconciled |= reconcile_sparse_path(&repo, leaf, &path)?;
            }
            if reconciled {
                return Ok(Materialized::Reconciled);
            }
        }
        Ok(Materialized::Reused)
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
        result.map(|()| Materialized::Cloned)
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
            MaterializeMode::Reuse,
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
            MaterializeMode::Reuse,
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
            MaterializeMode::Reuse,
        )
        .unwrap();
        assert_eq!(fetched, Materialized::Cloned);
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
            MaterializeMode::Reuse,
        )
        .unwrap();
        assert_eq!(fetched, Materialized::Reused);
        assert!(leaf.join("csvkit").join("module.json").exists());
        // Extending the leaf never rewrites a folder that is already
        // materialized, even when its content has been changed on disk.
        assert_eq!(
            std::fs::read(leaf.join("csvkit").join("index.wdl")).unwrap(),
            b"tampered"
        );
        assert!(leaf.join("csvkit").join("untracked.wdl").exists());
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
            MaterializeMode::Reuse,
        )
        .unwrap();
        assert_eq!(fetched, Materialized::Cloned);
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

        assert_eq!(
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
                MaterializeMode::Reuse,
            )
            .unwrap(),
            Materialized::Cloned
        );
        fs::write(sparse_meta_path(&leaf), b"not json").unwrap();

        assert_eq!(
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
                MaterializeMode::Reuse,
            )
            .unwrap(),
            Materialized::Cloned
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

    /// Returns a fetch policy with no transfer cap.
    fn open_fetch_policy() -> FetchPolicy {
        FetchPolicy {
            credentials: CredentialMode::Enabled,
            max_transfer_bytes: None,
        }
    }

    /// Materializes `paths` from `url` into `leaf` in the given mode.
    fn materialize(
        root: &Path,
        leaf: &Path,
        url: &Url,
        sha: &str,
        paths: &[&str],
        mode: MaterializeMode,
    ) -> Materialized {
        ensure_materialized(
            CacheLocation { root, leaf },
            url,
            sha,
            paths.iter().copied(),
            open_fetch_policy(),
            TreeLimits::default(),
            mode,
        )
        .unwrap()
    }

    /// Returns the inode of `path`, which changes when a file is replaced.
    #[cfg(unix)]
    fn inode(path: &Path) -> u64 {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(path).unwrap().ino()
    }

    /// Builds an upstream with two sibling modules and a top-level file.
    fn two_module_upstream() -> (tempfile::TempDir, String) {
        build_upstream(&[
            ("README.md", b"# repo"),
            (
                "csvkit/module.json",
                br#"{"name":"csvkit","license":"MIT"}"#,
            ),
            ("csvkit/index.wdl", b"workflow w {}"),
            ("csvkit/tasks/sort.wdl", b"task sort {}"),
            (
                "spellbook/module.json",
                br#"{"name":"spellbook","license":"MIT"}"#,
            ),
            ("spellbook/index.wdl", b"workflow w {}"),
        ])
    }

    #[test]
    fn sparse_path_coverage_is_component_based() {
        assert!(sparse_path_covers(".", "lib"));
        assert!(sparse_path_covers(".", "."));
        assert!(sparse_path_covers("lib", "lib"));
        assert!(sparse_path_covers("lib", "lib/common"));
        assert!(!sparse_path_covers("lib", "library"));
        assert!(!sparse_path_covers("lib", "lib2"));
        assert!(!sparse_path_covers("lib/common", "lib"));
        assert!(!sparse_path_covers("lib", "."));
        assert_eq!(
            normalize_sparse_paths(["lib/common", "lib", "library", "lib"]),
            vec!["lib".to_string(), "library".to_string()]
        );
        assert_eq!(normalize_sparse_paths(["a", "."]), vec![".".to_string()]);
    }

    #[test]
    fn extend_with_materialized_path_writes_nothing() {
        let (upstream, sha) = two_module_upstream();
        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        let first = materialize(
            dest.path(),
            &leaf,
            &url,
            &sha,
            &["csvkit"],
            MaterializeMode::Reuse,
        );
        assert_eq!(first, Materialized::Cloned);
        fs::write(leaf.join("csvkit/sentinel"), b"s").unwrap();
        let meta = fs::read(sparse_meta_path(&leaf)).unwrap();
        #[cfg(unix)]
        let (file_inode, meta_inode) = (
            inode(&leaf.join("csvkit/index.wdl")),
            inode(&sparse_meta_path(&leaf)),
        );

        for paths in [&["csvkit"][..], &["csvkit/tasks"][..]] {
            let again = materialize(
                dest.path(),
                &leaf,
                &url,
                &sha,
                paths,
                MaterializeMode::Reuse,
            );
            assert_eq!(again, Materialized::Reused);
        }

        assert!(leaf.join("csvkit/sentinel").exists());
        assert_eq!(fs::read(sparse_meta_path(&leaf)).unwrap(), meta);
        #[cfg(unix)]
        {
            assert_eq!(inode(&leaf.join("csvkit/index.wdl")), file_inode);
            assert_eq!(inode(&sparse_meta_path(&leaf)), meta_inode);
        }
    }

    #[test]
    fn root_path_materializes_the_whole_tree() {
        let (upstream, sha) = two_module_upstream();
        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        materialize(
            dest.path(),
            &leaf,
            &url,
            &sha,
            &["."],
            MaterializeMode::Reuse,
        );
        assert!(leaf.join("README.md").exists());
        assert!(leaf.join("csvkit/tasks/sort.wdl").exists());
        assert!(leaf.join("spellbook/index.wdl").exists());
        fs::write(leaf.join("spellbook/sentinel"), b"s").unwrap();

        // A path under an already-materialized `.` is covered and is a no-op.
        extend_sparse_checkout(&leaf, ["spellbook"], TreeLimits::default()).unwrap();
        assert!(leaf.join("spellbook/sentinel").exists());
        let meta = load_sparse_meta(&leaf).unwrap();
        assert_eq!(
            meta.0.into_iter().collect::<Vec<_>>(),
            vec![".".to_string()]
        );
    }

    #[test]
    fn extend_with_sibling_prefix_is_not_covered() {
        let (upstream, sha) = build_upstream(&[
            ("lib/module.json", br#"{"name":"lib"}"#),
            ("library/module.json", br#"{"name":"library"}"#),
        ]);
        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        materialize(
            dest.path(),
            &leaf,
            &url,
            &sha,
            &["lib"],
            MaterializeMode::Reuse,
        );
        assert!(!leaf.join("library").exists());
        extend_sparse_checkout(&leaf, ["library"], TreeLimits::default()).unwrap();
        assert!(leaf.join("library/module.json").exists());
    }

    #[test]
    fn extend_with_ancestor_path_keeps_existing_folders() {
        let (upstream, sha) = two_module_upstream();
        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        materialize(
            dest.path(),
            &leaf,
            &url,
            &sha,
            &["csvkit/tasks"],
            MaterializeMode::Reuse,
        );
        fs::write(leaf.join("csvkit/tasks/sentinel"), b"s").unwrap();
        #[cfg(unix)]
        let file_inode = inode(&leaf.join("csvkit/tasks/sort.wdl"));

        extend_sparse_checkout(&leaf, ["."], TreeLimits::default()).unwrap();

        assert!(leaf.join("README.md").exists());
        assert!(leaf.join("csvkit/index.wdl").exists());
        assert!(leaf.join("spellbook/index.wdl").exists());
        assert!(leaf.join("csvkit/tasks/sentinel").exists());
        #[cfg(unix)]
        assert_eq!(inode(&leaf.join("csvkit/tasks/sort.wdl")), file_inode);
        let meta: Vec<String> = load_sparse_meta(&leaf).unwrap().0.into_iter().collect();
        assert_eq!(meta, vec![".".to_string(), "csvkit/tasks".to_string()]);
    }

    #[test]
    fn reconcile_restores_only_content_that_differs() {
        let (upstream, sha) = two_module_upstream();
        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        materialize(
            dest.path(),
            &leaf,
            &url,
            &sha,
            &["csvkit", "spellbook"],
            MaterializeMode::Reuse,
        );
        // Same length as the original, so only a content comparison finds it.
        fs::write(leaf.join("csvkit/index.wdl"), b"workflow x {}").unwrap();
        fs::remove_file(leaf.join("csvkit/module.json")).unwrap();
        fs::write(leaf.join("csvkit/extra.wdl"), b"extra").unwrap();
        fs::create_dir_all(leaf.join("csvkit/tasks/nested")).unwrap();
        fs::write(leaf.join("csvkit/tasks/nested/extra.wdl"), b"extra").unwrap();
        fs::write(leaf.join("spellbook/index.wdl"), b"tampered").unwrap();
        #[cfg(unix)]
        let untouched_inode = inode(&leaf.join("csvkit/tasks/sort.wdl"));

        let outcome = materialize(
            dest.path(),
            &leaf,
            &url,
            &sha,
            &["csvkit"],
            MaterializeMode::Reconcile,
        );
        assert_eq!(outcome, Materialized::Reconciled);
        assert_eq!(
            fs::read(leaf.join("csvkit/index.wdl")).unwrap(),
            b"workflow w {}"
        );
        assert!(leaf.join("csvkit/module.json").exists());
        assert!(!leaf.join("csvkit/extra.wdl").exists());
        assert!(!leaf.join("csvkit/tasks/nested").exists());
        assert!(leaf.join("csvkit/tasks/sort.wdl").exists());
        #[cfg(unix)]
        assert_eq!(inode(&leaf.join("csvkit/tasks/sort.wdl")), untouched_inode);
        // Only the requested folder is reconciled.
        assert_eq!(
            fs::read(leaf.join("spellbook/index.wdl")).unwrap(),
            b"tampered"
        );

        let outcome = materialize(
            dest.path(),
            &leaf,
            &url,
            &sha,
            &["csvkit"],
            MaterializeMode::Reconcile,
        );
        assert_eq!(outcome, Materialized::Reused);
    }

    #[test]
    fn reconcile_of_clean_content_writes_nothing() {
        let (upstream, sha) = two_module_upstream();
        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        materialize(
            dest.path(),
            &leaf,
            &url,
            &sha,
            &["."],
            MaterializeMode::Reuse,
        );
        #[cfg(unix)]
        let inodes: Vec<u64> = ["README.md", "csvkit/index.wdl", "spellbook/index.wdl"]
            .iter()
            .map(|p| inode(&leaf.join(p)))
            .collect();

        for paths in [&["."][..], &["csvkit"][..]] {
            let outcome = materialize(
                dest.path(),
                &leaf,
                &url,
                &sha,
                paths,
                MaterializeMode::Reconcile,
            );
            assert_eq!(outcome, Materialized::Reused);
        }
        #[cfg(unix)]
        {
            let after: Vec<u64> = ["README.md", "csvkit/index.wdl", "spellbook/index.wdl"]
                .iter()
                .map(|p| inode(&leaf.join(p)))
                .collect();
            assert_eq!(after, inodes);
        }
    }

    #[cfg(unix)]
    #[test]
    fn reconcile_restores_file_mode() {
        use std::os::unix::fs::PermissionsExt;

        let (upstream, sha) = two_module_upstream();
        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        materialize(
            dest.path(),
            &leaf,
            &url,
            &sha,
            &["csvkit"],
            MaterializeMode::Reuse,
        );
        let path = leaf.join("csvkit/index.wdl");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();

        let outcome = materialize(
            dest.path(),
            &leaf,
            &url,
            &sha,
            &["csvkit"],
            MaterializeMode::Reconcile,
        );
        assert_eq!(outcome, Materialized::Reconciled);
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o111, 0);
    }

    #[test]
    fn reconcile_restores_directory_replaced_by_file() {
        let (upstream, sha) = two_module_upstream();
        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        materialize(
            dest.path(),
            &leaf,
            &url,
            &sha,
            &["csvkit"],
            MaterializeMode::Reuse,
        );
        fs::remove_dir_all(leaf.join("csvkit/tasks")).unwrap();
        fs::write(leaf.join("csvkit/tasks"), b"not a directory").unwrap();

        let outcome = materialize(
            dest.path(),
            &leaf,
            &url,
            &sha,
            &["csvkit"],
            MaterializeMode::Reconcile,
        );
        assert_eq!(outcome, Materialized::Reconciled);
        assert_eq!(
            fs::read(leaf.join("csvkit/tasks/sort.wdl")).unwrap(),
            b"task sort {}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn reconcile_restores_directory_replaced_by_symlink() {
        let (upstream, sha) = two_module_upstream();
        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        materialize(
            dest.path(),
            &leaf,
            &url,
            &sha,
            &["csvkit"],
            MaterializeMode::Reuse,
        );
        // A symlink to identical content must still be replaced, because the
        // module walker rejects symlinks.
        let copy = dest.path().join("copy");
        fs::create_dir_all(&copy).unwrap();
        fs::write(copy.join("sort.wdl"), b"task sort {}").unwrap();
        fs::remove_dir_all(leaf.join("csvkit/tasks")).unwrap();
        std::os::unix::fs::symlink(&copy, leaf.join("csvkit/tasks")).unwrap();

        let outcome = materialize(
            dest.path(),
            &leaf,
            &url,
            &sha,
            &["csvkit"],
            MaterializeMode::Reconcile,
        );
        assert_eq!(outcome, Materialized::Reconciled);
        let tasks = fs::symlink_metadata(leaf.join("csvkit/tasks")).unwrap();
        assert!(tasks.is_dir());
        assert!(leaf.join("csvkit/tasks/sort.wdl").is_file());
        assert!(copy.join("sort.wdl").exists());
    }

    /// Regression test for #1236: resolving imports from an already
    /// materialized leaf must not disturb concurrent readers of that leaf.
    #[test]
    fn concurrent_materialization_does_not_disturb_readers() {
        use std::sync::atomic::AtomicBool;
        use std::sync::atomic::Ordering;

        let (upstream, sha) = build_upstream(&[
            ("dep/module.json", br#"{"name":"dep","license":"MIT"}"#),
            ("dep/a.wdl", b"task a {}"),
            ("dep/b.wdl", b"task b {}"),
            ("dep/c.wdl", b"task c {}"),
            ("dep/d.wdl", b"task d {}"),
            ("other/module.json", br#"{"name":"other","license":"MIT"}"#),
        ]);
        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();
        materialize(
            dest.path(),
            &leaf,
            &url,
            &sha,
            &["dep"],
            MaterializeMode::Reuse,
        );

        let done = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let readers: Vec<_> = ["a", "b", "c", "d"]
                .into_iter()
                .map(|name| {
                    let path = leaf.join(format!("dep/{name}.wdl"));
                    let done = &done;
                    scope.spawn(move || {
                        let mut reads = 0usize;
                        while !done.load(Ordering::Relaxed) || reads == 0 {
                            let bytes = fs::read(&path)
                                .unwrap_or_else(|e| panic!("reading `{}`: {e}", path.display()));
                            assert!(!bytes.is_empty());
                            reads += 1;
                        }
                    })
                })
                .collect();
            let writers: Vec<_> = (0..4)
                .map(|i| {
                    let (root, leaf, url, sha) = (dest.path(), &leaf, &url, &sha);
                    scope.spawn(move || {
                        for iteration in 0..25 {
                            let paths: &[&str] = if i == 0 && iteration == 10 {
                                &["other"]
                            } else {
                                &["dep"]
                            };
                            materialize(root, leaf, url, sha, paths, MaterializeMode::Reuse);
                        }
                    })
                })
                .collect();
            for writer in writers {
                writer.join().unwrap();
            }
            done.store(true, Ordering::Relaxed);
            for reader in readers {
                reader.join().unwrap();
            }
        });
        assert!(leaf.join("other/module.json").exists());
    }
}
