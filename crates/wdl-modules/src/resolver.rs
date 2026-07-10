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
impl GitResolver {
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
            // SAFETY: the closure performs pure libgit2 work and does
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
                &leaf_for_clone,
            )
        })
        .await
        // SAFETY: the closure performs only libgit2 work and
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
                        // SAFETY: the closure performs only Git work; a
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
                        // SAFETY: the closure does not panic.
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
                        // SAFETY: the closure does not panic.
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
                        // SAFETY: `is_full` guarantees exactly 40 lowercase
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
                // SAFETY: the closure performs only Git work and does not panic.
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
                // SAFETY: the spawned closure performs pure libgit2 work
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
    // SAFETY: `GlobSetBuilder::build` only consolidates already-compiled
    // globs; `GlobBuilder::build` above is the validating step, so by the
    // time we reach this call there is nothing left for `build` to reject.
    Ok(builder.build().unwrap())
}

#[cfg(all(test, feature = "git-resolver"))]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use tempfile::TempDir;
    use tempfile::tempdir;

    use super::*;

    /// Builds a `Module` from a parsed `Manifest` and the directory it
    /// lives in.
    fn module(manifest: Manifest, root: &Path) -> Module {
        Module::new(Arc::new(manifest), root.to_path_buf())
    }

    fn checksum() -> crate::hash::ContentHash {
        "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            .parse()
            .unwrap()
    }

    /// Builds a `module.json` at `dir` with the given name, version, and
    /// Converts a path to a JSON-safe string (forward slashes on all
    /// platforms).
    fn json_path(p: &Path) -> String {
        p.display().to_string().replace('\\', "/")
    }

    /// optional `dependencies` map (each value is the JSON-encoded
    /// dependency source).
    fn write_manifest(dir: &Path, name: &str, version: &str, deps: &[(&str, &str)]) {
        fs::create_dir_all(dir).unwrap();
        let deps_obj = if deps.is_empty() {
            String::new()
        } else {
            let entries: Vec<String> = deps.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect();
            format!(",\"dependencies\":{{{}}}", entries.join(","))
        };
        let body = format!(
            "{{\"name\":\"{name}\",\"version\":\"{version}\",\"license\":\"MIT\"{deps_obj}}}"
        );
        fs::write(dir.join(crate::MANIFEST_FILENAME), body).unwrap();
    }

    fn resolver(cache: &TempDir) -> GitResolver {
        resolver_with_lockfile(cache, Lockfile::default())
    }

    fn resolver_with_lockfile(cache: &TempDir, lockfile: Lockfile) -> GitResolver {
        GitResolver::builder()
            .cache_root(cache.path())
            .trust(TrustStore::default())
            .lockfile(lockfile)
            .build()
    }

    /// Resolves a consumer's tree and builds a lockfile from it.
    async fn resolve_and_lock(cache: &TempDir, consumer: &Module) -> (GitResolver, Lockfile) {
        resolve_and_lock_with_config(
            cache,
            consumer,
            ResolverPolicy::default(),
            TrustStore::default(),
        )
        .await
    }

    async fn resolve_and_lock_with_config(
        cache: &TempDir,
        consumer: &Module,
        policy: ResolverPolicy,
        trust: TrustStore,
    ) -> (GitResolver, Lockfile) {
        let r = resolver(cache);
        let tree = r.resolve_tree(consumer).await.unwrap();
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

    /// Writes a `module.sig` next to `dir`'s `module.json` over the
    /// directory's content hash.
    fn write_signature(dir: &Path, signer: &crate::signing::SigningKey) {
        let digest = crate::hash::hash_directory(dir).unwrap();
        let signature = signer.sign(&digest);
        let sig = crate::signing::ModuleSignature {
            public_key: signer.verifying_key(),
            identity: None,
            signature,
        };
        let mut buf = Vec::new();
        sig.write(&mut buf).unwrap();
        fs::write(dir.join(crate::SIGNATURE_FILENAME), buf).unwrap();
    }

    #[test]
    fn builds_with_explicit_paths() {
        let cache = tempdir().unwrap();
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust(TrustStore::default())
            .lockfile(Lockfile::default())
            .build();
        assert_eq!(r.cache_root(), cache.path());
        assert!(r.trust_store().keys.is_empty());
    }

    #[tokio::test]
    async fn transitive_dep_on_unallowed_host_is_rejected() {
        let cache = tempdir().unwrap();
        let r = resolver(&cache);

        let dep: DependencyName = "widget".parse().unwrap();
        let source: DependencySource = serde_json::from_str(
            r#"{"git": "https://bitbucket.org/acme/widget", "version": "^1.0.0"}"#,
        )
        .unwrap();

        let err = r
            .discover_versions(&dep, &source, DependencyScope::Transitive)
            .await
            .unwrap_err();

        assert_eq!(
            err.to_string(),
            "`widget` git URL `https://bitbucket.org/acme/widget` targets host `bitbucket.org` \
             which is not in the configured allow list; to allow it, add `bitbucket.org` to \
             `allowed_transitive_hosts` in the `[modules]` section of your `sprocket.toml`"
        );
    }

    #[tokio::test]
    async fn github_rejected_when_removed_from_transitive_allowlist() {
        let cache = tempdir().unwrap();
        let policy = ResolverPolicy::try_from(&ModulesConfig {
            allowed_transitive_hosts: vec!["gitlab.com".into()],
            ..ModulesConfig::default()
        })
        .unwrap();
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust(TrustStore::default())
            .lockfile(Lockfile::default())
            .policy(policy)
            .build();

        let dep: DependencyName = "widget".parse().unwrap();
        let source: DependencySource = serde_json::from_str(
            r#"{"git": "https://github.com/acme/widget", "version": "^1.0.0"}"#,
        )
        .unwrap();

        let err = r
            .discover_versions(&dep, &source, DependencyScope::Transitive)
            .await
            .unwrap_err();

        assert_eq!(
            err.to_string(),
            "`widget` git URL `https://github.com/acme/widget` targets host `github.com` which is \
             not in the configured allow list; to allow it, add `github.com` to \
             `allowed_transitive_hosts` in the `[modules]` section of your `sprocket.toml`"
        );
    }

    #[tokio::test]
    async fn resolve_tree_recurses_into_local_path_deps() {
        let workdir = tempdir().unwrap();
        let consumer_dir = workdir.path().join("consumer");
        let dep_dir = workdir.path().join("dep");

        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);

        let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap();
        let consumer = Manifest::parse(&bytes).unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let tree = resolver(&cache).resolve_tree(&consumer).await.unwrap();
        let dep = tree.dependencies.get(&"dep".parse().unwrap()).unwrap();
        assert!(matches!(&dep.source, ResolvedSource::Path { .. }));
        assert!(dep.dependencies.is_empty());
        assert_eq!(dep.version, None);
    }

    fn hash_from_byte(byte: u8) -> crate::hash::ContentHash {
        format!("sha256:{}", hex::encode([byte; 32]))
            .parse()
            .unwrap()
    }

    #[test]
    fn cycle_identity_ignores_commit_and_selector() {
        // Same repository URL and sub-path but different resolved commit
        // and selector: identical coordinates, so a self-dependency is a
        // cycle even at a different version.
        let a = ResolvedSource::Git {
            git: "https://example.com/repo".parse().unwrap(),
            sha: "a".repeat(40).parse().unwrap(),
            selector: GitSelector::Version("^1".parse().unwrap()),
            path: Some("pkg".parse().unwrap()),
        };
        let b = ResolvedSource::Git {
            git: "https://example.com/repo".parse().unwrap(),
            sha: "b".repeat(40).parse().unwrap(),
            selector: GitSelector::Version("^2".parse().unwrap()),
            path: Some("pkg".parse().unwrap()),
        };
        assert_eq!(a.coordinates(), b.coordinates());

        // A different sub-path is a different module.
        let c = ResolvedSource::Git {
            git: "https://example.com/repo".parse().unwrap(),
            sha: "a".repeat(40).parse().unwrap(),
            selector: GitSelector::Version("^1".parse().unwrap()),
            path: Some("other".parse().unwrap()),
        };
        assert_ne!(a.coordinates(), c.coordinates());
    }

    fn rel(s: &str) -> crate::relative_path::RelativePath {
        s.parse().unwrap()
    }

    #[test]
    fn exclude_set_honors_gitignore_semantics() {
        let patterns = [rel("internal"), rel("scratch/*.wdl"), rel("secret/**")];
        let set = exclude_set(&patterns).unwrap();

        // A plain directory name excludes everything beneath it.
        assert!(set.is_match(Path::new("internal/private.wdl")));
        assert!(set.is_match(Path::new("internal/deep/nested.wdl")));
        // `*` matches within a single path segment only.
        assert!(set.is_match(Path::new("scratch/tmp.wdl")));
        assert!(!set.is_match(Path::new("scratch/sub/tmp.wdl")));
        // `**` crosses separators.
        assert!(set.is_match(Path::new("secret/a/b/c.wdl")));
        // Unrelated paths are not excluded.
        assert!(!set.is_match(Path::new("public.wdl")));
    }

    #[test]
    fn resolve_normalized_subpath_matches_hyphen_variant() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("my-tasks")).unwrap();
        fs::write(dir.path().join("my-tasks/do-thing.wdl"), b"version 1.2\n").unwrap();
        let dep: DependencyName = "dep".parse().unwrap();

        // The symbolic components use underscores; the files use hyphens.
        let resolved = resolve_normalized_subpath(dir.path(), "my_tasks/do_thing", &dep).unwrap();
        assert_eq!(resolved.as_path(), Path::new("my-tasks/do-thing.wdl"));
    }

    #[test]
    fn resolve_normalized_subpath_reports_ambiguity() {
        let dir = tempdir().unwrap();
        // Two files normalize to the same component `my_task`.
        fs::write(dir.path().join("my_task.wdl"), b"version 1.2\n").unwrap();
        fs::write(dir.path().join("my-task.wdl"), b"version 1.2\n").unwrap();
        let dep: DependencyName = "dep".parse().unwrap();

        let err = resolve_normalized_subpath(dir.path(), "my_task", &dep).unwrap_err();
        assert!(
            matches!(err, ResolverError::AmbiguousSubPath { .. }),
            "expected `AmbiguousSubPath`, got: {err}"
        );
    }

    #[test]
    fn resolve_normalized_subpath_missing_is_not_found() {
        let dir = tempdir().unwrap();
        let dep: DependencyName = "dep".parse().unwrap();
        let err = resolve_normalized_subpath(dir.path(), "nope", &dep).unwrap_err();
        assert!(
            matches!(&err, ResolverError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound),
            "expected `NotFound` I/O error, got: {err}"
        );
    }

    #[tokio::test]
    async fn materialize_returns_not_in_lockfile_when_dep_missing_from_lock() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("index.wdl"), b"workflow w {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let err = resolver(&cache)
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .unwrap_err();
        assert!(
            matches!(err, ResolverError::NotInLockfile { .. }),
            "expected `NotInLockfile`, got: {err}"
        );
    }

    #[tokio::test]
    async fn local_path_dep_records_no_checksum() {
        // Local path sources carry no checksum; their lockfile entry
        // records `None` and their content is read as-is.
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("index.wdl"), b"workflow w {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let (_, lockfile) = resolve_and_lock(&cache, &consumer).await;
        let dep_name = "dep".parse().unwrap();
        assert_eq!(
            lockfile.dependencies.get(&dep_name).unwrap().checksum,
            None,
            "local path deps carry no checksum"
        );
    }

    #[tokio::test]
    async fn materialize_succeeds_with_matching_lockfile_checksum() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("index.wdl"), b"workflow w {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let (r, _) = resolve_and_lock(&cache, &consumer).await;
        let mat = r
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .unwrap();
        assert_eq!(mat.path, dep_dir.join("index.wdl").canonicalize().unwrap());
    }

    #[tokio::test]
    async fn materialize_reads_local_path_content_as_is() {
        // Local path content is read as-is at materialization time, so
        // content that changed since locking does not fail the build.
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("index.wdl"), b"workflow w {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let (_, lockfile) = resolve_and_lock(&cache, &consumer).await;

        // Mutate the dep content after locking.
        fs::write(dep_dir.join("extra.wdl"), b"workflow extra {}").unwrap();

        let r = resolver_with_lockfile(&cache, lockfile);
        let mat = r
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .expect("local path content is read as-is, not checksum-verified");
        assert_eq!(mat.path, dep_dir.join("index.wdl").canonicalize().unwrap());
    }

    #[tokio::test]
    async fn materialize_rejects_changed_local_path_after_lock() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("index.wdl"), b"workflow w {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let (_, lockfile) = resolve_and_lock(&cache, &consumer).await;

        // Change the manifest to point to a different path.
        let other_dir = workdir.path().join("other");
        write_manifest(&other_dir, "dep", "1.0.0", &[]);
        fs::write(other_dir.join("index.wdl"), b"workflow w {}").unwrap();
        let other_src = format!("{{\"path\":\"{}\"}}", json_path(&other_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &other_src)]);
        let consumer2 =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer2 = module(consumer2, &consumer_dir);

        let r = resolver_with_lockfile(&cache, lockfile);
        let err = r
            .materialize(&consumer2, &"dep".parse().unwrap())
            .await
            .unwrap_err();
        assert!(
            matches!(err, ResolverError::LockfileSourceMismatch { .. }),
            "expected `LockfileSourceMismatch`, got: {err}"
        );
    }

    #[tokio::test]
    async fn materialize_checks_transitive_git_policy_for_child_module()
    -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempdir()?;
        let child_dir = workdir.path().join("child");
        let ssh_dep = r#"{"git":"ssh://git@github.com/openwdl/tasks","commit":"0000000000000000000000000000000000000001"}"#;
        write_manifest(&child_dir, "child", "1.0.0", &[("dep", ssh_dep)]);
        let child = Manifest::parse(&fs::read(child_dir.join(crate::MANIFEST_FILENAME))?)?;

        let parent_dir = workdir.path().join("parent");
        write_manifest(&parent_dir, "parent", "1.0.0", &[]);
        let parent = Manifest::parse(&fs::read(parent_dir.join(crate::MANIFEST_FILENAME))?)?;
        let parent = module(parent, &parent_dir);
        let child_name = "child".parse()?;
        let child = parent.child(child_name, Arc::new(child), child_dir);

        let cache = tempdir()?;
        let symbolic_path = "dep".parse()?;
        let err = match resolver(&cache).materialize(&child, &symbolic_path).await {
            Ok(_) => panic!("expected transitive git policy rejection"),
            Err(err) => err,
        };
        assert!(
            matches!(err, ResolverError::GitUrlPolicyViolation { .. }),
            "expected `GitUrlPolicyViolation`, got: {err}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn materialize_uses_top_level_git_policy_for_top_level_module()
    -> Result<(), Box<dyn std::error::Error>> {
        let workdir = tempdir()?;
        let consumer_dir = workdir.path().join("consumer");
        let ssh_dep = r#"{"git":"ssh://git@github.com/openwdl/tasks","commit":"0000000000000000000000000000000000000001"}"#;
        write_manifest(&consumer_dir, "consumer", "1.0.0", &[("dep", ssh_dep)]);
        let consumer = Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME))?)?;
        let consumer = module(consumer, &consumer_dir);
        assert!(
            consumer.lockfile_scope.is_empty(),
            "consumer must be a top-level `Module`"
        );

        let cache = tempdir()?;
        let symbolic_path = "dep".parse()?;
        let err = match resolver(&cache)
            .materialize(&consumer, &symbolic_path)
            .await
        {
            Ok(_) => panic!("expected lockfile rejection, not git policy rejection"),
            Err(err) => err,
        };
        assert!(
            !matches!(err, ResolverError::GitUrlPolicyViolation { .. }),
            "top-level `ssh://` dep must pass `DependencyScope::TopLevel` policy; got: {err}"
        );
        Ok(())
    }

    fn locked_git_resolver(cache: &TempDir, dep: &str, entry: DependencyEntry) -> GitResolver {
        let mut lockfile = Lockfile::default();
        lockfile.dependencies.insert(dep.parse().unwrap(), entry);
        resolver_with_lockfile(cache, lockfile)
    }

    fn locked_git_entry(selector: GitSelector) -> DependencyEntry {
        DependencyEntry {
            source: ResolvedSource::Git {
                git: "https://github.com/openwdl/tasks".parse().unwrap(),
                sha: "0000000000000000000000000000000000000001".parse().unwrap(),
                path: None,
                selector,
            },
            checksum: Some(checksum()),
            signer: None,
            dependencies: Default::default(),
        }
    }

    #[tokio::test]
    async fn locked_git_materialization_rejects_version_selector_mismatch() {
        let cache = tempdir().unwrap();
        let r = locked_git_resolver(
            &cache,
            "dep",
            locked_git_entry(GitSelector::Version("^1".parse().unwrap())),
        );
        let dep = "dep".parse().unwrap();
        let url = "https://github.com/openwdl/tasks".parse().unwrap();
        let selector = GitSelector::Version("^2".parse().unwrap());
        let err = r
            .plan_git_materialization(
                &dep,
                &url,
                &selector,
                &None,
                DependencyScope::TopLevel,
                ResolutionMode::Locked {
                    lockfile_scope: &[],
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ResolverError::LockfileSourceMismatch { .. }));
    }

    #[tokio::test]
    async fn locked_git_materialization_rejects_commit_selector_mismatch() {
        let cache = tempdir().unwrap();
        let r = locked_git_resolver(
            &cache,
            "dep",
            locked_git_entry(GitSelector::Version("^1".parse().unwrap())),
        );
        let dep = "dep".parse().unwrap();
        let url = "https://github.com/openwdl/tasks".parse().unwrap();
        let selector =
            GitSelector::Commit("0000000000000000000000000000000000000002".parse().unwrap());
        let err = r
            .plan_git_materialization(
                &dep,
                &url,
                &selector,
                &None,
                DependencyScope::TopLevel,
                ResolutionMode::Locked {
                    lockfile_scope: &[],
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ResolverError::LockfileSourceMismatch { .. }));
    }

    #[tokio::test]
    async fn locked_git_materialization_rejects_tag_selector_mismatch() {
        let cache = tempdir().unwrap();
        let r = locked_git_resolver(
            &cache,
            "dep",
            locked_git_entry(GitSelector::Tag("v1.0.0".to_string())),
        );
        let dep = "dep".parse().unwrap();
        let url = "https://github.com/openwdl/tasks".parse().unwrap();
        let selector = GitSelector::Tag("v2.0.0".to_string());
        let err = r
            .plan_git_materialization(
                &dep,
                &url,
                &selector,
                &None,
                DependencyScope::TopLevel,
                ResolutionMode::Locked {
                    lockfile_scope: &[],
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ResolverError::LockfileSourceMismatch { .. }));
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

    #[tokio::test]
    async fn materialize_resolves_default_entrypoint() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("index.wdl"), b"workflow w {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap();
        let consumer = Manifest::parse(&bytes).unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let (r, _) = resolve_and_lock(&cache, &consumer).await;
        let mat = r
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .unwrap();
        assert_eq!(mat.path, dep_dir.join("index.wdl").canonicalize().unwrap());
        assert!(matches!(mat.source, ResolvedSource::Path { .. }));
    }

    #[tokio::test]
    async fn materialize_resolves_named_entrypoint() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        fs::create_dir_all(&dep_dir).unwrap();
        // Manifest declares an explicit `entrypoint` other than the
        // default `index.wdl`.
        fs::write(
            dep_dir.join(crate::MANIFEST_FILENAME),
            br#"{"name":"dep","license":"MIT","entrypoint":"main.wdl"}"#,
        )
        .unwrap();
        fs::write(dep_dir.join("main.wdl"), b"workflow w {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let (r, _) = resolve_and_lock(&cache, &consumer).await;
        let mat = r
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .unwrap();
        assert_eq!(mat.path, dep_dir.join("main.wdl").canonicalize().unwrap());
        assert!(matches!(mat.source, ResolvedSource::Path { .. }));
    }

    #[tokio::test]
    async fn materialize_resolves_sub_path_to_wdl_file() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("cut.wdl"), b"workflow cut {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap();
        let consumer = Manifest::parse(&bytes).unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let (r, _) = resolve_and_lock(&cache, &consumer).await;
        let mat = r
            .materialize(&consumer, &"dep/cut".parse().unwrap())
            .await
            .unwrap();
        assert_eq!(mat.path, dep_dir.join("cut.wdl").canonicalize().unwrap());
        assert!(matches!(mat.source, ResolvedSource::Path { .. }));
    }

    #[tokio::test]
    async fn manifest_parse_rejects_invalid_commit_sha() {
        let workdir = tempdir().unwrap();
        let consumer_dir = workdir.path().join("consumer");
        let bad_src = "{\"git\":\"https://example.com/repo.git\",\"commit\":\"not-a-sha\"}";
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", bad_src)]);
        let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap();
        let err = Manifest::parse(&bytes).unwrap_err();
        assert!(
            matches!(err, crate::manifest::ManifestError::InvalidJson(_)),
            "expected `InvalidJson` from manifest parse, got: {err}"
        );
    }

    #[tokio::test]
    async fn materialize_blocks_excluded_glob() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        fs::create_dir_all(dep_dir.join("internal")).unwrap();
        let body = r#"{"name":"dep","license":"MIT","exclude":["internal/**"]}"#;
        fs::write(dep_dir.join(crate::MANIFEST_FILENAME), body).unwrap();
        fs::write(
            dep_dir.join("internal").join("private.wdl"),
            b"workflow w {}",
        )
        .unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let (r, _) = resolve_and_lock(&cache, &consumer).await;
        let err = r
            .materialize(&consumer, &"dep/internal/private".parse().unwrap())
            .await
            .unwrap_err();
        let ResolverError::MissingFile { kind, .. } = err else {
            panic!("expected `MissingFile`, got: {err}");
        };
        assert_eq!(kind, MissingFileKind::Excluded);
    }

    #[tokio::test]
    async fn materialize_rejects_entrypoint_symlink_escaping_dep_root() {
        let workdir = tempdir().unwrap();
        let outside = workdir.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("evil.wdl"), b"workflow evil {}").unwrap();

        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("index.wdl"), b"workflow w {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let (_, lockfile) = resolve_and_lock(&cache, &consumer).await;

        // Replace the entrypoint with a symlink after locking.
        fs::remove_file(dep_dir.join("index.wdl")).unwrap();
        let target = outside.join("evil.wdl");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, dep_dir.join("index.wdl")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&target, dep_dir.join("index.wdl")).unwrap();

        let r = resolver_with_lockfile(&cache, lockfile);
        let err = r
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                ResolverError::MaterializedSymlink { .. }
                    | ResolverError::Walk(crate::module_walk::ModuleWalkError::Symlink(_))
            ),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn materialize_errors_on_undeclared_dependency() {
        let workdir = tempdir().unwrap();
        let consumer_dir = workdir.path().join("consumer");
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[]);
        let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap();
        let consumer = Manifest::parse(&bytes).unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let err = resolver(&cache)
            .materialize(&consumer, &"missing".parse().unwrap())
            .await
            .unwrap_err();
        assert!(matches!(err, ResolverError::NotADependency { .. }));
    }

    #[tokio::test]
    async fn require_signed_exempts_local_path_dep() {
        // `require_signed` gates signature verification, which local
        // path sources are exempt from, so an unsigned local path dep
        // resolves even when the policy requires signatures.
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust(TrustStore::default())
            .policy(
                ResolverPolicy::try_from(&ModulesConfig {
                    require_signed: true,
                    ..ModulesConfig::default()
                })
                .unwrap(),
            )
            .lockfile(Lockfile::default())
            .build();
        let tree = r.resolve_tree(&consumer).await.unwrap();
        let dep = tree.dependencies.get(&"dep".parse().unwrap()).unwrap();
        assert_eq!(dep.signer, None);
    }

    #[tokio::test]
    async fn resolve_tree_verifies_parent_before_transitive_dependencies() {
        // Structural validation (here, the file-count limit) runs on a
        // parent before its transitive dependencies are traversed. The
        // parent exceeds the limit and the child does not, so the error
        // must name the parent.
        let workdir = tempdir().unwrap();
        let child_dir = workdir.path().join("child");
        write_manifest(&child_dir, "child", "1.0.0", &[]);

        let parent_dir = workdir.path().join("parent");
        let child_src = format!("{{\"path\":\"{}\"}}", json_path(&child_dir));
        write_manifest(&parent_dir, "parent", "1.0.0", &[("child", &child_src)]);
        fs::write(parent_dir.join("index.wdl"), b"workflow w {}").unwrap();
        fs::write(parent_dir.join("extra.wdl"), b"workflow e {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let parent_src = format!("{{\"path\":\"{}\"}}", json_path(&parent_dir));
        write_manifest(
            &consumer_dir,
            "consumer",
            "0.1.0",
            &[("parent", &parent_src)],
        );
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust(TrustStore::default())
            .policy(
                ResolverPolicy::try_from(&ModulesConfig {
                    max_materialized_files: Some(2),
                    ..ModulesConfig::default()
                })
                .unwrap(),
            )
            .lockfile(Lockfile::default())
            .build();

        let err = r.resolve_tree(&consumer).await.unwrap_err();
        let ResolverError::MaterializedTreeLimitExceeded { dep, .. } = err else {
            panic!("expected parent validation to run before transitive dependency traversal");
        };
        assert_eq!(dep, "parent");
    }

    #[tokio::test]
    async fn local_path_dep_signature_is_not_verified() {
        // Local path sources are read as-is and are not subject to
        // signature verification, so a signature that no longer matches
        // the (tampered) content does not fail resolution.
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        let signer = crate::signing::test_utils::signing_key_from_seed(7);
        write_signature(&dep_dir, &signer);
        // Modify a file after signing — this would invalidate a Git
        // dependency's signature, but a local path dep is not checked.
        fs::write(dep_dir.join("extra.wdl"), b"workflow w {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let tree = resolver(&cache).resolve_tree(&consumer).await.unwrap();
        let dep = tree.dependencies.get(&"dep".parse().unwrap()).unwrap();
        assert_eq!(dep.signer, None, "local path deps record no signer");
    }

    #[tokio::test]
    async fn local_path_dep_bypasses_trust_pin() {
        // Trust pins apply to signature verification, which local path
        // sources are exempt from, so a mismatched pin does not fail
        // resolution of a local path dependency.
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        let signer = crate::signing::test_utils::signing_key_from_seed(7);
        write_signature(&dep_dir, &signer);

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let pinned = crate::signing::test_utils::signing_key_from_seed(99).verifying_key();
        let mut trust = TrustStore::default();
        trust.insert_key(pinned);
        let cache = tempdir().unwrap();
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust(trust)
            .lockfile(Lockfile::default())
            .build();
        let tree = r.resolve_tree(&consumer).await.unwrap();
        let dep = tree.dependencies.get(&"dep".parse().unwrap()).unwrap();
        assert_eq!(dep.signer, None, "local path deps bypass trust pins");
    }

    #[tokio::test]
    async fn resolve_tree_rejects_symlink_escaping_module_root() {
        let workdir = tempdir().unwrap();
        let outside = workdir.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), b"sensitive").unwrap();

        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        let link = dep_dir.join("escape");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&outside, &link).unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let err = resolver(&cache).resolve_tree(&consumer).await.unwrap_err();
        assert!(
            matches!(
                err,
                ResolverError::Walk(crate::module_walk::ModuleWalkError::Symlink(_))
            ),
            "expected `Walk(Symlink)`, got: {err}"
        );
    }

    #[tokio::test]
    async fn resolve_tree_rejects_too_many_materialized_files() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("one.wdl"), b"workflow one {}").unwrap();
        fs::write(dep_dir.join("two.wdl"), b"workflow two {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust(TrustStore::default())
            .lockfile(Lockfile::default())
            .policy(
                ResolverPolicy::try_from(&ModulesConfig {
                    max_materialized_files: Some(1),
                    ..ModulesConfig::default()
                })
                .unwrap(),
            )
            .build();
        let err = r.resolve_tree(&consumer).await.unwrap_err();
        assert!(
            matches!(err, ResolverError::MaterializedTreeLimitExceeded { .. }),
            "expected `MaterializedTreeLimitExceeded`, got: {err}"
        );
    }

    #[tokio::test]
    async fn resolve_tree_rejects_too_many_materialized_bytes() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("big.wdl"), vec![b'x'; 1024]).unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust(TrustStore::default())
            .lockfile(Lockfile::default())
            .policy(
                ResolverPolicy::try_from(&ModulesConfig {
                    max_materialized_bytes: Some(100),
                    ..ModulesConfig::default()
                })
                .unwrap(),
            )
            .build();
        let err = r.resolve_tree(&consumer).await.unwrap_err();
        assert!(
            matches!(err, ResolverError::MaterializedTreeLimitExceeded { .. }),
            "expected `MaterializedTreeLimitExceeded`, got: {err}"
        );
    }

    #[tokio::test]
    async fn tree_limit_does_not_delete_local_path_dep() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("one.wdl"), b"workflow one {}").unwrap();
        fs::write(dep_dir.join("two.wdl"), b"workflow two {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust(TrustStore::default())
            .lockfile(Lockfile::default())
            .policy(
                ResolverPolicy::try_from(&ModulesConfig {
                    max_materialized_files: Some(1),
                    ..ModulesConfig::default()
                })
                .unwrap(),
            )
            .build();
        let err = r.resolve_tree(&consumer).await.unwrap_err();
        assert!(matches!(
            err,
            ResolverError::MaterializedTreeLimitExceeded { .. }
        ));
        assert!(
            dep_dir.exists(),
            "local-path dep directory must survive the limit error"
        );
        assert!(dep_dir.join("one.wdl").exists());
        assert!(dep_dir.join("two.wdl").exists());
    }

    #[tokio::test]
    async fn local_path_relock_records_no_checksum_across_content_changes() {
        // A local path dependency carries no checksum, so its lockfile
        // entry stays checksum-free even as its content changes between
        // relocks; the content is read as-is at execution time.
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("index.wdl"), b"workflow original {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let (_, lockfile_v1) = resolve_and_lock(&cache, &consumer).await;
        assert_eq!(
            lockfile_v1
                .dependencies
                .get(&"dep".parse().unwrap())
                .unwrap()
                .checksum,
            None,
        );

        fs::write(dep_dir.join("index.wdl"), b"workflow changed {}").unwrap();

        let (_, lockfile_v2) = resolve_and_lock(&cache, &consumer).await;
        assert_eq!(
            lockfile_v2
                .dependencies
                .get(&"dep".parse().unwrap())
                .unwrap()
                .checksum,
            None,
        );
    }

    #[tokio::test]
    async fn resolve_tree_detects_self_cycle() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("self-loop");

        // The dep declares itself as one of its own dependencies.
        let self_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&dep_dir, "loop", "1.0.0", &[("loop", &self_src)]);

        let consumer_dir = workdir.path().join("consumer");
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("loop", &self_src)]);
        let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap();
        let consumer = Manifest::parse(&bytes).unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let err = resolver(&cache).resolve_tree(&consumer).await.unwrap_err();
        let ResolverError::Cycle { path } = err else {
            panic!("expected `Cycle`, got: {err}");
        };
        assert_eq!(path.len(), 2, "self-loop should report a 2-element chain");
    }

    #[tokio::test]
    async fn resolve_tree_detects_relative_local_path_cycle() {
        let workdir = tempdir().expect("failed to create temporary directory");
        let consumer_dir = workdir.path().join("consumer");
        let dep_a_dir = workdir.path().join("a");
        let dep_b_dir = workdir.path().join("b");

        write_manifest(&dep_a_dir, "a", "1.0.0", &[("b", r#"{"path":"../b"}"#)]);
        write_manifest(&dep_b_dir, "b", "1.0.0", &[("a", r#"{"path":"../a"}"#)]);
        write_manifest(
            &consumer_dir,
            "consumer",
            "0.1.0",
            &[("a", r#"{"path":"../a"}"#)],
        );

        let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME))
            .expect("failed to read consumer manifest");
        let consumer = Manifest::parse(&bytes).expect("failed to parse consumer manifest");
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().expect("failed to create cache directory");
        let err = resolver(&cache)
            .resolve_tree(&consumer)
            .await
            .expect_err("relative local path cycle should be rejected");
        let ResolverError::Cycle { path } = err else {
            panic!("expected `Cycle`, got: {err}");
        };
        assert_eq!(path, ["a", "b", "a"]);
    }

    #[tokio::test]
    async fn materialize_rejects_entrypoint_symlink_to_nested_metadata() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("index.wdl"), b"workflow w {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let (_, lockfile) = resolve_and_lock(&cache, &consumer).await;

        // Replace the entrypoint with a symlink to nested metadata
        // after locking.
        fs::remove_file(dep_dir.join("index.wdl")).unwrap();
        fs::create_dir_all(dep_dir.join("nested").join(".git")).unwrap();
        fs::write(
            dep_dir.join("nested").join(".git").join("config"),
            b"private",
        )
        .unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            dep_dir.join("nested").join(".git").join("config"),
            dep_dir.join("index.wdl"),
        )
        .unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(
            dep_dir.join("nested").join(".git").join("config"),
            dep_dir.join("index.wdl"),
        )
        .unwrap();

        let r = resolver_with_lockfile(&cache, lockfile);
        let err = r
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                ResolverError::MaterializedSymlink { .. }
                    | ResolverError::Walk(crate::module_walk::ModuleWalkError::Symlink(_))
            ),
            "expected a symlink rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn materialize_accepts_unsigned_when_lockfile_has_no_signer() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("index.wdl"), b"workflow w {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
        let consumer = module(consumer, &consumer_dir);

        let cache = tempdir().unwrap();
        let (r, _) = resolve_and_lock(&cache, &consumer).await;
        let mat = r
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .unwrap();
        assert!(mat.path.exists());
    }

    #[test]
    fn verify_locked_verifies_matching_cache_leaf() {
        let workdir = tempdir().unwrap();
        let consumer_dir = workdir.path().join("consumer");
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
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
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
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
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();
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

    #[tokio::test]
    async fn discover_versions_returns_matching_tags() {
        let upstream = tempdir().unwrap();
        let repo = git2::Repository::init(upstream.path()).unwrap();
        let sig = git2::Signature::now("test", "test@example.com").unwrap();

        write_manifest(upstream.path(), "dep", "1.0.0", &[]);
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let oid = repo
            .commit(Some("HEAD"), &sig, &sig, "v1.0.0", &tree, &[])
            .unwrap();
        repo.tag_lightweight("v1.0.0", &repo.find_object(oid, None).unwrap(), false)
            .unwrap();

        let source = DependencySource::Git {
            url: url::Url::from_file_path(upstream.path()).unwrap(),
            selector: GitSelector::Version("^1".parse().unwrap()),
            path: None,
            extra: Default::default(),
        };

        let cache = tempdir().unwrap();
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust(TrustStore::default())
            .lockfile(Lockfile::default())
            .policy(
                ResolverPolicy::try_from(&ModulesConfig {
                    allowed_schemes: vec!["https".into(), "ssh".into(), "file".into()],
                    ..ModulesConfig::default()
                })
                .unwrap(),
            )
            .build();
        let dep = "tasks".parse().unwrap();
        let versions = r
            .discover_versions(&dep, &source, DependencyScope::TopLevel)
            .await
            .unwrap();
        assert_eq!(
            versions,
            vec![semver::Version::parse("1.0.0").unwrap()],
            "should discover `v1.0.0` tag"
        );
    }
}
