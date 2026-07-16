//! Public resolver API.
//!
//! The trait definitions (`Resolver`, `ResolverError`, `MaterializedFile`,
//! etc.) are always available. The Git-backed implementation and supporting
//! infrastructure (cache, config, fetch, lock, trust, etc.) are gated
//! behind the `resolver` cargo feature so that consumers like `wdl-doc`
//! that only need the manifest/lockfile/hashing types do not pay for
//! `git2` and friends.

#[cfg(feature = "git-resolver")]
pub(crate) mod cache;
#[cfg(feature = "git-resolver")]
pub(crate) mod config;
pub(crate) mod error;
#[cfg(feature = "git-resolver")]
pub(crate) mod fetch;
#[cfg(feature = "git-resolver")]
mod git;
#[cfg(feature = "git-resolver")]
pub(crate) mod lock;
#[cfg(feature = "git-resolver")]
pub(crate) mod policy;
pub(crate) mod scope;
#[cfg(feature = "git-resolver")]
pub(crate) mod trust;
pub(crate) mod types;
#[cfg(feature = "git-resolver")]
pub(crate) mod verify;
#[cfg(feature = "git-resolver")]
pub(crate) mod versions;

#[cfg(feature = "git-resolver")]
use std::collections::BTreeMap;
#[cfg(feature = "git-resolver")]
use std::path::Path;
#[cfg(feature = "git-resolver")]
use std::path::PathBuf;
#[cfg(feature = "git-resolver")]
use std::sync::Arc;

use async_trait::async_trait;
#[cfg(feature = "git-resolver")]
use bon::Builder;
#[cfg(feature = "git-resolver")]
use futures::future::BoxFuture;
#[cfg(feature = "git-resolver")]
use futures::future::FutureExt;
#[cfg(feature = "git-resolver")]
use path_clean::PathClean;
use semver::Version;

#[cfg(feature = "git-resolver")]
use crate::Lockfile;
#[cfg(feature = "git-resolver")]
use crate::Manifest;
use crate::dependency::DependencyName;
use crate::dependency::DependencySource;
#[cfg(feature = "git-resolver")]
use crate::dependency::GitModulePath;
#[cfg(feature = "git-resolver")]
use crate::dependency::GitSelector;
#[cfg(feature = "git-resolver")]
use crate::hash::NON_MODULE_CONTENT;
#[cfg(feature = "git-resolver")]
use crate::lockfile::DependencyEntry;
#[cfg(feature = "git-resolver")]
use crate::lockfile::DependencyMap;
#[cfg(feature = "git-resolver")]
use crate::lockfile::GitCommit;
#[cfg(feature = "git-resolver")]
use crate::lockfile::ResolvedSource;
use crate::module::Module;
#[cfg(feature = "git-resolver")]
#[cfg(feature = "git-resolver")]
use crate::relative_path::RelativePath;
#[cfg(feature = "git-resolver")]
use crate::resolver::cache::CacheKey;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::config::GitPlatform;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::config::LargeFileWarning;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::config::LargeFileWarningError;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::config::ModulesConfig;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::config::TrustMode;
pub use crate::resolver::error::GitRefKind;
pub use crate::resolver::error::MissingFileKind;
pub use crate::resolver::error::ResolverError;
#[cfg(feature = "git-resolver")]
use crate::resolver::fetch::GitFetcher;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::lock::ChangedSigner;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::lock::DependencyChange;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::lock::DependencyUpdate;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::lock::LockfileDiff;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::lock::NewSigner;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::lock::RelockOutcome;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::lock::RelockStats;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::lock::RemovedSigner;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::lock::SignerIdentityMap;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::lock::partial_relock;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::lock::signer_identity_map;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::lock::update_relock;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::policy::ResolverPolicy;
pub use crate::resolver::scope::DependencyScope;
#[cfg(feature = "git-resolver")]
use crate::resolver::scope::ResolutionMode;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::trust::TrustStore;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::trust::TrustStoreError;
#[cfg(feature = "git-resolver")]
pub use crate::resolver::trust::TrustedIdentity;
pub use crate::resolver::types::MaterializedFile;
pub use crate::resolver::types::ResolvedDependency;
pub use crate::resolver::types::ResolvedModule;
pub use crate::resolver::types::ResolvedTree;
#[cfg(feature = "git-resolver")]
use crate::resolver::verify::VerifiedModule;
use crate::symbolic_path::SymbolicPath;

/// Resolves WDL module imports to concrete files on disk.
#[async_trait]
pub trait Resolver: std::fmt::Debug + Send + Sync {
    /// Materializes a single symbolic import on disk and returns the path
    /// to the resulting file.
    ///
    /// The primary call site for `wdl-analysis`. When the analyzer
    /// encounters a symbolic import like `import openwdl/csvkit/cut`, it
    /// asks the resolver for the file path that statement should route
    /// to, then parses the result with the existing import machinery as
    /// if the user had written `import "<that path>"`.
    ///
    /// `consumer` is the importing module: its manifest declares the
    /// symbolic path's head component, and its root rebases any
    /// relative `LocalPath` dependencies. `path` is the parsed
    /// symbolic path. The resolver looks up the head component in
    /// `consumer.manifest.dependencies`, materializes the dep's module
    /// folder if not yet cached, and resolves either the manifest's
    /// `entrypoint` (when the symbolic path has no sub-path) or
    /// `<sub-path>.wdl` under the module folder.
    async fn materialize(
        &self,
        consumer: &Module,
        path: &SymbolicPath,
    ) -> Result<MaterializedFile, ResolverError>;

