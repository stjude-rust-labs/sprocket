//! The default Git-backed [`Resolver`] implementation.
//!
//! This module owns the resolver state, Git operations, cache lifecycle, and
//! all phases of dependency resolution and materialization. Low-level Git
//! operations live in [`ops`], while tests live in [`tests`].
//!
//! Git dependency materialization owns sparse-checkout planning, cache
//! materialization, manifest reading, and symbolic-path resolution backing
//! the [`Resolver::materialize`] entry point. The public trait method
//! delegates to [`GitResolver::materialize_file`].
//!
//! Fresh dependency resolution owns recursive tree walking, Git selector
//! resolution, remote version discovery, default-branch discovery, and cycle
//! detection backing the [`Resolver::resolve_tree`] and
//! [`Resolver::discover_versions`] entry points. Every network operation is
//! preceded by exactly one [`ResolverPolicy::check_git_url`] check.
//!
//! Locked dependency traversal owns lockfile-driven materialization, cache
//! leaf enumeration, and non-fetching verification of a consumer's locked
//! dependency tree. These operations never resolve selectors fresh. They
//! read commits straight from the lockfile. The lockfile is attacker-influenced
//! input, so [`GitResolver::ensure_locked`] still runs exactly one
//! [`ResolverPolicy::check_git_url`] before each locked materialization fetch.
//!
//! [`Resolver`]: crate::resolver::Resolver
//! [`Resolver::materialize`]: crate::resolver::Resolver::materialize
//! [`Resolver::resolve_tree`]: crate::resolver::Resolver::resolve_tree
//! [`Resolver::discover_versions`]: crate::resolver::Resolver::discover_versions
//! [`ResolverPolicy::check_git_url`]: crate::resolver::ResolverPolicy::check_git_url

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use bon::Builder;
use futures::future::BoxFuture;
use futures::future::FutureExt;
use path_clean::PathClean;
use semver::Version;

use crate::Lockfile;
use crate::Manifest;
use crate::dependency::DependencyName;
use crate::dependency::DependencySource;
use crate::dependency::GitModulePath;
use crate::dependency::GitSelector;
use crate::hash::NON_MODULE_CONTENT;
use crate::lockfile::DependencyMap;
use crate::lockfile::GitCommit;
use crate::lockfile::ResolvedSource;
use crate::module::Module;
use crate::relative_path::RelativePath;
use crate::resolver::Resolver;
use crate::resolver::cache::CacheKey;
use crate::resolver::error::GitRefKind;
use crate::resolver::error::MissingFileKind;
use crate::resolver::error::ResolverError;
use crate::resolver::fetch::GitFetcher;
use crate::resolver::policy::ResolverPolicy;
use crate::resolver::scope::DependencyScope;
use crate::resolver::scope::ResolutionMode;
use crate::resolver::trust::TrustStore;
use crate::resolver::types::MaterializedFile;
use crate::resolver::types::ResolvedDependency;
use crate::resolver::types::ResolvedTree;
use crate::resolver::verify::VerifiedModule;
use crate::symbolic_path::SymbolicPath;

pub(crate) mod ops;
#[cfg(test)]
mod tests;

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

/// Pre-computed materialization parameters for a Git dependency.
#[derive(Debug)]
pub(super) struct GitMaterializationPlan {
    /// The selected version from tag resolution, if any.
    pub(super) selected_version: Option<Version>,
    /// The resolved commit SHA.
    pub(super) commit: GitCommit,
    /// The absolute path to the cache leaf directory.
    pub(super) leaf: PathBuf,
    /// The sparse-checkout path (`path_prefix` or `"."`).
    pub(super) sparse_path: String,
    /// The absolute path to the module root within the cache leaf.
    pub(super) module_path: PathBuf,
}

/// Distinguishes resolver-owned cache paths from user-owned local
/// paths. Only `Cached` variants may be evicted.
#[derive(Clone, Debug)]
pub(super) enum MaterializedRoot {
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

impl MaterializedRoot {
    /// Returns the module root regardless of ownership.
    pub(super) fn module_root(&self) -> &Path {
        match self {
            Self::Local(root) => root,
            Self::Cached { module_root, .. } => module_root,
        }
    }
}

impl GitResolver {
    /// Initializes an empty cache root or validates its ownership marker.
    pub fn initialize_cache(&self) -> Result<(), ResolverError> {
        ops::initialize_cache_root(&self.cache_root)?;
        Ok(())
    }

    /// Removes every materialized module from the owned cache root.
    pub fn clean_all_cache(&self) -> Result<CacheCleanStats, ResolverError> {
        let (modules, bytes) = ops::remove_cache_root(&self.cache_root)?;
        Ok(CacheCleanStats { modules, bytes })
    }

