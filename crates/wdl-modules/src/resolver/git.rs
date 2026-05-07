//! Wrapper over `git2` covering the operations the resolver needs.
//! Handles credential delegation, partial clone via filtered fetch,
//! and sparse checkout of selected module folders within the cloned
//! tree.

use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

use git2::FetchOptions;
use git2::RemoteCallbacks;
use git2::Repository;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;
use url::Url;

/// File written into a cache leaf recording which module folders are
/// currently materialized via sparse checkout.
const SPARSE_META_FILENAME: &str = ".sparse.json";

/// The module folders currently materialized in a sparse-checkout cache
/// leaf.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(transparent)]
struct SparseMeta(BTreeSet<String>);

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
pub(crate) fn default_fetch_options<'fo>(mode: CredentialMode) -> FetchOptions<'fo> {
    let mut opts = FetchOptions::new();
    opts.remote_callbacks(default_callbacks(mode));
    opts
}

/// Creates a detached remote at `url` and connects it in the given
/// `direction` using [`default_callbacks`]. The caller is responsible
/// for `disconnect`ing (via [`disconnect_remote`]) when finished.
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

/// Clones the repository at `url` into `leaf`, checks out the working
/// tree to `commit`, and materializes only the listed `paths` from the
/// resulting tree.
///
/// `leaf` and any missing parent directories are created. Credentials
/// are obtained from libgit2's standard credential helper chain.
pub(crate) fn clone_with_sparse_checkout<I, S>(
    url: &Url,
    commit: &str,
    leaf: &Path,
    paths: I,
    mode: CredentialMode,
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

    let fetch_opts = default_fetch_options(mode);

    // Skip the default checkout; we'll do a path-filtered checkout below.
    let mut empty_checkout = git2::build::CheckoutBuilder::new();
    empty_checkout.disable_filters(true).dry_run();

    let mut builder = git2::build::RepoBuilder::new();
    builder
        .fetch_options(fetch_opts)
        .with_checkout(empty_checkout)
        .clone_local(git2::build::CloneLocal::Auto)
        .bare(false);

    let repo = builder.clone(url.as_str(), leaf).map_err(GitError::Git)?;

    let oid = git2::Oid::from_str(commit).map_err(GitError::Git)?;
    repo.set_head_detached(oid).map_err(GitError::Git)?;

    apply_sparse_checkout(&repo, &owned)?;
    save_sparse_meta(leaf, &owned)?;

    Ok(())
}

/// Ensures `leaf` contains a sparse checkout of `url` at `commit`
/// covering at least `paths`. Clones if `leaf` does not yet exist;
/// otherwise extends the existing leaf's sparse-checkout set.
///
/// Cached leaves are keyed by `(url, commit)` upstream, so an existing
/// leaf already corresponds to the requested commit; this helper does
/// not re-validate that.
pub(crate) fn ensure_materialized<I, S>(
    leaf: &Path,
    url: &Url,
    commit: &str,
    paths: I,
    mode: CredentialMode,
) -> Result<(), GitError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    if leaf.exists() {
        extend_sparse_checkout(leaf, paths)
    } else {
        clone_with_sparse_checkout(url, commit, leaf, paths, mode)
    }
}

/// Extends an existing sparse-checkout cache leaf to additionally
/// materialize the given `paths`. Paths already present are kept; the
/// union becomes the new sparse-checkout set.
pub(crate) fn extend_sparse_checkout<I, S>(leaf: &Path, paths: I) -> Result<(), GitError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let repo = Repository::open(leaf).map_err(GitError::Git)?;
    let mut all = load_sparse_meta(leaf)?.0;
    for p in paths {
        all.insert(p.as_ref().to_string());
    }
    let all_owned: Vec<String> = all.into_iter().collect();
    apply_sparse_checkout(&repo, &all_owned)?;
    save_sparse_meta(leaf, &all_owned)?;
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