    /// Resolves every transitive dependency declared by `consumer`.
    ///
    /// Walks `consumer.manifest.dependencies`, recurses into each dep's
    /// own manifest, and records every module visited along the way.
    /// Relative `LocalPath` entries are resolved against the declaring
    /// module's root. Detects cycles.
    async fn resolve_tree(&self, consumer: &Module) -> Result<ResolvedTree, ResolverError>;

    /// Lists discovered versions for a dependency source that satisfy
    /// the requirement, in descending semver order.
    ///
    /// Used by CLI commands that surface available versions to the user
    /// and internally by `resolve_tree` to select the version a Git dep
    /// resolves to.
    async fn discover_versions(
        &self,
        name: &DependencyName,
        source: &DependencySource,
        scope: DependencyScope,
    ) -> Result<Vec<Version>, ResolverError>;
}

#[cfg(feature = "git-resolver")]
/// The default Git-backed [`Resolver`].
///
/// Construct via [`GitResolver::builder`]. The caller is expected to
/// load the [`TrustStore`] from disk and pass it in; the library does
/// not derive default paths so the binary owns the policy of where
/// configuration lives.
#[derive(Builder, Clone, Debug)]
pub struct GitResolver {
    /// Filesystem root under which `(host, org, repo, commit)` cache
    /// leaves are materialized.
    #[builder(into)]
    cache_root: PathBuf,
    /// The resolved policy, derived from [`ModulesConfig`] at construction.
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

#[cfg(feature = "git-resolver")]
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

#[cfg(feature = "git-resolver")]
/// Summary of a WDL module cache cleanup.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheCleanStats {
    /// Number of materialized module commits removed.
    pub modules: usize,
    /// Number of cached bytes removed.
    pub bytes: u64,
}

