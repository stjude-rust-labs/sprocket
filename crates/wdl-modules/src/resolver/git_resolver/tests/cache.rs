use std::fs;
use std::path::Path;
use std::path::PathBuf;

use tempfile::TempDir;
use tempfile::tempdir;

use super::checksum;
use super::locked_git_entry_with;
use super::module;
use super::resolver;
use super::resolver_with_lockfile;
use super::write_manifest;
use crate::Lockfile;
use crate::Manifest;
use crate::dependency::GitSelector;
use crate::lockfile::DependencyEntry;
use crate::lockfile::GitCommit;
use crate::lockfile::ResolvedSource;
use crate::module::Module;
use crate::resolver::ResolverError;
use crate::resolver::cache::CacheKey;
use crate::resolver::git::CACHE_MARKER_FILENAME;
use crate::resolver::git::GitError;

fn consumer(root: &Path) -> Module {
    write_manifest(root, "consumer", "0.1.0", &[]);
    let manifest =
        Manifest::parse(&fs::read(root.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    module(manifest, root)
}

fn cache_leaf(cache: &TempDir, url: &str, sha: &str) -> PathBuf {
    let url = url.parse().unwrap();
    let sha: GitCommit = sha.parse().unwrap();
    CacheKey::from_git_url(&url, &sha).absolute_path(cache.path())
}

fn write_cache_leaf(leaf: &Path, name: &str) {
    fs::create_dir_all(leaf).unwrap();
    write_manifest(leaf, name, "1.0.0", &[]);
    fs::write(leaf.join("index.wdl"), format!("workflow {name} {{}}")).unwrap();
}

#[test]
fn initialize_cache_creates_owned_cache_root_with_marker() {
    let cache = tempdir().unwrap();
    let r = resolver(&cache);

    r.initialize_cache().unwrap();

    assert!(cache.path().join(CACHE_MARKER_FILENAME).is_file());
}

#[test]
fn initialize_cache_rejects_nonempty_root_without_marker() {
    let cache = tempdir().unwrap();
    fs::write(cache.path().join("stray.txt"), b"no marker").unwrap();

    let err = resolver(&cache).initialize_cache().unwrap_err();
    assert!(matches!(
        err,
        ResolverError::Git(GitError::UnsafeCacheRoot {
            reason: "the directory is non-empty and has no ownership marker",
            ..
        })
    ));
}

#[test]
fn locked_cache_leaves_collects_reachable_git_entries() {
    let cache = tempdir().unwrap();
    let consumer = consumer(&cache.path().join("consumer"));

    let mut parent_entry = DependencyEntry {
        source: ResolvedSource::Path {
            path: cache.path().join("local-parent"),
        },
        checksum: Some(checksum()),
        signer: None,
        dependencies: Default::default(),
    };
    parent_entry.dependencies.insert(
        "child".parse().unwrap(),
        locked_git_entry_with(
            "https://github.com/openwdl/child",
            "0000000000000000000000000000000000000002",
            GitSelector::Commit("0000000000000000000000000000000000000002".parse().unwrap()),
        ),
    );

    let mut lockfile = Lockfile::default();
    lockfile.dependencies.insert(
        "direct".parse().unwrap(),
        locked_git_entry_with(
            "https://github.com/openwdl/direct",
            "0000000000000000000000000000000000000001",
            GitSelector::Commit("0000000000000000000000000000000000000001".parse().unwrap()),
        ),
    );
    lockfile
        .dependencies
        .insert("parent".parse().unwrap(), parent_entry);

    let r = resolver_with_lockfile(&cache, lockfile);
    let mut leaves = r.locked_cache_leaves(&consumer).unwrap();
    leaves.sort();

    assert_eq!(
        leaves,
        vec![
            cache_leaf(
                &cache,
                "https://github.com/openwdl/child",
                "0000000000000000000000000000000000000002"
            ),
            cache_leaf(
                &cache,
                "https://github.com/openwdl/direct",
                "0000000000000000000000000000000000000001"
            ),
        ]
    );
}

#[test]
fn locked_cache_leaves_deduplicates_shared_git_leaves() {
    let cache = tempdir().unwrap();
    let consumer = consumer(&cache.path().join("consumer"));
    let shared = locked_git_entry_with(
        "https://github.com/openwdl/tasks",
        "0000000000000000000000000000000000000001",
        GitSelector::Commit("0000000000000000000000000000000000000001".parse().unwrap()),
    );

    let mut parent_entry = DependencyEntry {
        source: ResolvedSource::Path {
            path: cache.path().join("local-parent"),
        },
        checksum: Some(checksum()),
        signer: None,
        dependencies: Default::default(),
    };
    parent_entry
        .dependencies
        .insert("child".parse().unwrap(), shared.clone());

    let mut lockfile = Lockfile::default();
    lockfile
        .dependencies
        .insert("direct".parse().unwrap(), shared);
    lockfile
        .dependencies
        .insert("parent".parse().unwrap(), parent_entry);

    let r = resolver_with_lockfile(&cache, lockfile);
    let leaves = r.locked_cache_leaves(&consumer).unwrap();
    assert_eq!(leaves.len(), 1);
    assert_eq!(
        leaves[0],
        cache_leaf(
            &cache,
            "https://github.com/openwdl/tasks",
            "0000000000000000000000000000000000000001"
        )
    );
}

#[test]
fn clean_locked_cache_removes_only_reachable_locked_leaves() {
    let cache = tempdir().unwrap();
    let workdir = tempdir().unwrap();
    let consumer = consumer(&workdir.path().join("consumer"));

    let mut lockfile = Lockfile::default();
    lockfile.dependencies.insert(
        "dep".parse().unwrap(),
        locked_git_entry_with(
            "https://github.com/openwdl/tasks",
            "0000000000000000000000000000000000000001",
            GitSelector::Commit("0000000000000000000000000000000000000001".parse().unwrap()),
        ),
    );
    let r = resolver_with_lockfile(&cache, lockfile);
    r.initialize_cache().unwrap();

    let reachable = cache_leaf(
        &cache,
        "https://github.com/openwdl/tasks",
        "0000000000000000000000000000000000000001",
    );
    let extra = cache_leaf(
        &cache,
        "https://github.com/openwdl/other",
        "0000000000000000000000000000000000000002",
    );
    write_cache_leaf(&reachable, "dep");
    write_cache_leaf(&extra, "other");

    let stats = r.clean_locked_cache(&consumer).unwrap();
    assert_eq!(stats.modules, 1);
    assert!(stats.bytes > 0);
    assert!(!reachable.exists());
    assert!(extra.exists());
}

#[test]
fn clean_all_cache_removes_owned_cache_root() {
    let cache = tempdir().unwrap();
    let r = resolver(&cache);
    r.initialize_cache().unwrap();

    write_cache_leaf(
        &cache_leaf(
            &cache,
            "https://github.com/openwdl/tasks",
            "0000000000000000000000000000000000000001",
        ),
        "dep",
    );
    write_cache_leaf(
        &cache_leaf(
            &cache,
            "https://github.com/openwdl/other",
            "0000000000000000000000000000000000000002",
        ),
        "other",
    );

    let stats = r.clean_all_cache().unwrap();
    assert_eq!(stats.modules, 2);
    assert!(stats.bytes > 0);
    assert!(!cache.path().exists());
}
