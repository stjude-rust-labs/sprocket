//! Sparse checkout and materialization operations for Git cache leaves.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;

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

/// A repository-relative path selected by a sparse checkout.
///
/// Paths are compared by component, so `lib` covers `lib/common` but not
/// `library`. The root, written `.`, covers everything.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(from = "String", into = "String")]
enum SparsePath {
    /// The whole repository.
    Root,
    /// A `/`-separated path below the repository root.
    Sub(String),
}

impl SparsePath {
    /// Parses a sparse path, treating `.` and the empty string as the root.
    fn new<'a>(path: impl Into<Cow<'a, str>>) -> Self {
        let path = path.into();
        match path.as_ref() {
            "" | "." => Self::Root,
            _ => Self::Sub(path.into_owned()),
        }
    }

    /// Returns the path as written in sparse metadata.
    fn as_str(&self) -> &str {
        match self {
            Self::Root => ".",
            Self::Sub(path) => path,
        }
    }

    /// Returns the path below the root, or `None` for the root itself.
    fn as_sub(&self) -> Option<&str> {
        match self {
            Self::Root => None,
            Self::Sub(path) => Some(path),
        }
    }

    /// Returns whether materializing `self` also materializes `other`.
    fn covers(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Root, _) => true,
            (Self::Sub(_), Self::Root) => false,
            (Self::Sub(this), Self::Sub(other)) => other
                .strip_prefix(this.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/')),
        }
    }

    /// Returns the child path `name` below `self`.
    fn join(&self, name: &str) -> Self {
        match self {
            Self::Root => Self::Sub(name.to_string()),
            Self::Sub(path) => Self::Sub(format!("{path}/{name}")),
        }
    }

    /// Returns the proper ancestors of `self` below the root, outermost
    /// first.
    fn ancestors(&self) -> impl Iterator<Item = &str> {
        let path = self.as_sub().unwrap_or_default();
        path.match_indices('/').map(|(index, _)| &path[..index])
    }

    /// Returns whether this is the leaf's own `.git` directory.
    fn is_git_dir(&self) -> bool {
        self.as_sub() == Some(".git")
    }

    /// Returns where this path is materialized inside `leaf`.
    fn worktree_path(&self, leaf: &Path) -> PathBuf {
        match self {
            Self::Root => leaf.to_path_buf(),
            Self::Sub(path) => leaf.join(path),
        }
    }

    /// Deduplicates `paths` and drops any path covered by another one.
    fn normalize(paths: impl IntoIterator<Item = Self>) -> BTreeSet<Self> {
        let unique: BTreeSet<Self> = paths.into_iter().collect();
        unique
            .iter()
            .filter(|path| {
                !unique
                    .iter()
                    .any(|other| other != *path && other.covers(path))
            })
            .cloned()
            .collect()
    }
}

impl From<String> for SparsePath {
    fn from(path: String) -> Self {
        Self::new(path)
    }
}

impl From<SparsePath> for String {
    fn from(path: SparsePath) -> Self {
        match path {
            SparsePath::Root => ".".into(),
            SparsePath::Sub(path) => path,
        }
    }
}