#[cfg(feature = "git-resolver")]
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

    /// Returns a policy-enforcing Git fetcher.
    fn fetcher(&self) -> GitFetcher {
        GitFetcher::new(self.policy.clone())
    }

    /// Discovers the default branch advertised by a Git remote.
    pub async fn discover_default_branch(
        &self,
        name: &DependencyName,
        url: &url::Url,
        scope: DependencyScope,
    ) -> Result<String, ResolverError> {
        self.policy.check_git_url(name, url, scope)?;
        let fetcher = self.fetcher();
        let dep = name.clone();
        let url = url.clone();
        tokio::task::spawn_blocking(move || fetcher.default_branch(&dep, &url, scope))
            .await
            // The closure performs pure libgit2 work and does
            // not panic; `JoinError` only occurs on runtime shutdown.
            .unwrap()
    }

    /// Returns the lockfile.
    pub fn lockfile(&self) -> &Lockfile {
        &self.lockfile
    }

    /// Returns true dependency map at `scope` from the nested lockfile tree.
    fn lockfile_dependencies_at_scope(
        &self,
        scope: &[DependencyName],
    ) -> Result<&DependencyMap, ResolverError> {
        let mut current = &self.lockfile.dependencies;
        for parent in scope {
            current = &current
                .get(parent)
                .ok_or_else(|| ResolverError::NotInLockfile {
                    dep: parent.manifest().to_string(),
                })?
                .dependencies;
        }
        Ok(current)
    }

    /// Flattens nested lockfile dependencies into `(scope, name, source)`
    /// tuples.
    fn collect_locked_entries(
        scope: &[DependencyName],
        deps: &DependencyMap,
        out: &mut Vec<(Vec<DependencyName>, DependencyName, ResolvedSource)>,
    ) {
        for (name, entry) in deps {
            out.push((scope.to_vec(), name.clone(), entry.source.clone()));
            let mut child_scope = scope.to_vec();
            child_scope.push(name.clone());
            Self::collect_locked_entries(&child_scope, &entry.dependencies, out);
        }
    }

    /// Materializes every locked Git dependency reachable from `consumer`
    /// and returns the number of newly fetched cache leaves.
    pub async fn ensure_locked(&self, consumer: &Module) -> Result<usize, ResolverError> {
        let deps = self.lockfile_dependencies_at_scope(&consumer.lockfile_scope)?;
        let mut locked_entries = Vec::new();
        Self::collect_locked_entries(&consumer.lockfile_scope, deps, &mut locked_entries);

        let mut fetched = 0usize;
        for (scope, name, source) in locked_entries {
            let ResolvedSource::Git {
                git,
                selector,
                path,
                ..
            } = source
            else {
                continue;
            };

            let dep_scope = if consumer.lockfile_scope.is_empty() && scope.is_empty() {
                DependencyScope::TopLevel
            } else {
                DependencyScope::Transitive
            };
            let plan = self
                .plan_git_materialization(
                    &name,
                    &git,
                    &selector,
                    &path,
                    dep_scope,
                    ResolutionMode::Locked {
                        lockfile_scope: &scope,
                    },
                )
                .await?;
            let root = self.materialize_git(&name, &git, dep_scope, &plan).await?;
            if matches!(root, MaterializedRoot::Cached { fetched: true, .. }) {
                fetched += 1;
            }
        }
        Ok(fetched)
    }

    /// Returns cache leaves for every locked Git dependency reachable
    /// from `consumer`.
    pub fn locked_cache_leaves(&self, consumer: &Module) -> Result<Vec<PathBuf>, ResolverError> {
        let deps = self.lockfile_dependencies_at_scope(&consumer.lockfile_scope)?;
        let mut locked_entries = Vec::new();
        Self::collect_locked_entries(&consumer.lockfile_scope, deps, &mut locked_entries);

        let mut leaves = Vec::new();
        for (_, _, source) in locked_entries {
            let ResolvedSource::Git { git, sha, .. } = source else {
                continue;
            };
            leaves.push(CacheKey::from_git_url(&git, &sha).absolute_path(&self.cache_root));
        }
        leaves.sort();
        leaves.dedup();
        Ok(leaves)
    }

    /// Verifies every locked dependency reachable from `consumer` without
    /// fetching.
    pub fn verify_locked(&self, consumer: &Module) -> Result<usize, ResolverError> {
        let report = self.verify_locked_report(consumer)?;
        if let Some((_, err)) = report.errors.into_iter().next() {
            return Err(err);
        }
        Ok(report.verified)
    }

    /// Verifies every locked dependency reachable from `consumer` without
    /// fetching and returns all failures.
    pub fn verify_locked_report(
        &self,
        consumer: &Module,
    ) -> Result<VerifyLockedReport, ResolverError> {
        let deps = self.lockfile_dependencies_at_scope(&consumer.lockfile_scope)?;
        let mut locked_entries = Vec::new();
        Self::collect_locked_entries(&consumer.lockfile_scope, deps, &mut locked_entries);

        let mut report = VerifyLockedReport::default();
        for (scope, name, source) in locked_entries {
            // Local path sources carry no checksum and are read as-is;
            // there is nothing to verify against the lockfile.
            let (git, sha, sub_path) = match &source {
                ResolvedSource::Git { git, sha, path, .. } => (git, sha, path),
                ResolvedSource::Path { .. } => continue,
            };

            let leaf = CacheKey::from_git_url(git, sha).absolute_path(self.cache_root());
            tracing::trace!(
                dependency = name.manifest(),
                cache_leaf = %leaf.display(),
                commit = %sha,
                "checking module cache leaf"
            );
            if !leaf.exists() {
                tracing::debug!(
                    dependency = name.manifest(),
                    cache_leaf = %leaf.display(),
                    "module cache leaf is missing"
                );
                let dep = name.manifest().to_string();
                report
                    .errors
                    .push((name, ResolverError::NotFetched { dep }));
                continue;
            }
            tracing::debug!(
                dependency = name.manifest(),
                cache_leaf = %leaf.display(),
                "module cache leaf is present"
            );
            let module_root = match sub_path {
                Some(sub_path) => leaf.join(sub_path.as_path()),
                None => leaf,
            };

            let source_url = source.source_url();
            let source_path = source.source_path();
            let verified = match crate::resolver::verify::verify(
                &self.policy,
                &self.trust,
                &name,
                &module_root,
                Some((&source_url, source_path)),
            ) {
                Ok(verified) => verified,
                Err(err) => {
                    report.errors.push((name, err));
                    continue;
                }
            };
            if let Err(err) = crate::resolver::verify::verify_against_lockfile(
                &self.lockfile,
                &self.trust,
                &scope,
                &name,
                &verified.checksum,
                verified.signer.as_ref().map(|signer| &signer.key),
                verified
                    .signer
                    .as_ref()
                    .and_then(|signer| signer.identity.as_ref()),
            ) {
                report.errors.push((name, err));
                continue;
            }
            if verified.signer.is_none() {
                report.unsigned.push(name.clone());
            }
            report.verified += 1;
        }
        Ok(report)
    }

    /// Checks that a locked local-path dep matches the manifest declaration.
    fn validate_locked_local(
        &self,
        consumer: &Module,
        name: &DependencyName,
        path: &Path,
    ) -> Result<(), ResolverError> {
        let locked_entry = self
            .lockfile
            .find_scoped(&consumer.lockfile_scope, name)
            .ok_or_else(|| ResolverError::NotInLockfile {
                dep: name.manifest().to_string(),
            })?;
        if let ResolvedSource::Path { path: locked_path } = &locked_entry.source {
            // The lockfile may store either an absolute path or one
            // written relative to the declaring `module.json`. Rebase
            // both sides through the consumer so the comparison is
            // independent of how the path was originally written.
            let locked_resolved = consumer.resolve_local_path(locked_path);
            if path != locked_resolved {
                return Err(ResolverError::LockfileSourceMismatch {
                    dep: name.manifest().to_string(),
                });
            }
        } else {
            return Err(ResolverError::LockfileSourceMismatch {
                dep: name.manifest().to_string(),
            });
        }
        Ok(())
    }

    /// Runs the sparse checkout for a Git dependency and returns its root.
    ///
    /// On failure, cleans up the cache leaf so a corrupt partial
    /// checkout does not persist.
    async fn materialize_git(
        &self,
        name: &DependencyName,
        url: &url::Url,
        scope: DependencyScope,
        plan: &GitMaterializationPlan,
    ) -> Result<MaterializedRoot, ResolverError> {
        let fetcher = self.fetcher();
        let dep_for_clone = name.clone();
        let url_for_clone = url.clone();
        let leaf_for_clone = plan.leaf.clone();
        let cache_root = self.cache_root.clone();
        let commit_for_clone = plan.commit.clone();
        let sparse_path = plan.sparse_path.clone();
        tracing::debug!(
            dependency = name.manifest(),
            cache_leaf = %plan.leaf.display(),
            module_root = %plan.module_path.display(),
            commit = %plan.commit,
            sparse_path = %plan.sparse_path,
            "materializing Git dependency from module cache"
        );
        let result = tokio::task::spawn_blocking(move || {
            fetcher.ensure_materialized(
                &dep_for_clone,
                &url_for_clone,
                commit_for_clone.as_str(),
                &[sparse_path.as_str()],
                scope,
                crate::resolver::git::CacheLocation {
                    root: &cache_root,
                    leaf: &leaf_for_clone,
                },
            )
        })
        .await
        // The closure performs only libgit2 work and
        // does not panic; a `JoinError` would only fire on
        // runtime shutdown.
        .unwrap();

        let fetched = match result {
            Ok(fetched) => fetched,
            Err(err) => {
                if plan.leaf.starts_with(&self.cache_root)
                    && plan.leaf.exists()
                    && let Err(io_err) = std::fs::remove_dir_all(&plan.leaf)
                {
                    tracing::warn!(
                        path = %plan.leaf.display(),
                        error = %io_err,
                        "failed to clean up cache leaf after materialization failure",
                    );
                }
                return Err(err);
            }
        };

        tracing::trace!(
            dependency = name.manifest(),
            cache_leaf = %plan.leaf.display(),
            module_root = %plan.module_path.display(),
            "materialized Git dependency from module cache"
        );
        Ok(MaterializedRoot::Cached {
            fetched,
            module_root: plan.module_path.clone(),
        })
    }

    /// Recursively resolves a dependency map for `resolve_tree`.
    ///
    /// Each iteration: policy check, materialize, read manifest, cycle
    /// check, verify, recurse into transitive deps, assemble result.
    fn resolve_dependencies<'a>(
        &'a self,
        deps: &'a BTreeMap<DependencyName, DependencySource>,
        parent_root: &'a Path,
        parent: Option<&'a ResolvedSource>,
        chain: &'a mut Vec<(DependencyName, ResolvedSource)>,
    ) -> BoxFuture<'a, Result<BTreeMap<DependencyName, ResolvedDependency>, ResolverError>> {
        async move {
            let mut out = BTreeMap::new();
            let scope = if parent.is_some() {
                DependencyScope::Transitive
            } else {
                DependencyScope::TopLevel
            };

            for (name, source) in deps {
                // Local-path deps under a Git parent are disallowed:
                // the path would be meaningless outside the original
                // machine, making the resolution non-reproducible.
                if matches!(source, DependencySource::LocalPath { .. })
                    && matches!(parent, Some(ResolvedSource::Git { .. }))
                {
                    return Err(ResolverError::LocalPathInTransitive {
                        dep: name.manifest().to_string(),
                    });
                }

                // Enforce URL scheme and host policy.
                if let DependencySource::Git { url, .. } = source {
                    self.policy.check_git_url(name, url, scope)?;
                }

                // Materialize the dependency on disk and read its manifest.
                let (resolved_source, manifest, module_root, selected_version) = match source {
                    DependencySource::LocalPath { path, .. } => {
                        let resolved_path = if path.is_absolute() {
                            path.clean()
                        } else {
                            parent_root.join(path).clean()
                        };
                        let manifest = read_manifest(&resolved_path)?;
                        let resolved = ResolvedSource::Path {
                            path: resolved_path.clone(),
                        };
                        let root = MaterializedRoot::Local(resolved_path);
                        (resolved, manifest, root, None)
                    }
                    DependencySource::Git {
                        url,
                        selector,
                        path,
                        ..
                    } => {
                        let plan = self
                            .plan_git_materialization(
                                name,
                                url,
                                selector,
                                path,
                                scope,
                                ResolutionMode::Fresh,
                            )
                            .await?;
                        let root = self.materialize_git(name, url, scope, &plan).await?;
                        let manifest = read_manifest(&plan.module_path)?;
                        let selected_version = plan.selected_version.clone();
                        let resolved = ResolvedSource::Git {
                            git: url.clone(),
                            sha: plan.commit,
                            path: path.clone(),
                            selector: selector.clone(),
                        };
                        (resolved, manifest, root, selected_version)
                    }
                };

                // Detect cycles before recursing. Identity is the source's
                // coordinates (repository URL and sub-path, or local
                // directory), so a module that transitively depends on
                // itself is caught even at a different version or selector.
                if let Some(at) = chain
                    .iter()
                    .position(|(_, s)| s.coordinates() == resolved_source.coordinates())
                {
                    let mut path: Vec<String> = chain[at..]
                        .iter()
                        .map(|(n, _)| n.manifest().to_string())
                        .collect();
                    path.push(name.manifest().to_string());
                    return Err(ResolverError::Cycle { path });
                }

                // Verify content hash, signature, and trust pin. Local
                // path sources carry no checksum or signature and are
                // read as-is, so only structural validation runs for them.
                let (checksum, signer, signer_identity) = match &resolved_source {
                    ResolvedSource::Path { .. } => {
                        crate::resolver::verify::verify_structure(
                            &self.policy,
                            name,
                            module_root.module_root(),
                        )?;
                        (None, None, None)
                    }
                    ResolvedSource::Git { .. } => {
                        let source_url = resolved_source.source_url();
                        let source_path = resolved_source.source_path();
                        let VerifiedModule { checksum, signer } = crate::resolver::verify::verify(
                            &self.policy,
                            &self.trust,
                            name,
                            module_root.module_root(),
                            Some((&source_url, source_path)),
                        )?;
                        let signer_key = signer.as_ref().map(|signer| signer.key);
                        let signer_identity = signer.and_then(|signer| signer.identity);
                        (Some(checksum), signer_key, signer_identity)
                    }
                };

                // Recurse into transitive dependencies. Pass this dep's
                // module root so that relative `LocalPath` entries in its
                // own manifest resolve against the right directory.
                let child_root = module_root.module_root();
                chain.push((name.clone(), resolved_source.clone()));
                let inner = self
                    .resolve_dependencies(
                        &manifest.dependencies,
                        child_root,
                        Some(&resolved_source),
                        chain,
                    )
                    .await
                    .inspect_err(|_| {
                        chain.pop();
                    })?;
                chain.pop();

                out.insert(
                    name.clone(),
                    ResolvedDependency {
                        source: resolved_source,
                        version: selected_version,
                        checksum,
                        signer,
                        signer_identity,
                        dependencies: inner,
                    },
                );
            }
            Ok(out)
        }
        .boxed()
    }

    /// Resolves a [`GitSelector`] to a concrete commit SHA.
    ///
    /// Queries the remote at `url` for tags or branches (depending on
    /// the selector variant), then maps the result to a commit. For
    /// version selectors, also returns the matched semver version so
    /// callers can record it in the resolved tree.
    async fn resolve_git_selector(
        &self,
        name: &DependencyName,
        url: &url::Url,
        selector: &GitSelector,
        path_prefix: Option<&str>,
        scope: DependencyScope,
    ) -> Result<(Option<Version>, GitCommit), ResolverError> {
        let fetcher = self.fetcher();
        match selector {
            GitSelector::Version(requirement) => {
                let dep = name.clone();
                let url = url.clone();
                let requirement = requirement.clone();
                let path_prefix_owned = path_prefix.map(str::to_string);
                let refs =
                    tokio::task::spawn_blocking(move || fetcher.list_tags(&dep, &url, scope))
                        .await
                        // The closure performs only Git work; a
                        // `JoinError` would only fire on runtime shutdown.
                        .unwrap()?;
                let (version, commit) = crate::resolver::versions::resolve_version_to_commit(
                    &refs,
                    path_prefix_owned.as_deref(),
                    &requirement,
                )
                .map_err(|e| match e {
                    crate::resolver::versions::VersionError::NoSatisfyingVersion {
                        requirement,
                        considered,
                    } => ResolverError::NoSatisfyingVersion {
                        dep: name.manifest().to_string(),
                        requirement,
                        considered,
                        path: path_prefix_owned,
                    },
                })?;
                Ok((Some(version), commit))
            }
            GitSelector::Tag(tag) => {
                let dep = name.clone();
                let url = url.clone();
                let fetcher = self.fetcher();
                let refs =
                    tokio::task::spawn_blocking(move || fetcher.list_tags(&dep, &url, scope))
                        .await
                        // The closure does not panic.
                        .unwrap()?;
                let commit =
                    refs.get(tag)
                        .cloned()
                        .ok_or_else(|| ResolverError::UnknownGitRef {
                            dep: name.manifest().to_string(),
                            kind: GitRefKind::Tag,
                            name: tag.clone(),
                        })?;
                Ok((None, commit))
            }
            GitSelector::Branch(branch) => {
                let dep = name.clone();
                let url = url.clone();
                let fetcher = self.fetcher();
                let refs =
                    tokio::task::spawn_blocking(move || fetcher.list_branches(&dep, &url, scope))
                        .await
                        // The closure does not panic.
                        .unwrap()?;
                let commit =
                    refs.get(branch)
                        .cloned()
                        .ok_or_else(|| ResolverError::UnknownGitRef {
                            dep: name.manifest().to_string(),
                            kind: GitRefKind::Branch,
                            name: branch.clone(),
                        })?;
                Ok((None, commit))
            }
            GitSelector::Commit(commit) => {
                // A full SHA is used as-is. A prefix is expanded to the
                // full SHA by cloning the repository into a temporary
                // directory under the cache root and running rev-parse.
                if commit.is_full() {
                    let full = GitCommit::try_from(commit.as_str().to_string())
                        // `is_full` guarantees exactly 40 lowercase
                        // hex characters, which `GitCommit` accepts.
                        .expect("a full commit-ish is a valid commit SHA");
                    return Ok((None, full));
                }
                let dep = name.clone();
                let url = url.clone();
                let prefix = commit.as_str().to_string();
                let work_dir = self.commit_expand_dir(&url, &prefix);
                let _ = std::fs::remove_dir_all(&work_dir);
                let fetcher = self.fetcher();
                let expand_dir = work_dir.clone();
                let full = tokio::task::spawn_blocking(move || {
                    fetcher.resolve_commit_prefix(&dep, &url, &prefix, scope, &expand_dir)
                })
                .await
                // The closure performs only Git work and does not panic.
                .unwrap();
                let _ = std::fs::remove_dir_all(&work_dir);
                let full = full?;
                // A resolved Git OID is always 40 lowercase hex characters.
                let commit =
                    GitCommit::try_from(full).expect("a resolved Git OID is a valid commit SHA");
                Ok((None, commit))
            }
        }
    }

    /// Returns a unique temporary directory under the cache root used to
    /// clone a repository while expanding a commit-SHA prefix.
    fn commit_expand_dir(&self, _url: &url::Url, prefix: &str) -> PathBuf {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        self.cache_root()
            .join(".commit-expand")
            .join(format!("{prefix}-{ts}"))
    }

    /// Computes the materialization plan for a Git dependency.
    ///
    /// Resolves the commit (locked or fresh), derives cache paths from
    /// the URL and commit, and validates lockfile consistency when in
    /// locked mode. The returned plan carries everything
    /// [`materialize_git`](Self::materialize_git) needs to run the
    /// sparse checkout and verify the result.
    async fn plan_git_materialization(
        &self,
        name: &DependencyName,
        url: &url::Url,
        selector: &GitSelector,
        path: &Option<GitModulePath>,
        scope: DependencyScope,
        mode: ResolutionMode<'_>,
    ) -> Result<GitMaterializationPlan, ResolverError> {
        let path_prefix = path.as_ref().map(GitModulePath::as_str).map(str::to_string);

        let (selected_version, commit) = match mode {
            ResolutionMode::Locked { lockfile_scope } => {
                let locked_entry =
                    self.lockfile
                        .find_scoped(lockfile_scope, name)
                        .ok_or_else(|| ResolverError::NotInLockfile {
                            dep: name.manifest().to_string(),
                        })?;
                let (locked_url, locked_commit, locked_path, locked_selector) =
                    match &locked_entry.source {
                        ResolvedSource::Git {
                            git: lu,
                            sha: lc,
                            path: lp,
                            selector: ls,
                        } => (lu, lc, lp, ls),
                        _ => {
                            return Err(ResolverError::NotInLockfile {
                                dep: name.manifest().to_string(),
                            });
                        }
                    };
                if url != locked_url
                    || path != locked_path
                    || !locked_selector_satisfies(
                        locked_entry,
                        selector,
                        locked_commit,
                        locked_selector,
                    )
                {
                    return Err(ResolverError::LockfileSourceMismatch {
                        dep: name.manifest().to_string(),
                    });
                }
                (None, locked_commit.clone())
            }
            ResolutionMode::Fresh => {
                self.resolve_git_selector(name, url, selector, path_prefix.as_deref(), scope)
                    .await?
            }
        };

        let key = CacheKey::from_git_url(url, &commit);
        let leaf = key.absolute_path(&self.cache_root);
        let sparse_path = path_prefix.clone().unwrap_or_else(|| ".".to_string());
        let module_path = match path.as_ref() {
            Some(p) => leaf.join(p.as_path()),
            None => leaf.clone(),
        };
        tracing::trace!(
            dependency = name.manifest(),
            cache_root = %self.cache_root.display(),
            cache_leaf = %leaf.display(),
            commit = %commit,
            sparse_path = %sparse_path,
            "planned module cache location"
        );

        Ok(GitMaterializationPlan {
            selected_version,
            commit,
            leaf,
            sparse_path,
            module_path,
        })
    }
}

