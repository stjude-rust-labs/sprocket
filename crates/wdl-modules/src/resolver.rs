//! Public resolver API.
//!
//! Gated behind the `resolver` cargo feature. The resolver exposes
//! high-level resolution, materialization, lockfile, and trust types.
//! Git transport, cache layout, sparse checkout, and version-discovery
//! internals remain private so the implementation can change without
//! breaking consumers.

pub(crate) mod cache;
pub(crate) mod config;
pub(crate) mod error;
pub(crate) mod fetch;
mod git;
pub(crate) mod lock;
pub(crate) mod policy;
pub(crate) mod trust;
pub(crate) mod types;
pub(crate) mod verify;
pub(crate) mod versions;

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use bon::Builder;
use futures::future::BoxFuture;
use futures::future::FutureExt;
use semver::Version;

use crate::Lockfile;
use crate::Manifest;
use crate::dependency::DependencyName;
use crate::dependency::DependencySource;
use crate::dependency::GitModulePath;
use crate::dependency::GitSelector;
use crate::hash::NON_MODULE_CONTENT;
use crate::lockfile::DependencyEntry;
use crate::lockfile::GitCommit;
use crate::lockfile::ResolvedSource;
use crate::module_walk::ModuleWalkError;
use crate::relative_path::RelativePath;
use crate::resolver::cache::CacheKey;
pub use crate::resolver::config::LargeFileWarning;
pub use crate::resolver::config::LargeFileWarningError;
pub use crate::resolver::config::ModulesConfig;
pub use crate::resolver::config::TrustMode;
pub use crate::resolver::error::GitRefKind;
pub use crate::resolver::error::MissingFileKind;
pub use crate::resolver::error::ResolverError;
use crate::resolver::fetch::GitFetcher;
pub use crate::resolver::lock::DependencyChange;
pub use crate::resolver::lock::DependencyUpdate;
pub use crate::resolver::lock::LockfileDiff;
pub use crate::resolver::lock::NewSigner;
pub use crate::resolver::lock::RelockOutcome;
pub use crate::resolver::lock::RelockStats;
pub use crate::resolver::lock::partial_relock;
pub use crate::resolver::policy::ResolverPolicy;
pub use crate::resolver::trust::TrustEntry;
pub use crate::resolver::trust::TrustStore;
pub use crate::resolver::trust::TrustStoreError;
pub use crate::resolver::types::MaterializedFile;
pub use crate::resolver::types::ResolvedDependency;
pub use crate::resolver::types::ResolvedModule;
pub use crate::resolver::types::ResolvedTree;
use crate::resolver::verify::VerifiedModule;
use crate::symbolic_path::SymbolicPath;

/// Whether a dependency is declared directly by the consumer or
/// reached transitively through another dependency.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyScope {
    /// Declared in the consumer's own `module.json`.
    TopLevel,
    /// Reached through a transitive dependency chain.
    Transitive,
}

/// Whether to resolve mutable selectors against the remote or replay
/// a locked commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResolutionMode {
    /// Resolve mutable selectors against the remote. Used by
    /// `resolve_tree` when computing a fresh dependency graph.
    Fresh,
    /// Replay the locked commit from the lockfile. Used by
    /// `materialize` when reproducing a previously-locked dependency.
    Locked,
}

/// Resolves WDL module imports to concrete files on disk.
#[async_trait]
pub trait Resolver: Send + Sync {
    /// Materializes a single symbolic import on disk and returns the path
    /// to the resulting file.
    ///
    /// The primary call site for `wdl-analysis`. When the analyzer
    /// encounters a symbolic import like `import openwdl/csvkit/cut`, it
    /// asks the resolver for the file path that statement should route
    /// to, then parses the result with the existing import machinery as
    /// if the user had written `import "<that path>"`.
    ///
    /// - `consumer` is the manifest of the importing module.
    /// - `path` is the parsed symbolic path.
    ///
    /// The resolver looks up the head component in
    /// `consumer.dependencies`, materializes the dep's module folder if
    /// not yet cached, and resolves either the manifest's `entrypoint`
    /// (when the symbolic path has no sub-path) or `<sub-path>.wdl`
    /// under the module folder.
    async fn materialize(
        &self,
        consumer: &Manifest,
        path: &SymbolicPath,
    ) -> Result<MaterializedFile, ResolverError>;

