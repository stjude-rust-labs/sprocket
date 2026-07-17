use std::fs;

use tempfile::tempdir;

use super::checksum;
use super::locked_git_entry;
use super::locked_git_resolver;
use super::module;
use super::resolver_with_lockfile;
use super::write_manifest;
use crate::Lockfile;
use crate::Manifest;
use crate::dependency::DependencyName;
use crate::dependency::GitSelector;
use crate::lockfile::DependencyEntry;
use crate::lockfile::GitCommit;
use crate::lockfile::ResolvedSource;
use crate::resolver::ResolverError;
use crate::resolver::cache::CacheKey;
use crate::resolver::scope::DependencyScope;
use crate::resolver::scope::ResolutionMode;

fn hash_from_byte(byte: u8) -> crate::hash::ContentHash {
    format!("sha256:{}", hex::encode([byte; 32]))
        .parse()
        .unwrap()
}

/// A forbidden lockfile Git URL must be rejected by the resolver policy
/// before `ensure_locked` performs any fetch.
#[tokio::test]
async fn ensure_locked_rejects_forbidden_git_url_before_fetch() {
    let cache = tempdir().unwrap();

    let forbidden_url: url::Url = "http://github.com/acme/widget".parse().unwrap();
    let sha: GitCommit = "0000000000000000000000000000000000000001".parse().unwrap();
    let entry = DependencyEntry {
        source: ResolvedSource::Git {
            git: forbidden_url.clone(),
            sha: sha.clone(),
            path: None,
            selector: GitSelector::Commit(
                "0000000000000000000000000000000000000001".parse().unwrap(),
            ),
        },
        checksum: Some(checksum()),
        signer: None,
        dependencies: Default::default(),
    };
    let r = locked_git_resolver(&cache, "widget", entry);

    let workdir = tempdir().unwrap();
    let consumer_dir = workdir.path().join("consumer");
    write_manifest(&consumer_dir, "consumer", "1.0.0", &[]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);
    assert!(
        consumer.lockfile_scope.is_empty(),
        "consumer must be a top-level `Module`"
    );

    let err = r.ensure_locked(&consumer).await.unwrap_err();
    assert!(
        matches!(err, ResolverError::GitUrlPolicyViolation { .. }),
        "expected `GitUrlPolicyViolation` before any fetch, got: {err}"
    );

    let leaf = CacheKey::from_git_url(&forbidden_url, &sha).absolute_path(r.cache_root());
    assert!(
        !leaf.exists(),
        "forbidden locked URL must be rejected before any fetch creates a cache leaf"
    );
}

#[tokio::test]
async fn locked_git_materialization_uses_scoped_lockfile_entry()
-> Result<(), Box<dyn std::error::Error>> {
    let cache = tempdir()?;
    let parent_dir = cache.path().join("parent");
    let parent: DependencyName = "parent".parse()?;
    let dep: DependencyName = "dep".parse()?;
    let selector = GitSelector::Commit("0000000000000000000000000000000000000001".parse()?);

    let mut parent_entry = DependencyEntry {
        source: ResolvedSource::Path { path: parent_dir },
        checksum: Some(checksum()),
        signer: None,
        dependencies: Default::default(),
    };
    parent_entry
        .dependencies
        .insert(dep.clone(), locked_git_entry(selector.clone()));

    let mut lockfile = Lockfile::default();
    lockfile.dependencies.insert(parent.clone(), parent_entry);
    let r = resolver_with_lockfile(&cache, lockfile);
    let url = "https://github.com/openwdl/tasks".parse()?;
    let plan = r
        .plan_git_materialization(
            &dep,
            &url,
            &selector,
            &None,
            DependencyScope::Transitive,
            ResolutionMode::Locked {
                lockfile_scope: &[parent],
            },
        )
        .await?;
    assert_eq!(
        plan.commit,
        "0000000000000000000000000000000000000001".parse()?
    );
    Ok(())
}