#[cfg(feature = "git-resolver")]
#[async_trait]
impl Resolver for GitResolver {
    async fn materialize(
        &self,
        consumer: &Module,
        path: &SymbolicPath,
    ) -> Result<MaterializedFile, ResolverError> {
        // Look up the dependency declaration in the consumer's manifest.
        let name = path.dep_name();
        tracing::debug!(dep = %name.manifest(), "materializing symbolic import");
        let scope = if consumer.lockfile_scope.is_empty() {
            DependencyScope::TopLevel
        } else {
            DependencyScope::Transitive
        };
        let source = consumer.manifest.dependencies.get(name).ok_or_else(|| {
            ResolverError::NotADependency {
                name: name.manifest().to_string(),
            }
        })?;

        // Enforce URL scheme and host policy before any network access.
        if let DependencySource::Git { url, .. } = source {
            self.policy.check_git_url(name, url, scope)?;
        }

        // Materialize the dependency on disk and read its manifest.
        let (resolved_source, manifest, module_root) = match source {
            DependencySource::LocalPath { path, .. } => {
                let resolved_path = consumer.resolve_local_path(path);
                self.validate_locked_local(consumer, name, &resolved_path)?;
                let manifest = read_manifest(&resolved_path)?;
                let resolved = ResolvedSource::Path {
                    path: resolved_path.clone(),
                };
                let root = MaterializedRoot::Local(resolved_path);
                (resolved, manifest, root)
            }
            DependencySource::Git {
                url,
                selector,
                path,
                ..
            } => {
                let plan = self
                    .plan_git_materialization(
                        name,
                        url,
                        selector,
                        path,
                        scope,
                        ResolutionMode::Locked {
                            lockfile_scope: &consumer.lockfile_scope,
                        },
                    )
                    .await?;
                let root = self.materialize_git(name, url, scope, &plan).await?;
                let manifest = read_manifest(&plan.module_path)?;
                let resolved = ResolvedSource::Git {
                    git: url.clone(),
                    sha: plan.commit,
                    path: path.clone(),
                    selector: selector.clone(),
                };
                (resolved, manifest, root)
            }
        };

        // Verify the content hash, signature, and trust pin against the
        // lockfile. Local path sources carry no checksum and are read
        // as-is, so only structural validation runs for them.
        let root_path = module_root.module_root();
        match &resolved_source {
            ResolvedSource::Path { .. } => {
                crate::resolver::verify::verify_structure(&self.policy, name, root_path)?;
            }
            ResolvedSource::Git { .. } => {
                let source_url = resolved_source.source_url();
                let source_path = resolved_source.source_path();
                let verified = crate::resolver::verify::verify(
                    &self.policy,
                    &self.trust,
                    name,
                    root_path,
                    Some((&source_url, source_path)),
                )?;

                crate::resolver::verify::verify_against_lockfile(
                    &self.lockfile,
                    &self.trust,
                    &consumer.lockfile_scope,
                    name,
                    &verified.checksum,
                    verified.signer.as_ref().map(|signer| &signer.key),
                    verified
                        .signer
                        .as_ref()
                        .and_then(|signer| signer.identity.as_ref()),
                )?;
            }
        }

        // Resolve the symbolic path to a concrete `.wdl` file path.
        let (rel, kind) = match path.sub_path() {
            None => {
                let p = manifest.entrypoint_filename();
                (
                    RelativePath::try_from(Path::new(p))?,
                    MissingFileKind::Entrypoint,
                )
            }
            Some(sub) => {
                // Match each component against on-disk entries with
                // hyphen-to-underscore normalization, so `my_task`
                // resolves `my_task.wdl` or `my-task.wdl`.
                let s = sub.display().to_string().replace('\\', "/");
                let rel = resolve_normalized_subpath(root_path, &s, name)?;
                (rel, MissingFileKind::SubPath)
            }
        };

        // Reject paths that match the manifest's exclude globs.
        if exclude_set(&manifest.exclude)?.is_match(rel.as_path()) {
            return Err(ResolverError::MissingFile {
                dep: name.manifest().to_string(),
                path: rel.as_path().to_path_buf(),
                kind: MissingFileKind::Excluded,
            });
        }

        // Canonicalize the path, enforcing symlink containment.
        let canonical =
            resolve_content_file(module_root.module_root(), &rel, name).map_err(|e| match e {
                ResolverError::Io { source, .. }
                    if source.kind() == std::io::ErrorKind::NotFound =>
                {
                    ResolverError::MissingFile {
                        dep: name.manifest().to_string(),
                        path: rel.as_path().to_path_buf(),
                        kind,
                    }
                }
                other => other,
            })?;

        Ok(MaterializedFile {
            path: canonical,
            module_root: root_path.to_path_buf(),
            source: resolved_source,
            manifest: std::sync::Arc::new(manifest),
        })
    }