    /// Resolves every transitive dependency declared by `consumer`.
    ///
    /// Walks the consumer's `dependencies` map, recurses into each dep's
    /// own manifest, and records every module visited along the way.
    ///
    /// The implementation of this method is expected to return an error upon
    /// the detection of a cycle.
    async fn resolve_tree(&self, consumer: &Manifest) -> Result<ResolvedTree, ResolverError>;

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
        GitFetcher::new(Arc::clone(&self.policy))
    }

    /// Returns the lockfile.
    pub fn lockfile(&self) -> &Lockfile {
        &self.lockfile
    }

    /// Checks that a locked local-path dep matches the manifest declaration.
    fn validate_locked_local(
        &self,
        name: &DependencyName,
        path: &Path,
    ) -> Result<(), ResolverError> {
        let locked_entry =
            self.lockfile
                .dependencies
                .get(name)
                .ok_or_else(|| ResolverError::NotInLockfile {
                    dep: name.manifest().to_string(),
                })?;
        if let ResolvedSource::Path { path: locked_path } = &locked_entry.source {
            if path != locked_path {
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
    /// On a failed clone, the partial cache leaf is removed so a corrupt
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

        result?;

        Ok(MaterializedRoot::Cached {
            module_root: plan.module_path.clone(),
            cache_leaf: plan.leaf.clone(),
        })
    }

    /// Recursively resolves a dependency map for `resolve_tree`.
    ///
    /// Each iteration: policy check, materialize, read manifest, cycle
    /// check, verify, recurse into transitive deps, assemble result.
    ///
    /// Returns a boxed future rather than an `async fn` because the method is
    /// recursive, and an `async fn` cannot name its own future type.
    fn resolve_dependencies<'a>(
        &'a self,
        deps: &'a BTreeMap<DependencyName, DependencySource>,
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
                let (resolved_source, manifest, module_root) = match source {
                    DependencySource::LocalPath { path, .. } => {
                        let manifest = read_manifest(path)?;
                        let resolved = ResolvedSource::Path { path: path.clone() };
                        let root = MaterializedRoot::Local(path.clone());
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
                                name, url, selector, path, scope,
                                ResolutionMode::Fresh,
                            )
                            .await?;
                        let root = self.materialize_git(name, url, scope, &plan).await?;
                        let manifest = read_manifest(&plan.module_path)?;
                        check_tag_manifest_match(
                            plan.path_prefix.as_deref(),
                            plan.selected_version.as_ref(),
                            &manifest.version,
                        )?;
                        let resolved = ResolvedSource::Git {
                            git: url.clone(),
                            commit: plan.commit,
                            path: path.clone(),
                            selector: selector.clone(),
                        };
                        (resolved, manifest, root)
                    }
                };

                // Detect cycles before recursing.
                if let Some(at) = chain.iter().position(|(_, s)| *s == resolved_source) {
                    let mut path: Vec<String> =
                        chain[at..].iter().map(|(n, _)| n.manifest().to_string()).collect();
                    path.push(name.manifest().to_string());
                    return Err(ResolverError::Cycle { path });
                }

                // Verify content hash, signature, and trust pin.
                let source_url = resolved_source.source_url();
                let source_path = resolved_source.source_path();
                let VerifiedModule { checksum, signer } =
                    crate::resolver::verify::verify(
                        &self.policy,
                        &self.trust,
                        name,
                        module_root.module_root(),
                        Some((&source_url, source_path)),
                    )
                    .inspect_err(|e| {
                        if let MaterializedRoot::Cached { cache_leaf, .. } = &module_root {
                            tracing::warn!(
                                dep = name.manifest(),
                                cache_leaf = %cache_leaf.display(),
                                error = %e,
                                "verification failed; run `sprocket module clean` to remove the cached module",
                            );
                        }
                    })?;

                // Recurse into transitive dependencies.
                chain.push((name.clone(), resolved_source.clone()));
                let inner = self
                    .resolve_dependencies(&manifest.dependencies, Some(&resolved_source), chain)
                    .await
                    .inspect_err(|_| {
                        chain.pop();
                    })?;
                chain.pop();

                out.insert(name.clone(), ResolvedDependency {
                    source: resolved_source,
                    version: manifest.version,
                    checksum,
                    signer,
                    dependencies: inner,
                });
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
    /// callers can verify it against the manifest.
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
                    },
                })?;
                Ok((Some(version), commit))
            }
            GitSelector::Tag(tag) => {
                let dep = name.clone();
                let url = url.clone();
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
            GitSelector::Commit(commit) => Ok((None, commit.clone())),
        }
    }

    /// Computes the materialization plan for a Git dependency.
    ///
    /// Resolves the commit (locked or fresh), derives cache paths from
    /// the URL and commit, and validates lockfile consistency when in
    /// locked mode. The returned plan carries everything
    /// [`materialize_dependency`](Self::materialize_dependency) needs
    /// to run the sparse checkout and verify the result.
    async fn plan_git_materialization(
        &self,
        name: &DependencyName,
        url: &url::Url,
        selector: &GitSelector,
        path: &Option<GitModulePath>,
        scope: DependencyScope,
        mode: ResolutionMode,
    ) -> Result<GitMaterializationPlan, ResolverError> {
        let path_prefix = path.as_ref().map(GitModulePath::as_str);

        let (selected_version, commit) = match mode {
            ResolutionMode::Locked => {
                let locked_entry = self.lockfile.dependencies.get(name).ok_or_else(|| {
                    ResolverError::NotInLockfile {
                        dep: name.manifest().to_string(),
                    }
                })?;
                let (locked_url, locked_commit, locked_path, locked_selector) =
                    match &locked_entry.source {
                        ResolvedSource::Git {
                            git: lu,
                            commit: lc,
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
                self.resolve_git_selector(name, url, selector, path_prefix, scope)
                    .await?
            }
        };

        let key = CacheKey::from_git_url(url, &commit);
        let leaf = key.absolute_path(&self.cache_root);
        let sparse_path = path_prefix.unwrap_or(".").to_string();
        let module_path = match path.as_ref() {
            Some(p) => leaf.join(p.as_path()),
            None => leaf.clone(),
        };

        Ok(GitMaterializationPlan {
            selected_version,
            commit,
            path_prefix: path_prefix.map(str::to_string),
            leaf,
            sparse_path,
            module_path,
        })
    }
}

#[async_trait]
impl Resolver for GitResolver {
    async fn materialize(
        &self,
        consumer: &Manifest,
        path: &SymbolicPath,
    ) -> Result<MaterializedFile, ResolverError> {
        // Look up the dependency declaration in the consumer's manifest.
        let name = path.dep_name();
        let scope = DependencyScope::TopLevel;
        let source =
            consumer
                .dependencies
                .get(name)
                .ok_or_else(|| ResolverError::NotADependency {
                    name: name.manifest().to_string(),
                })?;

        // Enforce URL scheme and host policy before any network access.
        if let DependencySource::Git { url, .. } = source {
            self.policy.check_git_url(name, url, scope)?;
        }

        // Materialize the dependency on disk and read its manifest.
        let (resolved_source, manifest, module_root) = match source {
            DependencySource::LocalPath { path, .. } => {
                self.validate_locked_local(name, path)?;
                let manifest = read_manifest(path)?;
                let resolved = ResolvedSource::Path { path: path.clone() };
                let root = MaterializedRoot::Local(path.clone());
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
                        ResolutionMode::Locked,
                    )
                    .await?;
                let root = self.materialize_git(name, url, scope, &plan).await?;
                let manifest = read_manifest(&plan.module_path)?;
                check_tag_manifest_match(
                    plan.path_prefix.as_deref(),
                    plan.selected_version.as_ref(),
                    &manifest.version,
                )?;
                let resolved = ResolvedSource::Git {
                    git: url.clone(),
                    commit: plan.commit,
                    path: path.clone(),
                    selector: selector.clone(),
                };
                (resolved, manifest, root)
            }
        };

        // Verify the content hash, signature, and trust pin.
        let root_path = module_root.module_root();
        let source_url = resolved_source.source_url();
        let source_path = resolved_source.source_path();
        let verified = crate::resolver::verify::verify(
            &self.policy,
            &self.trust,
            name,
            root_path,
            Some((&source_url, source_path)),
        )?;

        // Confirm the on-disk content matches the lockfile expectations.
        crate::resolver::verify::verify_against_lockfile(
            &self.lockfile,
            name,
            &verified.checksum,
            verified.signer.as_ref(),
        )?;

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
                let s = sub.display().to_string().replace('\\', "/");
                let wdl = format!("{s}.wdl");
                (
                    RelativePath::try_from(Path::new(&wdl))?,
                    MissingFileKind::SubPath,
                )
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
            source: resolved_source,
        })
    }

    async fn resolve_tree(&self, consumer: &Manifest) -> Result<ResolvedTree, ResolverError> {
        // Walk every transitive dependency starting from the consumer's
        // direct dependencies, collecting the full resolved tree.
        let mut chain: Vec<(DependencyName, ResolvedSource)> = Vec::new();
        let dependencies = self
            .resolve_dependencies(&consumer.dependencies, None, &mut chain)
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
            DependencySource::LocalPath { path, .. } => {
                // For local paths, read the manifest and return its
                // single declared version.
                let manifest_path = path.join(crate::MANIFEST_FILENAME);
                let bytes = std::fs::read(&manifest_path).map_err(|source| ResolverError::Io {
                    path: manifest_path.clone(),
                    source,
                })?;
                let manifest = Manifest::parse(&bytes)?;
                Ok(vec![manifest.version])
            }
        }
    }
}

