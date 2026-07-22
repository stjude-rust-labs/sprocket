//! Tests for the Git module resolver.

mod cache;
mod locked;
mod materialize;
mod resolve;

use std::fs;
use std::path::Path;
use std::sync::Arc;

use tempfile::TempDir;
use tempfile::tempdir;

use super::*;
use crate::Lockfile;
use crate::Manifest;
use crate::dependency::GitSelector;
use crate::lockfile::DependencyEntry;
use crate::lockfile::ResolvedSource;
use crate::module::Module;
use crate::resolver::Resolver;
use crate::resolver::policy::ResolverPolicy;
use crate::resolver::trust::TrustStore;

/// Builds a [`Module`] from a parsed [`Manifest`] and the directory it
/// lives in.
pub(super) fn module(manifest: Manifest, root: &Path) -> Module {
    Module::new(Arc::new(manifest), root.to_path_buf())
}

/// Returns a deterministic content hash for lockfile fixtures.
pub(super) fn checksum() -> crate::hash::ContentHash {
    // SAFETY: the literal is a valid SHA-256 content hash.
    "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        .parse()
        .unwrap()
}

/// Converts a path to a JSON-safe string (forward slashes on all
/// platforms).
pub(super) fn json_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// Builds a `module.json` at `dir` with the given name, version, and
/// optional `dependencies` map (each value is the JSON-encoded
/// dependency source).
pub(super) fn write_manifest(dir: &Path, name: &str, version: &str, deps: &[(&str, &str)]) {
    // SAFETY: test fixture directories are writable temporary directories.
    fs::create_dir_all(dir).unwrap();
    let deps_obj = if deps.is_empty() {
        String::new()
    } else {
        let entries: Vec<String> = deps.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect();
        format!(",\"dependencies\":{{{}}}", entries.join(","))
    };
    let body =
        format!("{{\"name\":\"{name}\",\"version\":\"{version}\",\"license\":\"MIT\"{deps_obj}}}");
    // SAFETY: the fixture directory exists and is writable.
    fs::write(dir.join(crate::MANIFEST_FILENAME), body).unwrap();
}

/// Builds a resolver with an empty lockfile and the supplied cache root.
pub(super) fn resolver(cache: &TempDir) -> GitResolver {
    resolver_with_lockfile(cache, Lockfile::default())
}

/// Builds a resolver with the supplied cache root and lockfile.
pub(super) fn resolver_with_lockfile(cache: &TempDir, lockfile: Lockfile) -> GitResolver {
    GitResolver::builder()
        .cache_root(cache.path())
        .trust(TrustStore::default())
        .lockfile(lockfile)
        .build()
}

/// Resolves a consumer's tree and builds a lockfile from it.
pub(super) async fn resolve_and_lock(
    cache: &TempDir,
    consumer: &Module,
) -> (GitResolver, Lockfile) {
    resolve_and_lock_with_config(
        cache,
        consumer,
        ResolverPolicy::default(),
        TrustStore::default(),
    )
    .await
}

/// Resolves and locks a fixture, then configures the locked resolver's policy
/// and trust.
pub(super) async fn resolve_and_lock_with_config(
    cache: &TempDir,
    consumer: &Module,
    policy: ResolverPolicy,
    trust: TrustStore,
) -> (GitResolver, Lockfile) {
    let r = resolver(cache);
    // SAFETY: callers construct resolvable test fixtures.
    let tree = r.resolve_tree(consumer).await.unwrap();
    // SAFETY: the resolved tree contains every declared fixture dependency.
    let outcome =
        crate::resolver::lock::partial_relock(&consumer.manifest, &Lockfile::default(), &tree)
            .unwrap();
    let locked = GitResolver::builder()
        .cache_root(cache.path())
        .trust(trust)
        .lockfile(outcome.lockfile.clone())
        .policy(policy)
        .build();
    (locked, outcome.lockfile)
}

/// Builds a locked Git fixture entry with the standard URL and commit.
pub(super) fn locked_git_entry(selector: GitSelector) -> DependencyEntry {
    locked_git_entry_with(
        "https://github.com/openwdl/tasks",
        "0000000000000000000000000000000000000001",
        selector,
    )
}

/// Builds a locked Git fixture entry with explicit source data.
pub(super) fn locked_git_entry_with(
    url: &str,
    sha: &str,
    selector: GitSelector,
) -> DependencyEntry {
    // SAFETY: fixture callers provide valid Git URLs.
    let git = url.parse().unwrap();
    // SAFETY: fixture callers provide full 40-character commit SHAs.
    let sha = sha.parse().unwrap();
    DependencyEntry {
        source: ResolvedSource::Git {
            git,
            sha,
            path: None,
            selector,
        },
        checksum: Some(checksum()),
        signer: None,
        dependencies: Default::default(),
    }
}

#[test]
fn git_resolver_facade_exposes_only_command_api() {
    /// Asserts that a facade type is safe to share between resolver tasks.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<GitResolver>();
    assert_send_sync::<GitResolverBuilder>();
    assert_send_sync::<CacheCleanStats>();
    assert_send_sync::<VerifyLockedReport>();
}

#[test]
fn builds_with_explicit_paths() {
    // SAFETY: the operating system can create a temporary test directory.
    let cache = tempdir().unwrap();
    let r = GitResolver::builder()
        .cache_root(cache.path())
        .trust(TrustStore::default())
        .lockfile(Lockfile::default())
        .build();
    assert_eq!(r.cache_root(), cache.path());
    assert!(r.trust_store().keys.is_empty());
}