    async fn resolve_tree(&self, consumer: &Module) -> Result<ResolvedTree, ResolverError> {
        // Walk every transitive dependency starting from the consumer's
        // direct dependencies, collecting the full resolved tree.
        let mut chain: Vec<(DependencyName, ResolvedSource)> = Vec::new();
        let dependencies = self
            .resolve_dependencies(
                &consumer.manifest.dependencies,
                &consumer.root,
                None,
                &mut chain,
            )
            .await?;
        Ok(ResolvedTree { dependencies })
    }

    async fn discover_versions(
        &self,
        name: &DependencyName,
        source: &DependencySource,
        scope: DependencyScope,
    ) -> Result<Vec<Version>, ResolverError> {
        match source {
            DependencySource::Git {
                url,
                selector,
                path,
                ..
            } => {
                // Only version selectors produce a meaningful version list;
                // tag, branch, and commit selectors resolve to at most one
                // version that is not yet known.
                let GitSelector::Version(requirement) = selector else {
                    return Ok(Vec::new());
                };

                // List remote tags and filter to those satisfying the
                // semver requirement.
                let fetcher = self.fetcher();
                let dep = name.clone();
                let url = url.clone();
                let path_prefix = path.as_ref().map(GitModulePath::as_str).map(str::to_string);
                let requirement = requirement.clone();
                tokio::task::spawn_blocking(move || -> Result<Vec<Version>, ResolverError> {
                    let refs = fetcher.list_tags(&dep, &url, scope)?;
                    Ok(crate::resolver::versions::filter_matching(
                        &refs,
                        path_prefix.as_deref(),
                        &requirement,
                    ))
                })
                .await
                // The spawned closure performs pure libgit2 work
                // and does not panic; a `JoinError` would only fire on
                // runtime shutdown, in which case re-panicking is fine.
                .unwrap()
            }
            DependencySource::LocalPath { .. } => Ok(Vec::new()),
        }
    }
}