/// Pre-computed materialization parameters for a Git dependency.
#[derive(Debug)]
struct GitMaterializationPlan {
    /// The selected version from tag resolution, if any.
    selected_version: Option<Version>,
    /// The resolved commit SHA.
    commit: GitCommit,
    /// The path prefix (from [`GitModulePath`]) for tag-version matching.
    path_prefix: Option<String>,
    /// The absolute path to the cache leaf directory.
    leaf: PathBuf,
    /// The sparse-checkout path (`path_prefix` or `"."`).
    sparse_path: String,
    /// The absolute path to the module root within the cache leaf.
    module_path: PathBuf,
}

/// Distinguishes resolver-owned cache paths from user-owned local
/// paths. Only `Cached` variants may be evicted.
#[derive(Clone, Debug)]
enum MaterializedRoot {
    /// A user's local module directory. Must never be evicted.
    Local(PathBuf),
    /// A resolver-owned cache leaf.
    Cached {
        /// The module content root inside the cache leaf.
        module_root: PathBuf,
        /// The resolver-owned cache leaf directory for this module.
        cache_leaf: PathBuf,
    },
}

impl MaterializedRoot {
    /// Returns the module root regardless of ownership.
    fn module_root(&self) -> &Path {
        match self {
            Self::Local(root) => root,
            Self::Cached { module_root, .. } => module_root,
        }
    }
}

