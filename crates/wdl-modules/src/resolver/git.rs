//! Wrapper over `git2` covering the operations the resolver needs:
//! credential delegation, partial clone via filtered fetch, and sparse
//! checkout of selected module folders within the cloned tree.

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

/// Clones the repository at `url` into `leaf`, checks out the working
/// tree to `commit`, and materializes only the listed `paths` from the
/// resulting tree (libgit2's path-filtered checkout, the durable
/// equivalent of git's sparse-checkout).
///
/// `leaf` and any missing parent directories are created. Credentials
/// are obtained from libgit2's standard credential helper chain.
pub(crate) fn clone_with_sparse_checkout(
    url: &Url,
    commit: &str,
    leaf: &Path,
    paths: &[&str],
) -> Result<(), GitError> {
    if let Some(parent) = leaf.parent() {
        std::fs::create_dir_all(parent).map_err(|source| GitError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(default_credentials);

    let mut fetch_opts = FetchOptions::new();
    fetch_opts.remote_callbacks(callbacks);

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

    apply_sparse_checkout(&repo, paths)?;
    save_sparse_meta(leaf, paths)?;

    Ok(())
}

/// Extends an existing sparse-checkout cache leaf to additionally
/// materialize the given `paths`. Paths already present are kept; the
/// union becomes the new sparse-checkout set.
pub(crate) fn extend_sparse_checkout(leaf: &Path, paths: &[&str]) -> Result<(), GitError> {
    let repo = Repository::open(leaf).map_err(GitError::Git)?;
    let mut all = load_sparse_meta(leaf)?.0;
    for p in paths {
        all.insert((*p).to_string());
    }
    let all_refs: Vec<&str> = all.iter().map(String::as_str).collect();
    apply_sparse_checkout(&repo, &all_refs)?;
    save_sparse_meta(leaf, &all_refs)?;
    Ok(())
}

/// Materializes only the listed module folders from the repo's HEAD
/// tree using libgit2's path-filtered checkout.
fn apply_sparse_checkout(repo: &Repository, paths: &[&str]) -> Result<(), GitError> {
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

/// Writes `_sparse.json` next to the cache leaf, recording which
/// module folders are currently materialized so a later
/// [`extend_sparse_checkout`] knows what to extend.
fn save_sparse_meta(leaf: &Path, paths: &[&str]) -> Result<(), GitError> {
    let meta = SparseMeta(paths.iter().map(|s| (*s).to_string()).collect());
    let path = leaf.join(SPARSE_META_FILENAME);
    let bytes = serde_json::to_vec_pretty(&meta).map_err(|source| GitError::Json {
        path: path.clone(),
        source,
    })?;
    std::fs::write(&path, bytes).map_err(|source| GitError::Io { path, source })
}

/// Reads `_sparse.json` from the cache leaf, returning the default
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

/// Errors produced by the `git` module.
#[derive(Debug, Error)]
pub(crate) enum GitError {
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

        clone_with_sparse_checkout(&url, &sha, &leaf, &["csvkit"]).unwrap();

        assert!(leaf.join("csvkit").join("module.json").exists());
        assert!(!leaf.join("spellbook").exists());

        let meta = load_sparse_meta(&leaf).unwrap();
        assert_eq!(meta.0.iter().cloned().collect::<Vec<_>>(), vec![
            "csvkit".to_string()
        ]);
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

        clone_with_sparse_checkout(&url, &sha, &leaf, &["csvkit"]).unwrap();
        assert!(!leaf.join("spellbook").exists());

        extend_sparse_checkout(&leaf, &["spellbook"]).unwrap();
        assert!(leaf.join("spellbook").join("module.json").exists());
        assert!(leaf.join("csvkit").join("module.json").exists());

        let meta = load_sparse_meta(&leaf).unwrap();
        let mut paths: Vec<_> = meta.0.into_iter().collect();
        paths.sort();
        assert_eq!(paths, vec!["csvkit".to_string(), "spellbook".to_string()]);
    }
}