#[cfg(feature = "git-resolver")]
/// Pre-computed materialization parameters for a Git dependency.
#[derive(Debug)]
struct GitMaterializationPlan {
    /// The selected version from tag resolution, if any.
    selected_version: Option<Version>,
    /// The resolved commit SHA.
    commit: GitCommit,
    /// The absolute path to the cache leaf directory.
    leaf: PathBuf,
    /// The sparse-checkout path (`path_prefix` or `"."`).
    sparse_path: String,
    /// The absolute path to the module root within the cache leaf.
    module_path: PathBuf,
}

#[cfg(feature = "git-resolver")]
/// Distinguishes resolver-owned cache paths from user-owned local
/// paths. Only `Cached` variants may be evicted.
#[derive(Clone, Debug)]
enum MaterializedRoot {
    /// A user's local module directory. Must never be evicted.
    Local(PathBuf),
    /// A resolver-owned cache leaf.
    Cached {
        /// Whether this call cloned the cache leaf instead of using an
        /// existing checkout.
        fetched: bool,
        /// The module content root inside the cache leaf.
        module_root: PathBuf,
    },
}

#[cfg(feature = "git-resolver")]
impl MaterializedRoot {
    /// Returns the module root regardless of ownership.
    fn module_root(&self) -> &Path {
        match self {
            Self::Local(root) => root,
            Self::Cached { module_root, .. } => module_root,
        }
    }
}