    /// Removes cache leaves reachable from `consumer`'s locked dependency tree.
    pub fn clean_locked_cache(&self, consumer: &Module) -> Result<CacheCleanStats, ResolverError> {
        let leaves = self.locked_cache_leaves(consumer)?;
        let (modules, bytes) = ops::remove_cache_leaves(&self.cache_root, &leaves)?;
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

    /// Materializes a single symbolic import on disk and returns the path
    /// to the resulting file.
    ///
    /// This is the body backing [`Resolver::materialize`]. See that
    /// method's documentation for the full contract.
    ///
    /// [`Resolver::materialize`]: crate::resolver::Resolver::materialize
    pub(super) async fn materialize_file(
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
                let verified = crate::resolver::verify::verify(&self.policy, name, root_path)?;

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

    /// Runs the sparse checkout for a Git dependency and returns its root.
    ///
    /// On failure, cleans up the cache leaf so a corrupt partial
    /// checkout does not persist.
    pub(super) async fn materialize_git(
        &self,
        name: &DependencyName,
        url: &url::Url,
        scope: DependencyScope,
        plan: &GitMaterializationPlan,
    ) -> Result<MaterializedRoot, ResolverError> {
        let fetcher = self.fetcher();
        let url_for_clone = url.clone();
        let leaf_for_clone = plan.leaf.clone();
        let cache_root = self.cache_root().to_path_buf();
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
                &url_for_clone,
                commit_for_clone.as_str(),
                &[sparse_path.as_str()],
                scope,
                ops::CacheLocation {
                    root: &cache_root,
                    leaf: &leaf_for_clone,
                },
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
                if plan.leaf.starts_with(self.cache_root())
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

    /// Computes the materialization plan for a Git dependency.
    ///
    /// Resolves the commit (locked or fresh), derives cache paths from
    /// the URL and commit, and validates lockfile consistency when in
    /// locked mode. The returned plan carries everything
    /// [`materialize_git`](Self::materialize_git) needs to run the
    /// sparse checkout and verify the result.
    pub(super) async fn plan_git_materialization(
        &self,
        name: &DependencyName,
        url: &url::Url,
        selector: &GitSelector,
        path: &Option<GitModulePath>,
        scope: DependencyScope,
        mode: ResolutionMode<'_>,
    ) -> Result<GitMaterializationPlan, ResolverError> {
        let path_prefix = path.as_ref().map(GitModulePath::as_str);

        let (selected_version, commit) = match mode {
            ResolutionMode::Locked { lockfile_scope } => {
                let locked_entry = self
                    .lockfile()
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
                    || !locked_selector_satisfies(selector, locked_commit, locked_selector)
                {
                    return Err(ResolverError::LockfileSourceMismatch {
                        dep: name.manifest().to_string(),
                    });
                }
                (None, locked_commit.clone())
            }
            ResolutionMode::Fresh => {
                self.resolve_git_selector(name, url, selector, path_prefix, scope)
                    .await?
            }
        };

        let key = CacheKey::from_git_url(url, &commit);
        let leaf = key.absolute_path(self.cache_root());
        let sparse_path = path_prefix.unwrap_or(".").to_string();
        let module_path = match path.as_ref() {
            Some(p) => leaf.join(p.as_path()),
            None => leaf.clone(),
        };
        tracing::trace!(
            dependency = name.manifest(),
            cache_root = %self.cache_root().display(),
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

    /// Discovers the default branch advertised by a Git remote.
    pub async fn discover_default_branch(
        &self,
        name: &DependencyName,
        url: &url::Url,
        scope: DependencyScope,
    ) -> Result<String, ResolverError> {
        self.policy.check_git_url(name, url, scope)?;
        let fetcher = self.fetcher();
        let url = url.clone();
        tokio::task::spawn_blocking(move || fetcher.default_branch(&url, scope))
            .await
            // SAFETY: the closure performs pure libgit2 work and does
            // not panic; `JoinError` only occurs on runtime shutdown.
            .unwrap()
    }

    /// Resolves every transitive dependency declared by `consumer`.
    ///
    /// This is the body backing [`Resolver::resolve_tree`].
    ///
    /// [`Resolver::resolve_tree`]: crate::resolver::Resolver::resolve_tree
    pub(super) async fn resolve_fresh_tree(
        &self,
        consumer: &Module,
    ) -> Result<ResolvedTree, ResolverError> {
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

    /// Lists discovered versions for a dependency source that satisfy the
    /// requirement, in descending semver order.
    ///
    /// This is the body backing [`Resolver::discover_versions`].
    ///
    /// [`Resolver::discover_versions`]: crate::resolver::Resolver::discover_versions
    pub(super) async fn discover_matching_versions(
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
                let path_prefix = path.as_ref().map(GitModulePath::as_str).map(str::to_string);
                self.policy.check_git_url(name, url, scope)?;
                let fetcher = self.fetcher();
                let url = url.clone();
                let requirement = requirement.clone();
                tokio::task::spawn_blocking(move || -> Result<Vec<Version>, ResolverError> {
                    let refs = fetcher.list_tags(&url, scope)?;
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

    /// Recursively resolves a dependency map for `resolve_tree`.
    ///
    /// Each iteration checks policy, materializes and verifies the module,
    /// resolves transitive dependencies, and assembles the result.
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
                        let VerifiedModule { checksum, signer } = crate::resolver::verify::verify(
                            &self.policy,
                            name,
                            module_root.module_root(),
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
                // Keep the current dependency in the chain while resolving
                // descendants so cycle errors can report the complete path.
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
                        // Restore the caller's chain before propagating an
                        // error from a descendant.
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
    pub(super) async fn resolve_git_selector(
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
                let url = url.clone();
                let requirement = requirement.clone();
                let path_prefix_owned = path_prefix.map(str::to_string);
                let refs = tokio::task::spawn_blocking(move || fetcher.list_tags(&url, scope))
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
                let url = url.clone();
                let fetcher = self.fetcher();
                let refs = tokio::task::spawn_blocking(move || fetcher.list_tags(&url, scope))
                    .await
                    // SAFETY: the closure performs only Git work and does not panic.
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
                let url = url.clone();
                let fetcher = self.fetcher();
                let refs = tokio::task::spawn_blocking(move || fetcher.list_branches(&url, scope))
                    .await
                    // SAFETY: the closure performs only Git work and does not panic.
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
                    // SAFETY: `is_full` guarantees exactly 40 lowercase hex
                    // characters, which `GitCommit` accepts.
                    let full = GitCommit::try_from(commit.as_str().to_string()).unwrap();
                    return Ok((None, full));
                }
                let url = url.clone();
                let prefix = commit.as_str().to_string();
                let work_dir = self.commit_expand_dir(&prefix);
                let _ = std::fs::remove_dir_all(&work_dir);
                let fetcher = self.fetcher();
                let expand_dir = work_dir.clone();
                let full = tokio::task::spawn_blocking(move || {
                    fetcher.resolve_commit_prefix(&url, &prefix, scope, &expand_dir)
                })
                .await
                // SAFETY: the closure performs only Git work and does not panic.
                .unwrap();
                let _ = std::fs::remove_dir_all(&work_dir);
                let full = full?;
                // SAFETY: a resolved Git OID is always 40 lowercase hex
                // characters, which `GitCommit` accepts.
                let commit = GitCommit::try_from(full).unwrap();
                Ok((None, commit))
            }
        }
    }

    /// Returns a unique temporary directory under the cache root used to
    /// clone a repository while expanding a commit-SHA prefix.
    fn commit_expand_dir(&self, prefix: &str) -> PathBuf {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        self.cache_root()
            .join(".commit-expand")
            .join(format!("{prefix}-{ts}"))
    }

    /// Returns true dependency map at `scope` from the nested lockfile tree.
    fn lockfile_dependencies_at_scope(
        &self,
        scope: &[DependencyName],
    ) -> Result<&DependencyMap, ResolverError> {
        let mut current = &self.lockfile().dependencies;
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

            // Enforce URL scheme and host policy before any network access.
            // Locked materialization reads commits straight from the
            // lockfile, so this is the sole policy gate for the fetch that
            // `materialize_git` performs below; neither
            // `plan_git_materialization` (locked mode) nor `materialize_git`
            // re-checks the URL.
            self.policy.check_git_url(&name, &git, dep_scope)?;

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
            leaves.push(CacheKey::from_git_url(&git, &sha).absolute_path(self.cache_root()));
        }
        leaves.sort();
        leaves.dedup();
        Ok(leaves)
    }

    /// Verifies every locked dependency reachable from `consumer` without
    /// fetching and returns the first failure encountered.
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

            let verified = match crate::resolver::verify::verify(&self.policy, &name, &module_root)
            {
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
    pub(super) fn validate_locked_local(
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

/// Reads and parses `module.json` from `dir`.
pub(super) fn read_manifest(dir: &Path) -> Result<Manifest, ResolverError> {
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
///
/// The returned [`RelativePath`] always uses `/` separators.
pub(super) fn resolve_normalized_subpath(
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
                // SAFETY: this branch runs only when `matches` has one item.
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
pub(super) fn exclude_set(
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

/// Returns true when a lockfile entry can satisfy the current Git
/// selector in `module.json`.
pub(super) fn locked_selector_satisfies(
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
