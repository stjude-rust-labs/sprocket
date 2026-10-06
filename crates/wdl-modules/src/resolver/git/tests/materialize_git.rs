//! Tests for materializing Git dependencies through the module cache,
//! including restoring damaged cache content.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use tempfile::TempDir;
use tempfile::tempdir;

use super::module;
use super::write_manifest;
use crate::Lockfile;
use crate::Manifest;
use crate::dependency::DependencyName;
use crate::module::Module;
use crate::resolver::GitResolver;
use crate::resolver::ModulesConfig;
use crate::resolver::Resolver;
use crate::resolver::ResolverError;
use crate::resolver::TrustStore;
use crate::resolver::policy::ResolverPolicy;

/// Commits a `dep` module to a new upstream repository and returns the
/// repository with its commit SHA.
fn upstream() -> (TempDir, String) {
    upstream_at("dep")
}

/// Commits a `dep` module at `path` (`.` for the repository root) to a new
/// upstream repository and returns the repository with its commit SHA.
fn upstream_at(path: &str) -> (TempDir, String) {
    let upstream = tempdir().unwrap();
    let repo = git2::Repository::init(upstream.path()).unwrap();
    let dep = upstream.path().join(path);
    write_manifest(&dep, "dep", "1.0.0", &[]);
    fs::write(dep.join("index.wdl"), b"version 1.3\n").unwrap();
    fs::write(dep.join("a.wdl"), b"version 1.3\ntask a {}\n").unwrap();
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = git2::Signature::now("test", "test@example.com").unwrap();
    let oid = repo
        .commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
        .unwrap();
    (upstream, oid.to_string())
}

/// Builds a consumer module depending on `dep` from `upstream`.
fn consumer(workdir: &Path, upstream: &Path, sha: &str) -> Module {
    consumer_with_source(
        workdir,
        serde_json::json!({
            "git": url::Url::from_file_path(upstream).unwrap().as_str(),
            "commit": sha,
            "path": "dep",
        }),
    )
}

/// Builds a consumer module depending on `dep` from the given source.
fn consumer_with_source(workdir: &Path, source: serde_json::Value) -> Module {
    let dir = workdir.join("consumer");
    write_manifest(&dir, "consumer", "0.1.0", &[("dep", &source.to_string())]);
    let manifest = Manifest::parse(&fs::read(dir.join(crate::MANIFEST_FILENAME)).unwrap()).unwrap();
    module(manifest, &dir)
}

/// Builds a resolver that allows `file://` Git sources.
fn resolver(cache: &TempDir, lockfile: Lockfile) -> GitResolver {
    GitResolver::builder()
        .cache_root(cache.path())
        .trust(TrustStore::default())
        .lockfile(lockfile)
        .policy(
            ResolverPolicy::try_from(&ModulesConfig {
                allowed_schemes: vec!["https".into(), "file".into()],
                ..ModulesConfig::default()
            })
            .unwrap(),
        )
        .build()
}

/// Resolves `consumer` fresh and returns the resulting lockfile.
async fn lock(cache: &TempDir, consumer: &Module) -> Lockfile {
    let tree = resolver(cache, Lockfile::default())
        .resolve_tree(consumer)
        .await
        .unwrap();
    crate::resolver::lock::partial_relock(&consumer.manifest, &Lockfile::default(), &tree)
        .unwrap()
        .lockfile
}

/// Returns the locked checksum of `dep`.
fn locked_checksum(lockfile: &Lockfile) -> crate::hash::ContentHash {
    let dep: DependencyName = "dep".parse().unwrap();
    lockfile.dependencies[&dep].checksum.unwrap()
}

/// Materializes `dep/a` and returns the resolved file path.
async fn materialize(r: &GitResolver, consumer: &Module) -> Result<PathBuf, ResolverError> {
    r.materialize(consumer, &"dep/a".parse().unwrap())
        .await
        .map(|file| file.path)
}

#[tokio::test]
async fn materialize_restores_damaged_cache_content() {
    let (upstream, sha) = upstream();
    let workdir = tempdir().unwrap();
    let consumer = consumer(workdir.path(), upstream.path(), &sha);
    let cache = tempdir().unwrap();
    let r = resolver(&cache, lock(&cache, &consumer).await);

    let path = materialize(&r, &consumer).await.unwrap();
    let module_root = path.parent().unwrap().to_path_buf();
    fs::write(&path, b"version 1.3\ntask x {}\n").unwrap();
    fs::write(module_root.join("extra.wdl"), b"extra").unwrap();

    let restored = materialize(&r, &consumer).await.unwrap();
    assert_eq!(restored, path);
    assert_eq!(fs::read(&path).unwrap(), b"version 1.3\ntask a {}\n");
    assert!(!module_root.join("extra.wdl").exists());
}

#[tokio::test]
async fn materialize_leaves_clean_cache_alone_when_lockfile_mismatches() {
    let (upstream, sha) = upstream();
    let workdir = tempdir().unwrap();
    let consumer = consumer(workdir.path(), upstream.path(), &sha);
    let cache = tempdir().unwrap();
    let mut lockfile = lock(&cache, &consumer).await;

    let path = materialize(&resolver(&cache, lockfile.clone()), &consumer)
        .await
        .unwrap();
    let before = fs::read(&path).unwrap();
    #[cfg(unix)]
    let inode = {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(&path).unwrap().ino()
    };

    let dep: DependencyName = "dep".parse().unwrap();
    lockfile.dependencies.get_mut(&dep).unwrap().checksum =
        Some(crate::hash::ContentHash::from([0xAB; 32]));
    let error = materialize(&resolver(&cache, lockfile), &consumer)
        .await
        .unwrap_err();
    assert!(
        matches!(error, ResolverError::ChecksumMismatch { .. }),
        "got: {error}"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
    }
}

#[tokio::test]
async fn fresh_resolution_does_not_lock_damaged_cache_content() {
    let (upstream, sha) = upstream();
    let workdir = tempdir().unwrap();
    let consumer = consumer(workdir.path(), upstream.path(), &sha);
    let cache = tempdir().unwrap();
    let lockfile = lock(&cache, &consumer).await;
    let expected = locked_checksum(&lockfile);

    let path = materialize(&resolver(&cache, lockfile), &consumer)
        .await
        .unwrap();
    fs::write(&path, b"version 1.3\ntask x {}\n").unwrap();

    let relocked = lock(&cache, &consumer).await;
    assert_eq!(locked_checksum(&relocked), expected);
    assert_eq!(fs::read(&path).unwrap(), b"version 1.3\ntask a {}\n");
}

#[tokio::test]
async fn materialize_root_module_dependency() {
    let (upstream, sha) = upstream_at(".");
    let workdir = tempdir().unwrap();
    let consumer = consumer_with_source(
        workdir.path(),
        serde_json::json!({
            "git": url::Url::from_file_path(upstream.path()).unwrap().as_str(),
            "commit": sha,
        }),
    );
    let cache = tempdir().unwrap();
    let r = resolver(&cache, lock(&cache, &consumer).await);

    let path = materialize(&r, &consumer).await.unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"version 1.3\ntask a {}\n");
    assert!(
        path.parent()
            .unwrap()
            .join(crate::MANIFEST_FILENAME)
            .is_file()
    );
}
