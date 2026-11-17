//! Shared test fixtures for Git operations.

#![cfg(test)]

use std::fs;

use git2::Repository;
use git2::Signature;
use tempfile::tempdir;

pub(super) fn build_upstream(files: &[(&str, &[u8])]) -> (tempfile::TempDir, String) {
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

/// Git file modes used when writing raw tree objects.
pub(super) mod mode {
    /// A regular file.
    pub const BLOB: &str = "100644";
    /// A directory.
    pub const TREE: &str = "40000";
    /// A symbolic link.
    pub const LINK: &str = "120000";
    /// A submodule commit.
    pub const GITLINK: &str = "160000";
}

/// Writes a blob holding `content` and returns its object ID.
pub(super) fn raw_blob(repo: &Repository, content: &[u8]) -> git2::Oid {
    repo.blob(content).unwrap()
}

/// Writes a tree object with `entries` exactly as given, returning its ID.
///
/// Unlike `git2::TreeBuilder`, this keeps duplicate names, unsorted
/// entries, and names that Git itself refuses to write, so tests can
/// reproduce trees served by a hostile remote.
pub(super) fn raw_tree(repo: &Repository, entries: &[(&str, &[u8], git2::Oid)]) -> git2::Oid {
    let mut bytes = Vec::new();
    for (mode, name, oid) in entries {
        bytes.extend_from_slice(mode.as_bytes());
        bytes.push(b' ');
        bytes.extend_from_slice(name);
        bytes.push(0);
        bytes.extend_from_slice(oid.as_bytes());
    }
    repo.odb()
        .unwrap()
        .write(git2::ObjectType::Tree, &bytes)
        .unwrap()
}

/// Initializes an upstream repository whose `HEAD` commit has the tree
/// returned by `build`, returning the repository and the commit SHA.
///
/// The commit is written as a raw object so the tree is never parsed.
pub(super) fn raw_upstream(
    build: impl FnOnce(&Repository) -> git2::Oid,
) -> (tempfile::TempDir, String) {
    let upstream = tempdir().unwrap();
    let repo = Repository::init(upstream.path()).unwrap();
    let tree = build(&repo);
    let commit = format!(
        "tree {tree}\nauthor test <test@example.com> 0 +0000\ncommitter test <test@example.com> 0 \
         +0000\n\nhostile\n"
    );
    let oid = repo
        .odb()
        .unwrap()
        .write(git2::ObjectType::Commit, commit.as_bytes())
        .unwrap();
    repo.reference("refs/heads/main", oid, true, "hostile")
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
    (upstream, oid.to_string())
}