/// Returns true when a lockfile entry can satisfy the current Git
/// selector in `module.json`.
fn locked_selector_satisfies(
    entry: &DependencyEntry,
    selector: &GitSelector,
    locked_commit: &GitCommit,
    locked_selector: &GitSelector,
) -> bool {
    match selector {
        GitSelector::Version(requirement) => requirement.matches(&entry.version),
        GitSelector::Commit(commit) => commit == locked_commit,
        GitSelector::Tag(tag) => {
            matches!(locked_selector, GitSelector::Tag(locked) if locked == tag)
        }
        GitSelector::Branch(branch) => {
            matches!(locked_selector, GitSelector::Branch(locked) if locked == branch)
        }
    }
}

/// Resolves a relative content path under `root`, enforcing the same
/// metadata exclusions and containment rules used by
/// [`module_walk`](crate::module_walk).
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
        return Err(ResolverError::Walk(
            ModuleWalkError::SymlinkTargetsMetadata(rel.to_string()),
        ));
    }

    let candidate = root.join(rel.as_path());
    if !candidate.exists() {
        return Err(ResolverError::Io {
            path: candidate,
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "materialized content file does not exist",
            ),
        });
    }

    let meta = std::fs::symlink_metadata(&candidate).map_err(|source| ResolverError::Io {
        path: candidate.clone(),
        source,
    })?;

    if meta.file_type().is_symlink() {
        let canonical_root = std::fs::canonicalize(root).map_err(|source| ResolverError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let target = std::fs::canonicalize(&candidate).map_err(|source| ResolverError::Io {
            path: candidate.clone(),
            source,
        })?;

        if !target.starts_with(&canonical_root) {
            return Err(ResolverError::MaterializedSymlinkEscape {
                dep: dep.manifest().to_string(),
                path: candidate,
            });
        }

        if let Ok(target_rel) = target.strip_prefix(&canonical_root) {
            if target_rel.to_str().is_none() {
                return Err(ResolverError::Walk(ModuleWalkError::NonUtf8SymlinkTarget(
                    candidate.display().to_string(),
                )));
            }
            // SAFETY: the `to_str` check above guarantees all
            // components are valid UTF-8.
            if target_rel
                .components()
                .any(|c| NON_MODULE_CONTENT.contains(&c.as_os_str().to_str().unwrap()))
            {
                return Err(ResolverError::Walk(
                    ModuleWalkError::SymlinkTargetsMetadata(rel.to_string()),
                ));
            }
        }

        Ok(target)
    } else {
        candidate
            .canonicalize()
            .map_err(|source| ResolverError::Io {
                path: candidate,
                source,
            })
    }
}

/// Reads and parses `module.json` from `dir`.
fn read_manifest(dir: &Path) -> Result<Manifest, ResolverError> {
    let path = dir.join(crate::MANIFEST_FILENAME);
    let bytes = std::fs::read(&path).map_err(|source| ResolverError::Io {
        path: path.clone(),
        source,
    })?;
    Manifest::parse(&bytes).map_err(ResolverError::from)
}