impl fmt::Display for SparsePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

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
    let subtree = match SparsePath::new(path).as_sub() {
        None => root_tree,
        Some(sub) => {
            let entry = root_tree
                .get_path(Path::new(sub))
                .map_err(|source| GitError::Object { source })?;
            repo.find_tree(entry.id())
                .map_err(|source| GitError::Object { source })?
        }
    };
    let mut blob_oids = Vec::new();
    walk_tree(&subtree, |_, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            blob_oids.push(entry.id());
        }
        Ok(())
    })?;

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
fn enforce_tree_limits<'p>(
    repo: &Repository,
    oid: git2::Oid,
    paths: impl IntoIterator<Item = &'p SparsePath>,
    limits: TreeLimits,
) -> Result<(), GitError> {
    if limits.max_files.is_none() && limits.max_bytes.is_none() {
        return Ok(());
    }
    for path in paths {
        let stats = inspect_subtree_stats(repo, oid, path.as_str())?;
        let files_exceeded = limits.max_files.is_some_and(|limit| stats.files > limit);
        let bytes_exceeded = limits.max_bytes.is_some_and(|limit| stats.bytes > limit);
        if files_exceeded || bytes_exceeded {
            return Err(GitError::TreeLimitExceeded {
                path: path.to_string(),
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
fn apply_sparse_checkout<'p>(
    repo: &Repository,
    paths: impl IntoIterator<Item = &'p SparsePath>,
) -> Result<(), GitError> {
    let paths: Vec<&SparsePath> = paths.into_iter().collect();
    if paths.is_empty() {
        return Ok(());
    }
    let tree = head_tree(repo)?;

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
    if !paths.contains(&&SparsePath::Root) {
        for path in paths {
            checkout.path(path.as_str());
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
fn save_sparse_meta(leaf: &Path, paths: &BTreeSet<SparsePath>) -> Result<(), GitError> {
    let path = sparse_meta_path(leaf);
    let bytes = serde_json::to_vec_pretty(paths).map_err(|source| GitError::Json {
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

/// Reads the sparse paths recorded as materialized in a cache leaf,
/// returning an empty set if the metadata file is missing.
fn load_sparse_meta(leaf: &Path) -> Result<BTreeSet<SparsePath>, GitError> {
    let path = sparse_meta_path(leaf);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BTreeSet::new());
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
    let paths = SparsePath::normalize(paths.into_iter().map(|p| SparsePath::new(p.as_ref())));
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
    enforce_tree_limits(&repo, oid, &paths, limits)?;
    apply_sparse_checkout(&repo, &paths)?;
    save_sparse_meta(leaf, &paths)?;

    Ok(())
}

/// Returns the leaf's checked-out HEAD commit.
fn head_commit(repo: &Repository) -> Result<git2::Oid, GitError> {
    Ok(repo
        .head()
        .map_err(|source| GitError::Object { source })?
        .peel_to_commit()
        .map_err(|source| GitError::Object { source })?
        .id())
}

/// Returns the tree of the leaf's checked-out HEAD commit.
fn head_tree(repo: &Repository) -> Result<git2::Tree<'_>, GitError> {
    repo.head()
        .map_err(|source| GitError::Object { source })?
        .peel_to_tree()
        .map_err(|source| GitError::Object { source })
}

/// Walks every entry below `tree` in pre-order, passing each entry's parent
/// path relative to `tree` (empty or ending in `/`).
///
/// The walk stops at the first error returned by `visit`.
fn walk_tree(
    tree: &git2::Tree<'_>,
    mut visit: impl FnMut(&str, &git2::TreeEntry<'_>) -> Result<(), GitError>,
) -> Result<(), GitError> {
    let mut error = None;
    let result = tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
        match visit(root, entry) {
            Ok(()) => git2::TreeWalkResult::Ok,
            Err(e) => {
                error = Some(e);
                git2::TreeWalkResult::Abort
            }
        }
    });
    if let Some(error) = error {
        return Err(error);
    }
    result.map_err(|source| GitError::Object { source })
}

/// Returns a tree entry's name, rejecting names that are not UTF-8.
fn tree_entry_name<'e>(entry: &'e git2::TreeEntry<'_>) -> Result<&'e str, GitError> {
    entry.name().map_err(|_| GitError::Object {
        source: git2::Error::new(
            git2::ErrorCode::GenericError,
            git2::ErrorClass::Tree,
            "tree entry name is not valid UTF-8",
        ),
    })
}

/// The Git entries at and below one sparse path.
#[derive(Default)]
struct TreeEntries {
    /// Blobs by path, with their object ID and file mode.
    blobs: BTreeMap<SparsePath, (git2::Oid, i32)>,
    /// Directories.
    dirs: BTreeSet<SparsePath>,
    /// Submodule commits, which are never materialized.
    gitlinks: BTreeSet<SparsePath>,
}

impl TreeEntries {
    /// Records `entry` at `path`.
    fn add(&mut self, path: SparsePath, entry: &git2::TreeEntry<'_>) {
        match entry.kind() {
            Some(git2::ObjectType::Tree) => {
                self.dirs.insert(path);
            }
            Some(git2::ObjectType::Blob) => {
                self.blobs.insert(path, (entry.id(), entry.filemode()));
            }
            _ => {
                self.gitlinks.insert(path);
            }
        }
    }

    /// Returns whether `path` is tracked as something other than a
    /// directory.
    fn tracks_leaf_entry(&self, path: &SparsePath) -> bool {
        self.blobs.contains_key(path) || self.gitlinks.contains(path)
    }
}

/// What [`CacheLeaf::sync`] changed on disk.
#[derive(Clone, Copy, Debug, Default)]
struct SyncReport {
    /// Entries removed because Git does not track them.
    removed: usize,
    /// Files written from Git because they were missing or differed.
    restored: usize,
}

/// An existing cache leaf, opened while holding its leaf lock.
struct CacheLeaf<'a> {
    /// The leaf's worktree directory.
    path: &'a Path,
    /// The leaf's Git repository.
    repo: Repository,
    /// The sparse paths recorded as materialized.
    materialized: BTreeSet<SparsePath>,
}

impl<'a> CacheLeaf<'a> {
    /// Opens an existing cache leaf and reads its sparse metadata.
    fn open(path: &'a Path) -> Result<Self, GitError> {
        let repo = Repository::open(path).map_err(|source| GitError::Object { source })?;
        let materialized = load_sparse_meta(path)?;
        Ok(Self {
            path,
            repo,
            materialized,
        })
    }

    /// Materializes the sparse paths in `paths` that are not yet covered by
    /// the leaf's sparse metadata.
    ///
    /// Covered content is not rewritten: other resolutions may be reading it
    /// without holding the leaf lock. When nothing is new this writes
    /// nothing.
    fn extend(&mut self, paths: &BTreeSet<SparsePath>, limits: TreeLimits) -> Result<(), GitError> {
        let new: Vec<SparsePath> = paths
            .iter()
            .filter(|path| !self.materialized.iter().any(|m| m.covers(path)))
            .cloned()
            .collect();
        if new.is_empty() {
            return Ok(());
        }
        enforce_tree_limits(&self.repo, head_commit(&self.repo)?, &new, limits)?;
        // Syncing rather than checking out wholesale removes leftovers from an
        // interrupted earlier extension and leaves any materialized folders
        // inside the new paths untouched.
        for path in &new {
            self.sync(path)?;
        }
        self.materialized = SparsePath::normalize(self.materialized.iter().cloned().chain(new));
        save_sparse_meta(self.path, &self.materialized)
    }

    /// Restores the materialized sparse path `path` to match Git, returning
    /// whether anything was written.
    fn reconcile(&self, path: &SparsePath) -> Result<bool, GitError> {
        let report = self.sync(path)?;
        if report.removed == 0 && report.restored == 0 {
            return Ok(false);
        }
        tracing::warn!(
            cache_leaf = %self.path.display(),
            path = %path,
            restored = report.restored,
            removed = report.removed,
            "restored module cache content that did not match its Git commit"
        );
        Ok(true)
    }

    /// Makes the worktree at `path` match the leaf's HEAD tree.
    ///
    /// Each on-disk entry is compared with its Git blob, so edits that keep
    /// the file size and timestamp are still found. Only entries that are
    /// missing or differ are written, and only entries absent from the tree
    /// are removed. Content that already matches is never touched, so this
    /// is safe to run while other resolutions read the same folder.
    fn sync(&self, path: &SparsePath) -> Result<SyncReport, GitError> {
        let entries = self.tree_entries(path)?;

        // Remove entries absent from the tree first. This includes a file or
        // symlink sitting where the tree has a directory, so the blob
        // comparison below never looks through a replaced directory.
        let mut untracked = Vec::new();
        self.collect_untracked_ancestor(path, &mut untracked)?;
        if untracked.is_empty() {
            match std::fs::symlink_metadata(path.worktree_path(self.path)) {
                Ok(metadata) => {
                    self.collect_untracked(path, metadata.is_dir(), &entries, &mut untracked)?
                }
                Err(error) if is_absent(&error) => {}
                Err(source) => {
                    return Err(GitError::Io {
                        path: path.worktree_path(self.path),
                        source,
                    });
                }
            }
        }
        for entry in &untracked {
            remove_worktree_path(entry)?;
        }

        let mut stale = Vec::new();
        for (rel, &(oid, mode)) in &entries.blobs {
            let file = rel.worktree_path(self.path);
            if !self.blob_matches(&file, oid, mode)? {
                remove_worktree_path(&file)?;
                stale.push(rel.clone());
            }
        }
        apply_sparse_checkout(&self.repo, &stale)?;
        Ok(SyncReport {
            removed: untracked.len(),
            restored: stale.len(),
        })
    }

    /// Returns the Git entries at and below `path` in the HEAD tree.
    ///
    /// A path absent from the tree has no entries. A path below a tracked
    /// file or submodule is an error, so syncing it never removes that
    /// entry.
    fn tree_entries(&self, path: &SparsePath) -> Result<TreeEntries, GitError> {
        let tree = head_tree(&self.repo)?;
        let mut entries = TreeEntries::default();
        let subtree = match path.as_sub() {
            None => tree,
            Some(sub) => match tree.get_path(Path::new(sub)) {
                Ok(entry) if entry.kind() == Some(git2::ObjectType::Tree) => self
                    .repo
                    .find_tree(entry.id())
                    .map_err(|source| GitError::Object { source })?,
                Ok(entry) => {
                    entries.add(path.clone(), &entry);
                    return Ok(entries);
                }
                Err(error) if error.code() == git2::ErrorCode::NotFound => {
                    let below_non_tree = path.ancestors().any(|ancestor| {
                        tree.get_path(Path::new(ancestor))
                            .is_ok_and(|e| e.kind() != Some(git2::ObjectType::Tree))
                    });
                    if below_non_tree {
                        return Err(GitError::Object { source: error });
                    }
                    return Ok(entries);
                }
                Err(source) => return Err(GitError::Object { source }),
            },
        };
        entries.dirs.insert(path.clone());
        walk_tree(&subtree, |root, entry| {
            let rel = path.join(&format!("{root}{}", tree_entry_name(entry)?));
            entries.add(rel, entry);
            Ok(())
        })?;
        Ok(entries)
    }

    /// Collects the nearest ancestor of `path` that is on disk but is not a
    /// real directory, so nothing below it is read through a symlink.
    fn collect_untracked_ancestor(
        &self,
        path: &SparsePath,
        out: &mut Vec<PathBuf>,
    ) -> Result<(), GitError> {
        for ancestor in path.ancestors() {
            let dir = self.path.join(ancestor);
            match std::fs::symlink_metadata(&dir) {
                Ok(metadata) if metadata.is_dir() => {}
                Ok(_) => {
                    out.push(dir);
                    return Ok(());
                }
                Err(error) if is_absent(&error) => return Ok(()),
                Err(source) => return Err(GitError::Io { path: dir, source }),
            }
        }
        Ok(())
    }

    /// Collects the on-disk entry at `rel`, or entries below it, that are
    /// absent from `entries`.
    fn collect_untracked(
        &self,
        rel: &SparsePath,
        is_dir: bool,
        entries: &TreeEntries,
        out: &mut Vec<PathBuf>,
    ) -> Result<(), GitError> {
        if rel.is_git_dir() || entries.tracks_leaf_entry(rel) {
            return Ok(());
        }
        let dir = rel.worktree_path(self.path);
        if !is_dir || !entries.dirs.contains(rel) {
            out.push(dir);
            return Ok(());
        }
        let children = std::fs::read_dir(&dir).map_err(|source| GitError::Io {
            path: dir.clone(),
            source,
        })?;
        for child in children {
            let child = child.map_err(|source| GitError::Io {
                path: dir.clone(),
                source,
            })?;
            let Some(name) = child.file_name().to_str().map(str::to_string) else {
                out.push(child.path());
                continue;
            };
            let file_type = child.file_type().map_err(|source| GitError::Io {
                path: child.path(),
                source,
            })?;
            self.collect_untracked(&rel.join(&name), file_type.is_dir(), entries, out)?;
        }
        Ok(())
    }

    /// Returns whether the worktree entry at `path` matches a Git blob.
    fn blob_matches(&self, path: &Path, oid: git2::Oid, mode: i32) -> Result<bool, GitError> {
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if is_absent(&error) => return Ok(false),
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
            let blob = self
                .repo
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
}

/// Returns whether an I/O error means the path does not exist.
fn is_absent(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
    )
}

/// The state of a cache leaf found on disk.
enum LeafState<'a> {
    /// No leaf exists yet.
    Missing,
    /// The leaf is pinned to the expected commit and can be reused.
    Usable(CacheLeaf<'a>),
    /// The leaf must be evicted and cloned again, for the given reason.
    Evict(&'static str),
}

/// Inspects the cache leaf at `path`, expected to be pinned to `commit`.
fn inspect_leaf<'a>(path: &'a Path, commit: &str) -> Result<LeafState<'a>, GitError> {
    if !path.exists() {
        return Ok(LeafState::Missing);
    }
    let leaf = match CacheLeaf::open(path) {
        Ok(leaf) => leaf,
        // Only sparse metadata is parsed as JSON here.
        Err(GitError::Json { .. }) => return Ok(LeafState::Evict("corrupt sparse metadata")),
        Err(error) => return Err(error),
    };
    let expected = git2::Oid::from_str(commit).map_err(|source| GitError::Object { source })?;
    if head_commit(&leaf.repo)? != expected {
        return Ok(LeafState::Evict("an unexpected Git HEAD"));
    }
    Ok(LeafState::Usable(leaf))
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
    let paths = SparsePath::normalize(paths.into_iter().map(|p| SparsePath::new(p.as_ref())));
    let _cache_lock = lock_cache_root_shared(cache.root)?;
    let leaf = cache.leaf;
    tracing::debug!(
        cache_leaf = %leaf.display(),
        url = %url,
        commit,
        exists = leaf.exists(),
        "preparing module cache leaf"
    );
    tracing::trace!(cache_leaf = %leaf.display(), "acquiring module cache leaf lock");
    let _lock = lock_cache_leaf(leaf)?;
    tracing::trace!(cache_leaf = %leaf.display(), "acquired module cache leaf lock");

    let mut cached = match inspect_leaf(leaf, commit)? {
        LeafState::Usable(cached) => cached,
        LeafState::Missing => return clone_leaf(url, commit, leaf, &paths, fetch, limits),
        LeafState::Evict(reason) => {
            tracing::warn!(
                cache_leaf = %leaf.display(),
                commit,
                "evicting module cache leaf with {reason}"
            );
            clear_cache_leaf(leaf)?;
            return clone_leaf(url, commit, leaf, &paths, fetch, limits);
        }
    };

    tracing::debug!(
        cache_leaf = %leaf.display(),
        commit,
        "using cached module checkout"
    );
    cached.extend(&paths, limits)?;
    if mode == MaterializeMode::Reconcile {
        let mut reconciled = false;
        for path in &paths {
            reconciled |= cached.reconcile(path)?;
        }
        if reconciled {
            return Ok(Materialized::Reconciled);
        }
    }
    Ok(Materialized::Reused)
}

/// Clones a new cache leaf, removing it again if the clone fails.
fn clone_leaf(
    url: &Url,
    commit: &str,
    leaf: &Path,
    paths: &BTreeSet<SparsePath>,
    fetch: super::creds::FetchPolicy,
    limits: TreeLimits,
) -> Result<Materialized, GitError> {
    tracing::info!(
        cache_leaf = %leaf.display(),
        url = %url,
        commit,
        "fetching module into cache"
    );
    let result = clone_with_sparse_checkout(
        url,
        commit,
        leaf,
        paths.iter().map(SparsePath::as_str),
        fetch,
        limits,
    );
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

        assert_eq!(meta_paths(&leaf), vec!["csvkit"]);
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

        let error = extend(&leaf, &["spellbook"], TreeLimits::default()).unwrap_err();
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

        extend(&leaf, &["spellbook"], TreeLimits::default()).unwrap();
        assert!(leaf.join("spellbook").join("module.json").exists());
        assert!(leaf.join("csvkit").join("module.json").exists());
        assert_eq!(meta_paths(&leaf), vec!["csvkit", "spellbook"]);
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

        let err = extend(
            &leaf,
            &["big"],
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

    /// Extends the cache leaf at `leaf` to cover `paths`.
    fn extend(leaf: &Path, paths: &[&str], limits: TreeLimits) -> Result<(), GitError> {
        let paths = SparsePath::normalize(paths.iter().map(|p| SparsePath::new(*p)));
        CacheLeaf::open(leaf)?.extend(&paths, limits)
    }

    /// Returns the sparse paths recorded in the leaf's metadata.
    fn meta_paths(leaf: &Path) -> Vec<String> {
        load_sparse_meta(leaf)
            .unwrap()
            .into_iter()
            .map(String::from)
            .collect()
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
        let covers = |a: &str, b: &str| SparsePath::new(a).covers(&SparsePath::new(b));
        assert!(covers(".", "lib"));
        assert!(covers(".", "."));
        assert!(covers("lib", "lib"));
        assert!(covers("lib", "lib/common"));
        assert!(!covers("lib", "library"));
        assert!(!covers("lib", "lib2"));
        assert!(!covers("lib/common", "lib"));
        assert!(!covers("lib", "."));
        let normalize = |paths: &[&str]| -> Vec<String> {
            SparsePath::normalize(paths.iter().map(|p| SparsePath::new(*p)))
                .into_iter()
                .map(String::from)
                .collect()
        };
        assert_eq!(
            normalize(&["lib/common", "lib", "library", "lib"]),
            vec!["lib", "library"]
        );
        assert_eq!(normalize(&["a", "."]), vec!["."]);
        assert_eq!(normalize(&[""]), vec!["."]);
    }

    #[test]
    fn sparse_path_ancestors_are_outermost_first() {
        let ancestors: Vec<String> = SparsePath::new("a/b/c")
            .ancestors()
            .map(String::from)
            .collect();
        assert_eq!(ancestors, vec!["a", "a/b"]);
        assert_eq!(SparsePath::new("a").ancestors().count(), 0);
        assert_eq!(SparsePath::Root.ancestors().count(), 0);
        assert!(SparsePath::new(".git").is_git_dir());
        assert!(!SparsePath::new("lib/.git").is_git_dir());
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
        extend(&leaf, &["spellbook"], TreeLimits::default()).unwrap();
        assert!(leaf.join("spellbook/sentinel").exists());
        assert_eq!(meta_paths(&leaf), vec!["."]);
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
        extend(&leaf, &["library"], TreeLimits::default()).unwrap();
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
        #[cfg(unix)]
        let file_inode = inode(&leaf.join("csvkit/tasks/sort.wdl"));

        extend(&leaf, &["."], TreeLimits::default()).unwrap();

        assert!(leaf.join("README.md").exists());
        assert!(leaf.join("csvkit/index.wdl").exists());
        assert!(leaf.join("spellbook/index.wdl").exists());
        assert!(leaf.join("csvkit/tasks/sort.wdl").exists());
        #[cfg(unix)]
        assert_eq!(inode(&leaf.join("csvkit/tasks/sort.wdl")), file_inode);
        assert_eq!(meta_paths(&leaf), vec!["."]);
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

    #[cfg(unix)]
    #[test]
    fn reconcile_does_not_follow_a_symlinked_ancestor() {
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
        let outside = dest.path().join("outside");
        fs::create_dir_all(outside.join("tasks")).unwrap();
        fs::write(outside.join("tasks/keep.txt"), b"keep").unwrap();
        fs::remove_dir_all(leaf.join("csvkit")).unwrap();
        std::os::unix::fs::symlink(&outside, leaf.join("csvkit")).unwrap();

        let outcome = materialize(
            dest.path(),
            &leaf,
            &url,
            &sha,
            &["csvkit/tasks"],
            MaterializeMode::Reconcile,
        );
        assert_eq!(outcome, Materialized::Reconciled);
        assert!(outside.join("tasks/keep.txt").exists());
        assert!(fs::symlink_metadata(leaf.join("csvkit")).unwrap().is_dir());
        assert!(leaf.join("csvkit/tasks/sort.wdl").is_file());
    }

    #[test]
    fn extend_replaces_leftovers_from_an_interrupted_extension() {
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
        fs::create_dir_all(leaf.join("spellbook")).unwrap();
        fs::write(leaf.join("spellbook/index.wdl"), b"partial").unwrap();
        fs::write(leaf.join("spellbook/stray"), b"stray").unwrap();

        extend(&leaf, &["spellbook"], TreeLimits::default()).unwrap();
        assert_eq!(
            fs::read(leaf.join("spellbook/index.wdl")).unwrap(),
            b"workflow w {}"
        );
        assert!(leaf.join("spellbook/module.json").is_file());
        assert!(!leaf.join("spellbook/stray").exists());
    }

    #[test]
    fn reconcile_of_path_below_a_tracked_file_is_an_error() {
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
        let error = ensure_materialized(
            CacheLocation {
                root: dest.path(),
                leaf: &leaf,
            },
            &url,
            &sha,
            ["csvkit/index.wdl/x"],
            open_fetch_policy(),
            TreeLimits::default(),
            MaterializeMode::Reconcile,
        )
        .unwrap_err();
        assert!(matches!(error, GitError::Object { .. }), "got: {error}");
        assert_eq!(
            fs::read(leaf.join("csvkit/index.wdl")).unwrap(),
            b"workflow w {}"
        );
    }

    #[test]
    fn extend_with_path_absent_from_the_tree_writes_nothing() {
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
        extend(&leaf, &["missing"], TreeLimits::default()).unwrap();
        assert!(!leaf.join("missing").exists());
        assert!(!leaf.join("spellbook").exists());
        assert_eq!(meta_paths(&leaf), vec!["csvkit", "missing"]);
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