/// Writes `.sparse.json` next to the cache leaf, recording which
/// module folders are currently materialized so a later
/// [`extend_sparse_checkout`] knows what to extend.
fn save_sparse_meta(leaf: &Path, paths: &[String]) -> Result<(), GitError> {
    let meta = SparseMeta(paths.iter().cloned().collect());
    let path = leaf.join(SPARSE_META_FILENAME);
    let bytes = serde_json::to_vec_pretty(&meta).map_err(|source| GitError::Json {
        path: path.clone(),
        source,
    })?;
    std::fs::write(&path, bytes).map_err(|source| GitError::Io { path, source })
}

/// Reads `.sparse.json` from the cache leaf, returning the default
/// empty meta if the file is missing.
fn load_sparse_meta(leaf: &Path) -> Result<SparseMeta, GitError> {
    let path = leaf.join(SPARSE_META_FILENAME);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SparseMeta::default());
        }
        Err(source) => return Err(GitError::Io { path, source }),
    };
    serde_json::from_slice(&bytes).map_err(|source| GitError::Json { path, source })
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
                br#"{"name":"csvkit","version":"1.0.0","license":"MIT"}"#,
            ),
            ("csvkit/index.wdl", b"workflow w {}"),
            (
                "spellbook/module.json",
                br#"{"name":"spellbook","version":"1.0.0","license":"MIT"}"#,
            ),
            ("spellbook/index.wdl", b"workflow w {}"),
        ]);

        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        clone_with_sparse_checkout(&url, &sha, &leaf, ["csvkit"], CredentialMode::Enabled).unwrap();

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
        let (upstream, _sha) = build_upstream(&[(
            "module.json",
            br#"{"name":"x","version":"1.0.0","license":"MIT"}"#,
        )]);
        let url = Url::from_directory_path(upstream.path()).unwrap();
        let err = list_advertised_refs(&url, 0, CredentialMode::Enabled).unwrap_err();
        assert!(
            matches!(err, GitError::RefLimitExceeded { .. }),
            "got: {err}"
        );
    }

    #[test]
    fn ensure_materialized_clones_then_extends() {
        let (upstream, sha) = build_upstream(&[
            (
                "csvkit/module.json",
                br#"{"name":"csvkit","version":"1.0.0","license":"MIT"}"#,
            ),
            ("csvkit/index.wdl", b"workflow w {}"),
            (
                "spellbook/module.json",
                br#"{"name":"spellbook","version":"1.0.0","license":"MIT"}"#,
            ),
            ("spellbook/index.wdl", b"workflow w {}"),
        ]);

        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        ensure_materialized(&leaf, &url, &sha, ["csvkit"], CredentialMode::Enabled).unwrap();
        assert!(leaf.join("csvkit").join("module.json").exists());
        assert!(!leaf.join("spellbook").exists());

        ensure_materialized(&leaf, &url, &sha, ["spellbook"], CredentialMode::Enabled).unwrap();
        assert!(leaf.join("csvkit").join("module.json").exists());
        assert!(leaf.join("spellbook").join("module.json").exists());
    }

    #[test]
    fn extend_adds_a_second_module_folder() {
        let (upstream, sha) = build_upstream(&[
            (
                "csvkit/module.json",
                br#"{"name":"csvkit","version":"1.0.0","license":"MIT"}"#,
            ),
            ("csvkit/index.wdl", b"workflow w {}"),
            (
                "spellbook/module.json",
                br#"{"name":"spellbook","version":"1.0.0","license":"MIT"}"#,
            ),
            ("spellbook/index.wdl", b"workflow w {}"),
        ]);

        let dest = tempdir().unwrap();
        let leaf = dest.path().join("leaf");
        let url = Url::from_directory_path(upstream.path()).unwrap();

        clone_with_sparse_checkout(&url, &sha, &leaf, ["csvkit"], CredentialMode::Enabled).unwrap();
        assert!(!leaf.join("spellbook").exists());

        extend_sparse_checkout(&leaf, ["spellbook"]).unwrap();
        assert!(leaf.join("spellbook").join("module.json").exists());
        assert!(leaf.join("csvkit").join("module.json").exists());

        let meta = load_sparse_meta(&leaf).unwrap();
        let mut paths: Vec<_> = meta.0.into_iter().collect();
        paths.sort();
        assert_eq!(paths, vec!["csvkit".to_string(), "spellbook".to_string()]);
    }
}