/// Returns `Err(TagManifestMismatch)` when a Git tag's selected
/// semver `expected` does not equal the manifest's `declared` version.
fn check_tag_manifest_match(
    path_prefix: Option<&str>,
    expected: Option<&semver::Version>,
    declared: &semver::Version,
) -> Result<(), ResolverError> {
    if let Some(exp) = expected
        && exp != declared
    {
        let tag = crate::resolver::versions::VersionTag::new(
            path_prefix.map(str::to_string),
            exp.clone(),
        )
        .to_string();
        return Err(ResolverError::TagManifestMismatch {
            tag,
            declared: declared.clone(),
        });
    }
    Ok(())
}

/// Compiles a manifest's `exclude` patterns into a [`globset::GlobSet`].
fn exclude_set(
    patterns: &[crate::relative_path::RelativePath],
) -> Result<globset::GlobSet, ResolverError> {
    if patterns.is_empty() {
        return Ok(globset::GlobSet::empty());
    }
    let mut builder = globset::GlobSetBuilder::new();
    for p in patterns {
        let s: &str = p.as_ref();
        let glob = globset::Glob::new(s).map_err(|source| ResolverError::InvalidExclude {
            pattern: s.to_string(),
            source,
        })?;
        builder.add(glob);
    }
    // SAFETY: `GlobSetBuilder::build` only consolidates already-compiled
    // globs; `Glob::new` above is the validating step, so by the time
    // we reach this call there is nothing left for `build` to reject.
    Ok(builder.build().unwrap())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;
    use tempfile::tempdir;

    use super::*;

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
    async fn resolve_and_lock(cache: &TempDir, consumer: &Manifest) -> (GitResolver, Lockfile) {
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
        consumer: &Manifest,
        policy: ResolverPolicy,
        trust: TrustStore,
    ) -> (GitResolver, Lockfile) {
        let r = resolver(cache);
        let tree = r.resolve_tree(consumer).await.unwrap();
        let outcome =
            crate::resolver::lock::partial_relock(consumer, &Lockfile::default(), &tree).unwrap();
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
        assert!(r.trust_store().entries.is_empty());
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
        let policy = ResolverPolicy::from(&ModulesConfig {
            allowed_transitive_hosts: vec!["gitlab.com".into()],
            ..ModulesConfig::default()
        });
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

        let cache = tempdir().unwrap();
        let tree = resolver(&cache).resolve_tree(&consumer).await.unwrap();
        let dep = tree
            .dependencies
            .get(&DependencyName::try_from("dep".to_string()).unwrap())
            .unwrap();
        assert!(matches!(&dep.source, ResolvedSource::Path { .. }));
        assert!(dep.dependencies.is_empty());
        assert_eq!(dep.version, Version::parse("1.0.0").unwrap());
    }

    fn hash_from_byte(byte: u8) -> crate::hash::ContentHash {
        format!("sha256:{}", hex::encode([byte; 32]))
            .parse()
            .unwrap()
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
    async fn materialize_rejects_lockfile_checksum_mismatch() {
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

        let cache = tempdir().unwrap();
        let (_, mut lockfile) = resolve_and_lock(&cache, &consumer).await;
        let dep_name = DependencyName::try_from("dep".to_string()).unwrap();
        lockfile.dependencies.get_mut(&dep_name).unwrap().checksum = hash_from_byte(42);
        let r = resolver_with_lockfile(&cache, lockfile);
        let err = r
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .unwrap_err();
        assert!(
            matches!(err, ResolverError::ChecksumMismatch { .. }),
            "expected `ChecksumMismatch`, got: {err}"
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

        let cache = tempdir().unwrap();
        let (r, _) = resolve_and_lock(&cache, &consumer).await;
        let mat = r
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .unwrap();
        assert_eq!(mat.path, dep_dir.join("index.wdl").canonicalize().unwrap());
    }

    #[tokio::test]
    async fn materialize_detects_content_drift_against_lockfile() {
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

        let cache = tempdir().unwrap();
        let (_, lockfile) = resolve_and_lock(&cache, &consumer).await;

        // Mutate the dep content after locking.
        fs::write(dep_dir.join("extra.wdl"), b"workflow extra {}").unwrap();

        let r = resolver_with_lockfile(&cache, lockfile);
        let err = r
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .unwrap_err();
        assert!(
            matches!(err, ResolverError::ChecksumMismatch { .. }),
            "expected `ChecksumMismatch` after content drift, got: {err}"
        );
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

    fn locked_git_resolver(cache: &TempDir, dep: &str, entry: DependencyEntry) -> GitResolver {
        let mut lockfile = Lockfile::default();
        lockfile.dependencies.insert(dep.parse().unwrap(), entry);
        resolver_with_lockfile(cache, lockfile)
    }

    fn locked_git_entry(selector: GitSelector) -> DependencyEntry {
        DependencyEntry {
            source: ResolvedSource::Git {
                git: "https://github.com/openwdl/tasks".parse().unwrap(),
                commit: "0000000000000000000000000000000000000001".parse().unwrap(),
                path: None,
                selector,
            },
            version: Version::parse("1.0.0").unwrap(),
            checksum: checksum(),
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
        let dep = DependencyName::try_from("dep".to_string()).unwrap();
        let url = "https://github.com/openwdl/tasks".parse().unwrap();
        let selector = GitSelector::Version("^2".parse().unwrap());
        let err = r
            .plan_git_materialization(
                &dep,
                &url,
                &selector,
                &None,
                DependencyScope::TopLevel,
                ResolutionMode::Locked,
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
        let dep = DependencyName::try_from("dep".to_string()).unwrap();
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
                ResolutionMode::Locked,
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
        let dep = DependencyName::try_from("dep".to_string()).unwrap();
        let url = "https://github.com/openwdl/tasks".parse().unwrap();
        let selector = GitSelector::Tag("v2.0.0".to_string());
        let err = r
            .plan_git_materialization(
                &dep,
                &url,
                &selector,
                &None,
                DependencyScope::TopLevel,
                ResolutionMode::Locked,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ResolverError::LockfileSourceMismatch { .. }));
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
            br#"{"name":"dep","version":"1.0.0","license":"MIT","entrypoint":"main.wdl"}"#,
        )
        .unwrap();
        fs::write(dep_dir.join("main.wdl"), b"workflow w {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();

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
        let body = r#"{"name":"dep","version":"1.0.0","license":"MIT","exclude":["internal/**"]}"#;
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
    async fn materialize_rejects_unsigned_when_require_signed() {
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

        // Lock with `require_signed` disabled so the unsigned dep can
        // be recorded in the lockfile. The replay below then enforces
        // `require_signed` and must reject the locked unsigned dep.
        let cache = tempdir().unwrap();
        let (_, lockfile) = resolve_and_lock(&cache, &consumer).await;

        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust(TrustStore::default())
            .policy(ResolverPolicy::from(&ModulesConfig {
                require_signed: true,
                ..ModulesConfig::default()
            }))
            .lockfile(lockfile)
            .build();
        let err = r
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .unwrap_err();
        assert!(
            matches!(err, ResolverError::RequireSignedViolation { .. }),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn materialize_rejects_tampered_signed_dependency() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("index.wdl"), b"workflow w {}").unwrap();
        let signer = crate::signing::test_utils::signing_key_from_seed(7);
        write_signature(&dep_dir, &signer);

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();

        let cache = tempdir().unwrap();
        let (_, lockfile) = resolve_and_lock(&cache, &consumer).await;

        // Tamper after locking.
        fs::write(dep_dir.join("extra.wdl"), b"workflow extra {}").unwrap();

        let r = resolver_with_lockfile(&cache, lockfile);
        let err = r
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .unwrap_err();
        // Tampered content changes the hash, which the signature or
        // lockfile checksum comparison catches.
        assert!(
            matches!(
                err,
                ResolverError::SignatureVerificationFailed { .. }
                    | ResolverError::ChecksumMismatch { .. }
            ),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn materialize_rejects_trust_pin_mismatch() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("index.wdl"), b"workflow w {}").unwrap();
        let signer = crate::signing::test_utils::signing_key_from_seed(7);
        write_signature(&dep_dir, &signer);

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();

        let pinned = crate::signing::test_utils::signing_key_from_seed(99).verifying_key();
        let trust = TrustStore {
            entries: vec![TrustEntry {
                dep: DependencyName::try_from("dep".to_string()).unwrap(),
                source: json_path(&dep_dir),
                path: None,
                key: pinned,
            }],
        };
        let cache = tempdir().unwrap();
        let (r, _) =
            resolve_and_lock_with_config(&cache, &consumer, ResolverPolicy::default(), trust).await;
        let err = r
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .unwrap_err();
        assert!(
            matches!(err, ResolverError::SignerKeyMismatch { .. }),
            "got: {err}"
        );
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
                ResolverError::MaterializedSymlinkEscape { .. }
                    | ResolverError::Walk(ModuleWalkError::SymlinkEscapesRoot(_))
                    | ResolverError::ChecksumMismatch { .. }
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

        let cache = tempdir().unwrap();
        let err = resolver(&cache)
            .materialize(&consumer, &"missing".parse().unwrap())
            .await
            .unwrap_err();
        assert!(matches!(err, ResolverError::NotADependency { .. }));
    }

    #[tokio::test]
    async fn signed_dependency_records_signer() {
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

        let cache = tempdir().unwrap();
        let tree = resolver(&cache).resolve_tree(&consumer).await.unwrap();
        let dep = tree
            .dependencies
            .get(&DependencyName::try_from("dep".to_string()).unwrap())
            .unwrap();
        assert_eq!(dep.signer.as_ref(), Some(&signer.verifying_key()));
    }

    #[tokio::test]
    async fn resolve_tree_rejects_unsigned_when_require_signed() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();

        let cache = tempdir().unwrap();
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust(TrustStore::default())
            .policy(ResolverPolicy::from(&ModulesConfig {
                require_signed: true,
                ..ModulesConfig::default()
            }))
            .lockfile(Lockfile::default())
            .build();
        let err = r.resolve_tree(&consumer).await.unwrap_err();
        assert!(
            matches!(err, ResolverError::RequireSignedViolation { .. }),
            "expected `RequireSignedViolation`, got: {err}"
        );
    }

    #[tokio::test]
    async fn resolve_tree_verifies_parent_before_transitive_dependencies() {
        let workdir = tempdir().unwrap();
        let child_dir = workdir.path().join("child");
        write_manifest(&child_dir, "child", "1.0.0", &[]);

        let parent_dir = workdir.path().join("parent");
        let child_src = format!("{{\"path\":\"{}\"}}", json_path(&child_dir));
        write_manifest(&parent_dir, "parent", "1.0.0", &[("child", &child_src)]);

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

        let cache = tempdir().unwrap();
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust(TrustStore::default())
            .policy(ResolverPolicy::from(&ModulesConfig {
                require_signed: true,
                ..ModulesConfig::default()
            }))
            .lockfile(Lockfile::default())
            .build();

        let err = r.resolve_tree(&consumer).await.unwrap_err();
        let ResolverError::RequireSignedViolation { dep } = err else {
            panic!("expected parent verification to run before transitive dependency traversal");
        };
        assert_eq!(dep, "parent");
    }

    #[tokio::test]
    async fn resolve_tree_rejects_tampered_signed_dependency() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        let signer = crate::signing::test_utils::signing_key_from_seed(7);
        write_signature(&dep_dir, &signer);
        // Modify a file after signing — this invalidates the signature.
        fs::write(dep_dir.join("extra.wdl"), b"workflow w {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();

        let cache = tempdir().unwrap();
        let err = resolver(&cache).resolve_tree(&consumer).await.unwrap_err();
        assert!(
            matches!(err, ResolverError::SignatureVerificationFailed { .. }),
            "expected `SignatureVerificationFailed`, got: {err}"
        );
    }

    #[tokio::test]
    async fn resolve_tree_rejects_trust_pin_mismatch() {
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

        let pinned = crate::signing::test_utils::signing_key_from_seed(99).verifying_key();
        let trust = TrustStore {
            entries: vec![TrustEntry {
                dep: DependencyName::try_from("dep".to_string()).unwrap(),
                source: json_path(&dep_dir),
                path: None,
                key: pinned,
            }],
        };
        let cache = tempdir().unwrap();
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust(trust)
            .lockfile(Lockfile::default())
            .build();
        let err = r.resolve_tree(&consumer).await.unwrap_err();
        assert!(
            matches!(err, ResolverError::SignerKeyMismatch { .. }),
            "expected `SignerKeyMismatch`, got: {err}"
        );
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

        let cache = tempdir().unwrap();
        let err = resolver(&cache).resolve_tree(&consumer).await.unwrap_err();
        assert!(
            matches!(
                err,
                ResolverError::Walk(ModuleWalkError::SymlinkEscapesRoot(_))
            ),
            "expected `Hash(SymlinkEscapesRoot)`, got: {err}"
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

        let cache = tempdir().unwrap();
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust(TrustStore::default())
            .lockfile(Lockfile::default())
            .policy(ResolverPolicy::from(&ModulesConfig {
                max_materialized_files: Some(1),
                ..ModulesConfig::default()
            }))
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

        let cache = tempdir().unwrap();
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust(TrustStore::default())
            .lockfile(Lockfile::default())
            .policy(ResolverPolicy::from(&ModulesConfig {
                max_materialized_bytes: Some(100),
                ..ModulesConfig::default()
            }))
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

        let cache = tempdir().unwrap();
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust(TrustStore::default())
            .lockfile(Lockfile::default())
            .policy(ResolverPolicy::from(&ModulesConfig {
                max_materialized_files: Some(1),
                ..ModulesConfig::default()
            }))
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
    async fn local_path_relock_refreshes_on_content_change() {
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

        let cache = tempdir().unwrap();
        let (_, lockfile_v1) = resolve_and_lock(&cache, &consumer).await;
        let v1_checksum = lockfile_v1
            .dependencies
            .get(&DependencyName::try_from("dep".to_string()).unwrap())
            .unwrap()
            .checksum;

        fs::write(dep_dir.join("index.wdl"), b"workflow changed {}").unwrap();

        let (_, lockfile_v2) = resolve_and_lock(&cache, &consumer).await;
        let v2_checksum = lockfile_v2
            .dependencies
            .get(&DependencyName::try_from("dep".to_string()).unwrap())
            .unwrap()
            .checksum;

        assert_ne!(
            v1_checksum, v2_checksum,
            "local path relock must produce a new checksum when content changes"
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

        let cache = tempdir().unwrap();
        let err = resolver(&cache).resolve_tree(&consumer).await.unwrap_err();
        let ResolverError::Cycle { path } = err else {
            panic!("expected `Cycle`, got: {err}");
        };
        assert_eq!(path.len(), 2, "self-loop should report a 2-element chain");
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
                ResolverError::Walk(ModuleWalkError::SymlinkTargetsMetadata(_))
            ),
            "expected `SymlinkTargetsMetadata`, got: {err}"
        );
    }

    #[tokio::test]
    async fn materialize_rejects_signature_downgrade() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("index.wdl"), b"workflow w {}").unwrap();
        let signer = crate::signing::test_utils::signing_key_from_seed(7);
        write_signature(&dep_dir, &signer);

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();

        let cache = tempdir().unwrap();
        let (_, lockfile) = resolve_and_lock(&cache, &consumer).await;
        let dep_name = DependencyName::try_from("dep".to_string()).unwrap();
        assert!(
            lockfile
                .dependencies
                .get(&dep_name)
                .unwrap()
                .signer
                .is_some(),
            "lockfile should record the signer"
        );

        fs::remove_file(dep_dir.join(crate::SIGNATURE_FILENAME)).unwrap();

        let r = resolver_with_lockfile(&cache, lockfile);
        let err = r
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .unwrap_err();
        assert!(
            matches!(err, ResolverError::SignatureDowngrade { .. }),
            "expected `SignatureDowngrade`, got: {err}"
        );
    }

    #[tokio::test]
    async fn materialize_rejects_signer_key_change() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("index.wdl"), b"workflow w {}").unwrap();
        let signer_a = crate::signing::test_utils::signing_key_from_seed(7);
        write_signature(&dep_dir, &signer_a);

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", json_path(&dep_dir));
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();

        let cache = tempdir().unwrap();
        let (_, lockfile) = resolve_and_lock(&cache, &consumer).await;

        let signer_b = crate::signing::test_utils::signing_key_from_seed(99);
        write_signature(&dep_dir, &signer_b);

        let r = resolver_with_lockfile(&cache, lockfile);
        let err = r
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .unwrap_err();
        assert!(
            matches!(err, ResolverError::SignerKeyMismatch { .. }),
            "expected `SignerKeyMismatch`, got: {err}"
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

        let cache = tempdir().unwrap();
        let (r, _) = resolve_and_lock(&cache, &consumer).await;
        let mat = r
            .materialize(&consumer, &"dep".parse().unwrap())
            .await
            .unwrap();
        assert!(mat.path.exists());
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
            .policy(ResolverPolicy::from(&ModulesConfig {
                allowed_schemes: vec!["https".into(), "ssh".into(), "file".into()],
                ..ModulesConfig::default()
            }))
            .build();
        let dep = DependencyName::try_from("tasks".to_string()).unwrap();
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

    #[test]
    fn tag_manifest_mismatch_errors_on_disagreement() {
        let v_expected = semver::Version::parse("2.0.0").unwrap();
        let v_declared = semver::Version::parse("1.0.0").unwrap();
        let err = check_tag_manifest_match(None, Some(&v_expected), &v_declared).unwrap_err();
        let ResolverError::TagManifestMismatch { tag, declared } = err else {
            panic!("got: {err:?}");
        };
        assert_eq!(tag, "v2.0.0");
        assert_eq!(declared, v_declared);
    }

    #[test]
    fn check_tag_manifest_match_succeeds_when_versions_agree() {
        let v = semver::Version::parse("1.2.3").unwrap();
        check_tag_manifest_match(Some("csvkit"), Some(&v), &v).unwrap();
    }

    #[test]
    fn check_tag_manifest_match_succeeds_when_no_expected_version() {
        check_tag_manifest_match(None, None, &semver::Version::parse("0.0.1").unwrap()).unwrap();
    }
}