#[cfg(feature = "git-resolver")]
/// Returns true when a lockfile entry can satisfy the current Git
/// selector in `module.json`.
fn locked_selector_satisfies(
    _entry: &DependencyEntry,
    selector: &GitSelector,
    locked_commit: &GitCommit,
    locked_selector: &GitSelector,
) -> bool {
    match selector {
        GitSelector::Version(_) => selector == locked_selector,
        GitSelector::Commit(commit) => locked_commit.as_str().starts_with(commit.as_str()),
        GitSelector::Tag(tag) => {
            matches!(locked_selector, GitSelector::Tag(locked) if locked == tag)
        }
        GitSelector::Branch(branch) => {
            matches!(locked_selector, GitSelector::Branch(locked) if locked == branch)
        }
    }
}

#[cfg(feature = "git-resolver")]
/// Resolves a relative content path under `root` to a concrete file.
///
/// Symbolic links are not permitted anywhere in a module tree, so a
/// resolved path that is a symbolic link makes the module invalid. The
/// whole-tree walk performed during verification also enforces this;
/// the check here guards the specific imported file.
fn resolve_content_file(
    root: &Path,
    rel: &crate::relative_path::RelativePath,
    dep: &DependencyName,
) -> Result<PathBuf, ResolverError> {
    if rel
        .as_str()
        .split('/')
        .any(|name| NON_MODULE_CONTENT.contains(&name))
    {
        return Err(ResolverError::Io {
            path: root.join(rel.as_path()),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "path resolves to non-module content",
            ),
        });
    }

    let candidate = root.join(rel.as_path());
    let meta = match std::fs::symlink_metadata(&candidate) {
        Ok(meta) => meta,
        Err(source) => {
            return Err(ResolverError::Io {
                path: candidate,
                source,
            });
        }
    };

    if meta.file_type().is_symlink() {
        return Err(ResolverError::MaterializedSymlink {
            dep: dep.manifest().to_string(),
            path: candidate,
        });
    }

    candidate
        .canonicalize()
        .map_err(|source| ResolverError::Io {
            path: candidate,
            source,
        })
}