#[test]
fn verify_locked_verifies_matching_cache_leaf() {
    let workdir = tempdir().unwrap();
    let consumer_dir = workdir.path().join("consumer");
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let dep: DependencyName = "dep".parse().unwrap();
    let git = url::Url::parse("https://github.com/openwdl/tasks").unwrap();
    let commit: GitCommit = "0000000000000000000000000000000000000001".parse().unwrap();
    let leaf = CacheKey::from_git_url(&git, &commit).absolute_path(cache.path());
    fs::create_dir_all(&leaf).unwrap();
    write_manifest(&leaf, "dep", "1.0.0", &[]);
    fs::write(leaf.join("index.wdl"), b"workflow w {}").unwrap();
    let checksum = crate::hash::hash_directory(&leaf).unwrap();

    let mut lockfile = Lockfile::default();
    lockfile.dependencies.insert(
        dep,
        DependencyEntry {
            source: ResolvedSource::Git {
                git,
                sha: commit.clone(),
                selector: GitSelector::Commit(commit.as_str().parse().unwrap()),
                path: None,
            },
            checksum: Some(checksum),
            signer: None,
            dependencies: Default::default(),
        },
    );
    let r = resolver_with_lockfile(&cache, lockfile);
    assert_eq!(r.verify_locked(&consumer).unwrap(), 1);
}

#[test]
fn verify_locked_rejects_tampered_cache_leaf() {
    let workdir = tempdir().unwrap();
    let consumer_dir = workdir.path().join("consumer");
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let dep: DependencyName = "dep".parse().unwrap();
    let git = url::Url::parse("https://github.com/openwdl/tasks").unwrap();
    let commit: GitCommit = "0000000000000000000000000000000000000001".parse().unwrap();
    let leaf = CacheKey::from_git_url(&git, &commit).absolute_path(cache.path());
    fs::create_dir_all(&leaf).unwrap();
    write_manifest(&leaf, "dep", "1.0.0", &[]);
    fs::write(leaf.join("index.wdl"), b"workflow w {}").unwrap();
    let checksum = crate::hash::hash_directory(&leaf).unwrap();
    fs::write(leaf.join("index.wdl"), b"workflow tampered {}").unwrap();

    let mut lockfile = Lockfile::default();
    lockfile.dependencies.insert(
        dep,
        DependencyEntry {
            source: ResolvedSource::Git {
                git,
                sha: commit.clone(),
                selector: GitSelector::Commit(commit.as_str().parse().unwrap()),
                path: None,
            },
            checksum: Some(checksum),
            signer: None,
            dependencies: Default::default(),
        },
    );
    let r = resolver_with_lockfile(&cache, lockfile);
    let err = r.verify_locked(&consumer).unwrap_err();
    assert!(matches!(err, ResolverError::ChecksumMismatch { .. }));
}

#[test]
fn verify_locked_returns_not_fetched_when_cache_leaf_missing() {
    let workdir = tempdir().unwrap();
    let consumer_dir = workdir.path().join("consumer");
    write_manifest(&consumer_dir, "consumer", "0.1.0", &[]);
    let consumer =
        Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    let consumer = module(consumer, &consumer_dir);

    let cache = tempdir().unwrap();
    let dep: DependencyName = "dep".parse().unwrap();
    let git = url::Url::parse("https://github.com/openwdl/tasks").unwrap();
    let commit: GitCommit = "0000000000000000000000000000000000000001".parse().unwrap();
    let mut lockfile = Lockfile::default();
    lockfile.dependencies.insert(
        dep,
        DependencyEntry {
            source: ResolvedSource::Git {
                git,
                sha: commit.clone(),
                selector: GitSelector::Commit(commit.as_str().parse().unwrap()),
                path: None,
            },
            checksum: Some(hash_from_byte(1)),
            signer: None,
            dependencies: Default::default(),
        },
    );
    let r = resolver_with_lockfile(&cache, lockfile);
    let err = r.verify_locked(&consumer).unwrap_err();
    assert!(matches!(err, ResolverError::NotFetched { .. }));
}
