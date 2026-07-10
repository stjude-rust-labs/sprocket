//! Wrapper over `git2` covering the operations the resolver needs.
//! Handles credential delegation, partial clone via filtered fetch,
//! and sparse checkout of selected module folders within the cloned
//! tree.
//!
//! ## Cache layout
//!
//! Each resolved Git dependency is materialized under the resolver's
//! `cache_root`. The directory structure is derived from the Git URL
//! and commit SHA by [`CacheKey`](super::cache::CacheKey):
//!
//! ```text
//! <cache_root>/
//!   <host>/                                        # structured layout
//!     <org>/
//!       <repo>-<digest8>/
//!         .<commit_sha>.lock                       # advisory file lock
//!         .<commit_sha>.sparse.json                # sparse-checkout metadata
//!         <commit_sha>/                            # the "cache leaf" — a clean Git checkout
//!           .git/
//!           csvkit/                                # a materialized module folder
//!             module.json
//!             index.wdl
//!           spellbook/                             # another module folder (added by extend)
//!             module.json
//!             index.wdl
//!   _opaque/                                       # fallback for URLs without host/org/repo
//!     <sha256(url)>/
//!       .<commit_sha>.lock
//!       .<commit_sha>.sparse.json
//!       <commit_sha>/
//!         .git/
//!         ...
//! ```
//!
//! The structured layout is used when the Git URL has a parseable
//! `<host>/<org>/<repo>` path. URLs that don't fit that shape
//! (IP-only hosts, deeply nested groups, etc.) fall back to the
//! `_opaque/` layout keyed by a SHA-256 digest of the URL.
//!
//! Both `.<commit>.lock` and `.<commit>.sparse.json` live next to the
//! cache leaf (in its parent directory), keeping the Git checkout
//! clean. `.sparse.json` tracks which module folders have been checked
//! out so far; when a second dependency in the same repository needs
//! a different folder, the existing checkout is extended rather than
//! re-cloned. The `.lock` file serializes concurrent operations via
//! `File::lock()`.

use std::collections::BTreeSet;
use std::fs::File;
use std::fs::TryLockError;
use std::path::Path;
use std::path::PathBuf;

use git2::FetchOptions;
use git2::RemoteCallbacks;
use git2::Repository;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;
use url::Url;

/// Extension appended to `.<leaf_name>` in the parent directory to
/// form the per-leaf sparse-checkout metadata path.
const SPARSE_META_EXT: &str = ".sparse.json";

/// Extension appended to `.<leaf_name>` in the parent directory to
/// form the per-leaf advisory lock path.
const LOCK_EXT: &str = ".lock";

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

/// Errors produced by the `git` module.
#[derive(Debug, Error)]
pub enum GitError {
    /// A `git2` operation failed.
    #[error("git operation failed")]
    Git(#[source] git2::Error),

    /// I/O error.
    #[error("i/o error at `{path}`")]
    Io {
        /// The path involved.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// JSON (de)serialization error for the sparse-meta file.
    #[error("sparse-checkout metadata error at `{path}`")]
    Json {
        /// The path involved.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: serde_json::Error,
    },

    /// The cache leaf path has no parent directory and cannot be created.
    #[error("cache leaf path `{0}` has no parent directory")]
    RootLeaf(PathBuf),

    /// A remote advertised more refs than the configured limit.
    #[error("remote at `{url}` advertised {count} refs, exceeding the limit of {limit}")]
    RefLimitExceeded {
        /// The remote URL.
        url: String,
        /// The number of refs advertised.
        count: usize,
        /// The configured limit.
        limit: usize,
    },

    /// A module subtree exceeds configured file or byte limits.
    #[error(
        "module subtree `{path}` exceeds tree limits (files: {files}, bytes: {bytes}, \
         max_files: {}, max_bytes: {})",
        max_files.map(|v| v.to_string()).as_deref().unwrap_or("unlimited"),
        max_bytes.map(|v| v.to_string()).as_deref().unwrap_or("unlimited"),
    )]
    TreeLimitExceeded {
        /// The module path within the repository.
        path: String,
        /// The number of files observed.
        files: usize,
        /// The total bytes observed.
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

    /// The remote default branch name was not valid UTF-8.
    #[error("remote at `{url}` advertised a non-UTF-8 default branch name")]
    DefaultBranchUtf8 {
        /// The remote URL.
        url: String,
        /// The underlying UTF-8 error.
        #[source]
        source: std::str::Utf8Error,
    },

    /// A commit-SHA prefix could not be resolved to a single commit in
    /// the repository (no match, or an ambiguous match).
    #[error("commit prefix `{prefix}` in `{url}` did not resolve to a unique commit")]
    CommitPrefix {
        /// The remote URL.
        url: String,
        /// The prefix that failed to resolve.
        prefix: String,
    },
}

/// Default credential resolver. Tries the user's configured Git
/// credential helper first, then falls back to ssh-agent for SSH URLs,
/// and finally to no credentials.
fn default_credentials(
    url: &str,
    username: Option<&str>,
    allowed: git2::CredentialType,
) -> Result<git2::Cred, git2::Error> {
    if let Ok(config) = git2::Config::open_default()
        && let Ok(cred) = git2::Cred::credential_helper(&config, url, username)
    {
        return Ok(cred);
    }
    if allowed.contains(git2::CredentialType::SSH_KEY) {
        return git2::Cred::ssh_key_from_agent(username.unwrap_or("git"));
    }
    git2::Cred::default()
}

/// Whether Git operations should use credential helpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CredentialMode {
    /// Use the user's configured Git credential helpers and ssh-agent.
    Enabled,
    /// Do not attach any credential callbacks.
    Disabled,
}

