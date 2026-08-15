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
