//! The default Git-backed [`Resolver`] implementation.
//!
//! This module owns the resolver's shared state (the [`GitResolver`]
//! struct and its `bon`-generated builder), the cache lifecycle
//! accessors, and the short [`Resolver`] trait delegation. The three
//! resolution phases live in child modules:
//!
//! - [`materialize`] — sparse-checkout planning and content-file resolution.
//! - [`resolve`] — fresh tree resolution and remote version discovery.
//! - [`locked`] — lockfile-driven traversal and non-fetching verification.
//!
//! [`Resolver`]: crate::resolver::Resolver

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use bon::Builder;
use semver::Version;

use crate::Lockfile;
use crate::dependency::DependencyName;
use crate::dependency::DependencySource;
use crate::module::Module;
use crate::resolver::Resolver;
use crate::resolver::error::ResolverError;
use crate::resolver::fetch::GitFetcher;
use crate::resolver::policy::ResolverPolicy;
use crate::resolver::scope::DependencyScope;
use crate::resolver::trust::TrustStore;
use crate::resolver::types::MaterializedFile;
use crate::resolver::types::ResolvedTree;
use crate::symbolic_path::SymbolicPath;

pub(in crate::resolver) mod materialize;
mod locked;
mod resolve;

/// The default Git-backed [`Resolver`].
///
/// Construct via [`GitResolver::builder`]. The caller is expected to
/// load the [`TrustStore`] from disk and pass it in; the library does
/// not derive default paths so the binary owns the policy of where
/// configuration lives.
///
/// [`Resolver`]: crate::resolver::Resolver
#[derive(Builder, Clone, Debug)]
pub struct GitResolver {
    /// Filesystem root under which `(host, org, repo, commit)` cache
    /// leaves are materialized.
    #[builder(into)]
    cache_root: PathBuf,
    /// The resolved policy, derived from [`ModulesConfig`] at construction.
    ///
    /// [`ModulesConfig`]: crate::resolver::ModulesConfig
    #[builder(default, into)]
    policy: Arc<ResolverPolicy>,
    /// The user-level trust store, loaded by the caller.
    trust: TrustStore,
    /// The lockfile to verify materialized dependencies against.
    ///
    /// `materialize` compares each dependency's observed content hash
    /// against the locked checksum and rejects mismatches.
    lockfile: Lockfile,
}

/// Summary of lockfile verification.
#[derive(Debug, Default)]
pub struct VerifyLockedReport {
    /// Count of dependencies that verified successfully.
    pub verified: usize,
    /// Verified dependencies that had no cryptographic module signature.
    pub unsigned: Vec<DependencyName>,
    /// Per-dependency verification failures.
    pub errors: Vec<(DependencyName, ResolverError)>,
}

/// Summary of a WDL module cache cleanup.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheCleanStats {
    /// Number of materialized module commits removed.
    pub modules: usize,
    /// Number of cached bytes removed.
    pub bytes: u64,
}

impl GitResolver {
    /// Initializes an empty cache root or validates its ownership marker.
    pub fn initialize_cache(&self) -> Result<(), ResolverError> {
        crate::resolver::git::initialize_cache_root(&self.cache_root)?;
        Ok(())
    }

    /// Removes every materialized module from the owned cache root.
    pub fn clean_all_cache(&self) -> Result<CacheCleanStats, ResolverError> {
        let (modules, bytes) = crate::resolver::git::remove_cache_root(&self.cache_root)?;
        Ok(CacheCleanStats { modules, bytes })
    }

    /// Removes cache leaves reachable from `consumer`'s locked dependency tree.
    pub fn clean_locked_cache(&self, consumer: &Module) -> Result<CacheCleanStats, ResolverError> {
        let leaves = self.locked_cache_leaves(consumer)?;
        let (modules, bytes) =
            crate::resolver::git::remove_cache_leaves(&self.cache_root, &leaves)?;
        Ok(CacheCleanStats { modules, bytes })
    }

    /// Returns the cache root.
    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    /// Returns the active trust store.
    pub fn trust_store(&self) -> &TrustStore {
        &self.trust
    }

    /// Returns a policy-configured Git fetcher.
    pub(in crate::resolver) fn fetcher(&self) -> GitFetcher {
        GitFetcher::new(self.policy.clone())
    }

    /// Returns the lockfile.
    pub fn lockfile(&self) -> &Lockfile {
        &self.lockfile
    }
}

#[async_trait]
impl Resolver for GitResolver {
    async fn materialize(
        &self,
        consumer: &Module,
        path: &SymbolicPath,
    ) -> Result<MaterializedFile, ResolverError> {
        self.materialize_file(consumer, path).await
    }

    async fn resolve_tree(&self, consumer: &Module) -> Result<ResolvedTree, ResolverError> {
        self.resolve_fresh_tree(consumer).await
    }

    async fn discover_versions(
        &self,
        name: &DependencyName,
        source: &DependencySource,
        scope: DependencyScope,
    ) -> Result<Vec<Version>, ResolverError> {
        self.discover_matching_versions(name, source, scope).await
    }
}