#[cfg(feature = "git-resolver")]
/// Reads and parses `module.json` from `dir`.
fn read_manifest(dir: &Path) -> Result<Manifest, ResolverError> {
    let path = dir.join(crate::MANIFEST_FILENAME);
    let bytes = std::fs::read(&path).map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => ResolverError::MissingManifest { path: path.clone() },
        _ => ResolverError::Io {
            path: path.clone(),
            source,
        },
    })?;
    Manifest::parse(&bytes).map_err(ResolverError::from)
}

#[cfg(feature = "git-resolver")]
/// Resolves a symbolic sub-path to an on-disk relative path, matching
/// each `/`-separated component against directory entries with
/// hyphen-to-underscore normalization.
///
/// A component matches a directory entry whose name, after replacing
/// every `-` with `_`, equals the component (with `.wdl` appended for
/// the final component). Intermediate components must match a directory
/// and the final component a file. If more than one entry in a directory
/// matches, resolution fails with [`ResolverError::AmbiguousSubPath`]. A
/// component with no match yields a `NotFound` I/O error that the caller
/// maps to a missing-file error.
fn resolve_normalized_subpath(
    root: &Path,
    sub: &str,
    dep: &DependencyName,
) -> Result<RelativePath, ResolverError> {
    let components: Vec<&str> = sub.split('/').collect();
    let mut current = root.to_path_buf();
    let mut parts: Vec<String> = Vec::with_capacity(components.len());

    for (i, component) in components.iter().enumerate() {
        let is_final = i + 1 == components.len();
        let target = if is_final {
            format!("{component}.wdl")
        } else {
            (*component).to_string()
        };

        let mut matches: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(&current).map_err(|source| ResolverError::Io {
            path: current.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| ResolverError::Io {
                path: current.clone(),
                source,
            })?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.replace('-', "_") != target {
                continue;
            }
            // Intermediate components must be directories; the final
            // component must be a file.
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir == is_final {
                continue;
            }
            matches.push(name);
        }

        match matches.len() {
            0 => {
                return Err(ResolverError::Io {
                    path: current.join(&target),
                    source: std::io::Error::from(std::io::ErrorKind::NotFound),
                });
            }
            1 => {
                current.push(&matches[0]);
                parts.push(matches.pop().unwrap());
            }
            _ => {
                matches.sort();
                return Err(ResolverError::AmbiguousSubPath {
                    dep: dep.manifest().to_string(),
                    path: sub.to_string(),
                    entries: matches,
                });
            }
        }
    }

    RelativePath::try_from(Path::new(&parts.join("/"))).map_err(ResolverError::from)
}

/// Compiles a manifest's `exclude` patterns into a [`globset::GlobSet`].
///
/// Patterns use gitignore-style semantics per the module specification:
/// `*` matches any run of non-separator characters, `**` matches across
/// separators, and a plain directory name excludes the directory and
/// everything beneath it. To honor the directory-subtree rule, each
/// pattern is compiled both literally and with a trailing `/**`, and
/// `literal_separator` is enabled so a single `*` does not cross `/`.
#[cfg(feature = "git-resolver")]
fn exclude_set(
    patterns: &[crate::relative_path::RelativePath],
) -> Result<globset::GlobSet, ResolverError> {
    if patterns.is_empty() {
        return Ok(globset::GlobSet::empty());
    }
    let mut builder = globset::GlobSetBuilder::new();
    for p in patterns {
        let s: &str = p.as_ref();
        let compile = |glob: &str| {
            globset::GlobBuilder::new(glob)
                .literal_separator(true)
                .build()
                .map_err(|source| ResolverError::InvalidExclude {
                    pattern: s.to_string(),
                    source,
                })
        };
        builder.add(compile(s)?);
        builder.add(compile(&format!("{}/**", s.trim_end_matches('/')))?);
    }
    // `GlobSetBuilder::build` only consolidates already-compiled
    // globs; `GlobBuilder::build` above is the validating step, so by the
    // time we reach this call there is nothing left for `build` to reject.
    Ok(builder.build().unwrap())
}

#[cfg(all(test, feature = "git-resolver"))]
mod tests;