/// Builds a [`RemoteCallbacks`] wired up with credentials according
/// to `mode`.
pub(crate) fn default_callbacks<'cb>(mode: CredentialMode) -> RemoteCallbacks<'cb> {
    let mut cb = RemoteCallbacks::new();
    if mode == CredentialMode::Enabled {
        cb.credentials(default_credentials);
    }
    cb
}

/// Builds a [`FetchOptions`] preconfigured with [`default_callbacks`].
/// Proxy is left at the libgit2 default (`GIT_PROXY_NONE`), which
/// disables proxy usage for resolver-managed fetches.
pub(crate) fn default_fetch_options<'fo>(mode: CredentialMode) -> FetchOptions<'fo> {
    let mut opts = FetchOptions::new();
    opts.remote_callbacks(default_callbacks(mode));
    opts
}

/// Creates a detached remote at `url` and connects it in the given
/// `direction` using [`default_callbacks`]. Proxy is disabled
/// (`GIT_PROXY_NONE`). The caller is responsible for `disconnect`ing
/// (via [`disconnect_remote`]) when finished.
pub(crate) fn connect_remote(
    url: &Url,
    direction: git2::Direction,
    mode: CredentialMode,
) -> Result<git2::Remote<'_>, GitError> {
    let mut remote = git2::Remote::create_detached(url.as_str()).map_err(GitError::Git)?;
    remote
        .connect_auth(direction, Some(default_callbacks(mode)), None)
        .map_err(GitError::Git)?;
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
    mode: CredentialMode,
) -> Result<Vec<(String, String)>, GitError> {
    let mut remote = connect_remote(url, git2::Direction::Fetch, mode)?;
    let advertised = remote.list().map_err(GitError::Git)?;
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
    mode: CredentialMode,
    max_refs: usize,
) -> Result<String, GitError> {
    let mut remote = connect_remote(url, git2::Direction::Fetch, mode)?;
    let advertised = remote.list().map_err(GitError::Git)?;
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
/// [`GitFetcher::resolve_commit_prefix`](super::fetch::GitFetcher::resolve_commit_prefix),
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
    mode: CredentialMode,
) -> Result<String, GitError> {
    let mut fetch_opts = default_fetch_options(mode);
    fetch_opts.download_tags(git2::AutotagOption::All);
    let mut builder = git2::build::RepoBuilder::new();
    builder.bare(true).fetch_options(fetch_opts);
    let repo = builder
        .clone(url.as_str(), work_dir)
        .map_err(GitError::Git)?;

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

/// Inspects a subtree at `path` within the commit identified by `oid`,
/// counting blob entries and summing their sizes without materializing
/// any content to disk.
pub(crate) fn inspect_subtree_stats(
    repo: &Repository,
    oid: git2::Oid,
    path: &str,
) -> Result<GitTreeStats, GitError> {
    let commit = repo.find_commit(oid).map_err(GitError::Git)?;
    let root_tree = commit.tree().map_err(GitError::Git)?;
    let subtree = if path.is_empty() || path == "." {
        root_tree
    } else {
        let entry = root_tree.get_path(Path::new(path)).map_err(GitError::Git)?;
        repo.find_tree(entry.id()).map_err(GitError::Git)?
    };
    let mut stats = GitTreeStats::default();
    subtree
        .walk(git2::TreeWalkMode::PreOrder, |_, entry| {
            if entry.kind() == Some(git2::ObjectType::Blob) {
                stats.files += 1;
                if let Ok(blob) = repo.find_blob(entry.id()) {
                    stats.bytes = stats.bytes.saturating_add(blob.size() as u64);
                }
            }
            git2::TreeWalkResult::Ok
        })
        .map_err(GitError::Git)?;
    Ok(stats)
}

/// Checks that the tree statistics at each of the given `paths` fall
/// within the configured limits. Returns the first violation.
///
/// This runs after clone/fetch but before sparse checkout. It prevents
/// materializing oversized module trees but does not bound pack
/// transfer size or remote object negotiation. Full network-transfer
/// limits would require transport-level enforcement (e.g., libgit2
/// transfer-progress callbacks or a custom transport), which is not
/// yet implemented.
pub(crate) fn enforce_tree_limits(
    repo: &Repository,
    oid: git2::Oid,
    paths: &[String],
    max_files: Option<usize>,
    max_bytes: Option<u64>,
) -> Result<(), GitError> {
    if max_files.is_none() && max_bytes.is_none() {
        return Ok(());
    }
    for path in paths {
        let stats = inspect_subtree_stats(repo, oid, path)?;
        let files_exceeded = max_files.is_some_and(|limit| stats.files > limit);
        let bytes_exceeded = max_bytes.is_some_and(|limit| stats.bytes > limit);
        if files_exceeded || bytes_exceeded {
            return Err(GitError::TreeLimitExceeded {
                path: path.clone(),
                files: stats.files,
                bytes: stats.bytes,
                max_files,
                max_bytes,
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
        .map_err(GitError::Git)?
        .peel_to_commit()
        .map_err(GitError::Git)?;
    let tree = head_commit.tree().map_err(GitError::Git)?;

    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force().recreate_missing(true);
    for p in paths {
        // Match every entry under the given module folder.
        checkout.path(format!("{p}/**"));
    }
    repo.checkout_tree(tree.as_object(), Some(&mut checkout))
        .map_err(GitError::Git)?;

    Ok(())
}

/// Returns the path to the sparse-checkout metadata file for a cache
/// leaf. The file is placed in the leaf's parent directory as
/// `.<leaf_name>.sparse.json`.
fn sparse_meta_path(leaf: &Path) -> PathBuf {
    let name = leaf
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    leaf.with_file_name(format!(".{name}{SPARSE_META_EXT}"))
}

/// Writes the sparse-checkout metadata next to the cache leaf.
fn save_sparse_meta(leaf: &Path, paths: &[String]) -> Result<(), GitError> {
    let meta = SparseMeta(paths.iter().cloned().collect());
    let path = sparse_meta_path(leaf);
    let bytes = serde_json::to_vec_pretty(&meta).map_err(|source| GitError::Json {
        path: path.clone(),
        source,
    })?;
    std::fs::write(&path, bytes).map_err(|source| GitError::Io { path, source })
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

/// Clones the repository at `url` into `leaf`, checks out the working
/// tree to `commit`, and materializes only the listed `paths` from the
/// resulting tree.
///
/// `leaf` and any missing parent directories are created. Credentials
/// are obtained from libgit2's standard credential helper chain.
///
/// When `max_files` or `max_bytes` are set, the selected module
/// subtrees are inspected via Git tree objects after clone but before
/// sparse checkout. This bounds the materialized content but not the
/// network transfer itself.
pub(crate) fn clone_with_sparse_checkout<I, S>(
    url: &Url,
    commit: &str,
    leaf: &Path,
    paths: I,
    mode: CredentialMode,
    max_files: Option<usize>,
    max_bytes: Option<u64>,
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

    // Shallow-fetch remote URLs (depth 1) to reduce bandwidth since
    // we only need the single resolved commit. Local `file://` URLs
    // skip this because there is no network transfer to optimize.
    let mut fetch_opts = default_fetch_options(mode);
    if url.scheme() != "file" {
        fetch_opts.depth(1);
    }

    // Dry-run the checkout so the clone fetches objects without
    // writing the full tree to disk. `apply_sparse_checkout`
    // materializes only the requested module folders afterward.
    let mut empty_checkout = git2::build::CheckoutBuilder::new();
    empty_checkout.disable_filters(true).dry_run();

    // `bare(false)` gives us a working tree (not just the object
    // database) so `apply_sparse_checkout` can write files to disk.
    // `CloneLocal::Auto` lets libgit2 hardlink objects when the
    // source is on the same filesystem.
    let mut builder = git2::build::RepoBuilder::new();
    builder
        .fetch_options(fetch_opts)
        .with_checkout(empty_checkout)
        .clone_local(git2::build::CloneLocal::Auto)
        .bare(false);

    let repo = builder.clone(url.as_str(), leaf).map_err(GitError::Git)?;

    // The shallow clone above fetches only the remote's default HEAD.
    // If the resolved commit lives on a different branch or tag, it
    // won't be present in the local object store. Fall back to an
    // explicit fetch of the exact OID so the detach succeeds.
    let oid = git2::Oid::from_str(commit).map_err(GitError::Git)?;
    if repo.set_head_detached(oid).is_err() {
        let mut fetch_opts = default_fetch_options(mode);
        // `+<src>:<dst>`: the `+` forces the update, `<src>` is the
        // remote OID we need, and `<dst>` parks it under a local ref so
        // libgit2 writes the object into the local store.
        let refspec = format!("+{commit}:refs/fetched/{commit}");
        repo.remote_anonymous(url.as_str())
            .map_err(GitError::Git)?
            .fetch(&[&refspec], Some(&mut fetch_opts), None)
            .map_err(GitError::Git)?;
        repo.set_head_detached(oid).map_err(GitError::Git)?;
    }

    enforce_tree_limits(&repo, oid, &owned, max_files, max_bytes)?;

    apply_sparse_checkout(&repo, &owned)?;
    save_sparse_meta(leaf, &owned)?;

    Ok(())
}

/// Extends an existing sparse-checkout cache leaf to additionally
/// materialize the given `paths`. Paths already present are kept; the
/// union becomes the new sparse-checkout set.
pub(crate) fn extend_sparse_checkout<I, S>(
    leaf: &Path,
    paths: I,
    max_files: Option<usize>,
    max_bytes: Option<u64>,
) -> Result<(), GitError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let repo = Repository::open(leaf).map_err(GitError::Git)?;
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
            .map_err(GitError::Git)?
            .peel_to_commit()
            .map_err(GitError::Git)?
            .id();
        enforce_tree_limits(&repo, head_oid, &new_paths, max_files, max_bytes)?;
    }
    let all_owned: Vec<String> = all.into_iter().collect();
    apply_sparse_checkout(&repo, &all_owned)?;
    save_sparse_meta(leaf, &all_owned)?;
    Ok(())
}

/// Acquires an exclusive file lock for a cache leaf directory.
///
/// The lock file is placed next to the leaf (in its parent directory)
/// as `.<leaf_name>.lock` so it can be created before the leaf itself
/// exists. The lock is released when the returned [`File`] handle is
/// dropped; the lock file remains on disk to avoid delete-after-unlock
/// races.
fn lock_cache_leaf(leaf: &Path) -> Result<File, GitError> {
    // Cache leaves are always `<parent>/<commit_sha>`, so
    // both `parent()` and `file_name()` are always `Some`.
    let parent = leaf.parent().unwrap();
    std::fs::create_dir_all(parent).map_err(|source| GitError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let name = leaf.file_name().unwrap().to_string_lossy();
    let lock_path = parent.join(format!(".{name}{LOCK_EXT}"));
    let file = File::create(&lock_path).map_err(|source| GitError::Io {
        path: lock_path.clone(),
        source,
    })?;
    // Try the lock first so we can tell the user why `sprocket` appears to
    // hang when another process already holds it before blocking on it.
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            tracing::info!(
                "waiting to acquire exclusive lock on Git cache leaf `{leaf}`",
                leaf = leaf.display(),
            );
            file.lock().map_err(|source| GitError::Io {
                path: lock_path.clone(),
                source,
            })?;
        }
        Err(TryLockError::Error(source)) => {
            return Err(GitError::Io {
                path: lock_path,
                source,
            });
        }
    }
    Ok(file)
}

/// Ensures `leaf` contains a sparse checkout of `url` at `commit`
/// covering at least `paths`. Clones if `leaf` does not yet exist;
/// otherwise extends the existing leaf's sparse-checkout set.
///
/// Cached leaves are keyed by `(url, commit)` upstream, so an existing
/// leaf already corresponds to the requested commit; this helper does
/// not re-validate that.
///
/// If the initial clone fails, the partially-written leaf is removed so a
/// corrupt checkout does not persist.
pub(crate) fn ensure_materialized<I, S>(
    leaf: &Path,
    url: &Url,
    commit: &str,
    paths: I,
    mode: CredentialMode,
    max_files: Option<usize>,
    max_bytes: Option<u64>,
) -> Result<bool, GitError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
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
    if leaf.exists() {
        tracing::debug!(
            cache_leaf = %leaf.display(),
            commit,
            "using cached module checkout"
        );
        extend_sparse_checkout(leaf, paths, max_files, max_bytes)?;
        Ok(false)
    } else {
        tracing::info!(
            cache_leaf = %leaf.display(),
            url = %url,
            commit,
            "fetching module into cache"
        );
        let result =
            clone_with_sparse_checkout(url, commit, leaf, paths, mode, max_files, max_bytes);
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

    fn build_upstream(files: &[(&str, &[u8])]) -> (tempfile::TempDir, String) {
        let upstream = tempdir().unwrap();
        let repo = Repository::init(upstream.path()).unwrap();
        for (rel, bytes) in files {
            let abs = upstream.path().join(rel);
            if let Some(parent) = abs.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&abs, bytes).unwrap();
        }
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = Signature::now("test", "test@example.com").unwrap();
        let oid = repo
            .commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        (upstream, oid.to_string())
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
            CredentialMode::Enabled,
            None,
            None,
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
    fn ref_count_limit_is_enforced() {
        let (upstream, _sha) =
            build_upstream(&[("module.json", br#"{"name":"x","license":"MIT"}"#)]);
        let url = Url::from_directory_path(upstream.path()).unwrap();
        let err = list_advertised_refs(&url, 0, CredentialMode::Enabled).unwrap_err();
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
        let observed = discover_default_branch(&url, CredentialMode::Enabled, 1024).unwrap();
        assert_eq!(observed, expected);
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
            &leaf,
            &url,
            &sha,
            ["csvkit"],
            CredentialMode::Enabled,
            None,
            None,
        )
        .unwrap();
        assert!(fetched);
        assert!(leaf.join("csvkit").join("module.json").exists());
        assert!(!leaf.join("spellbook").exists());

        let fetched = ensure_materialized(
            &leaf,
            &url,
            &sha,
            ["spellbook"],
            CredentialMode::Enabled,
            None,
            None,
        )
        .unwrap();
        assert!(!fetched);
        assert!(leaf.join("csvkit").join("module.json").exists());
        assert!(leaf.join("spellbook").join("module.json").exists());
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
            CredentialMode::Enabled,
            None,
            None,
        )
        .unwrap();
        assert!(!leaf.join("spellbook").exists());

        extend_sparse_checkout(&leaf, ["spellbook"], None, None).unwrap();
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
            CredentialMode::Enabled,
            Some(2),
            None,
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
            CredentialMode::Enabled,
            None,
            Some(512),
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
            CredentialMode::Enabled,
            Some(100),
            Some(100_000),
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
            CredentialMode::Enabled,
            None,
            None,
        )
        .unwrap();

        let err = extend_sparse_checkout(&leaf, ["big"], Some(2), None).unwrap_err();
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
            CredentialMode::Enabled,
            None,
            None,
        )
        .unwrap();

        assert!(
            leaf_path.join("mod_b").join("b.txt").exists(),
            "checkout should contain the file from the non-default branch"
        );
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
            CredentialMode::Enabled,
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
            CredentialMode::Enabled,
        )
        .unwrap_err();
        assert!(
            matches!(err, GitError::CommitPrefix { .. }),
            "expected `CommitPrefix`, got: {err}"
        );
    }
}
