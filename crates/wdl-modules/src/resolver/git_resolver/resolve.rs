//! Fresh dependency resolution.
//!
//! Owns the recursive tree walk, Git selector resolution, remote version
//! discovery, default-branch discovery, and cycle detection that back the
//! [`Resolver::resolve_tree`] and [`Resolver::discover_versions`] entry
//! points. Every network operation is preceded by exactly one
//! [`ResolverPolicy::check_git_url`] check.
//!
//! [`Resolver::resolve_tree`]: crate::resolver::Resolver::resolve_tree
//! [`Resolver::discover_versions`]: crate::resolver::Resolver::discover_versions
//! [`ResolverPolicy::check_git_url`]: crate::resolver::ResolverPolicy::check_git_url

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use futures::future::BoxFuture;
use futures::future::FutureExt;
use path_clean::PathClean;
use semver::Version;

use super::GitResolver;
use super::materialize::MaterializedRoot;
use super::materialize::read_manifest;
use crate::dependency::DependencyName;
use crate::dependency::DependencySource;
use crate::dependency::GitModulePath;
use crate::dependency::GitSelector;
use crate::lockfile::DependencyEntry;
use crate::lockfile::GitCommit;
use crate::lockfile::ResolvedSource;
use crate::module::Module;
use crate::resolver::error::GitRefKind;
use crate::resolver::error::ResolverError;
use crate::resolver::scope::DependencyScope;
use crate::resolver::scope::ResolutionMode;
use crate::resolver::types::ResolvedDependency;
use crate::resolver::types::ResolvedTree;
use crate::resolver::verify::VerifiedModule;

impl GitResolver {
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
            // The closure performs pure libgit2 work and does
            // not panic; `JoinError` only occurs on runtime shutdown.
            .unwrap()
    }

    /// Resolves every transitive dependency declared by `consumer`.
    ///
    /// This is the body backing [`Resolver::resolve_tree`].
    ///
    /// [`Resolver::resolve_tree`]: crate::resolver::Resolver::resolve_tree
    pub(in crate::resolver) async fn resolve_fresh_tree(
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
    pub(in crate::resolver) async fn discover_matching_versions(
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
                self.policy.check_git_url(name, url, scope)?;
                let fetcher = self.fetcher();
                let url = url.clone();
                let path_prefix = path.as_ref().map(GitModulePath::as_str).map(str::to_string);
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
                // The spawned closure performs pure libgit2 work
                // and does not panic; a `JoinError` would only fire on
                // runtime shutdown, in which case re-panicking is fine.
                .unwrap()
            }
            DependencySource::LocalPath { .. } => Ok(Vec::new()),
        }
    }

    /// Recursively resolves a dependency map for `resolve_tree`.
    ///
    /// Each iteration: policy check, materialize, read manifest, cycle
    /// check, verify, recurse into transitive deps, assemble result.
    pub(in crate::resolver) fn resolve_dependencies<'a>(
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
    pub(in crate::resolver) async fn resolve_git_selector(
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
                let url = url.clone();
                let fetcher = self.fetcher();
                let refs = tokio::task::spawn_blocking(move || fetcher.list_tags(&url, scope))
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
                let url = url.clone();
                let fetcher = self.fetcher();
                let refs = tokio::task::spawn_blocking(move || fetcher.list_branches(&url, scope))
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
                let url = url.clone();
                let prefix = commit.as_str().to_string();
                let work_dir = self.commit_expand_dir(&url, &prefix);
                let _ = std::fs::remove_dir_all(&work_dir);
                let fetcher = self.fetcher();
                let expand_dir = work_dir.clone();
                let full = tokio::task::spawn_blocking(move || {
                    fetcher.resolve_commit_prefix(&url, &prefix, scope, &expand_dir)
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
}

/// Returns true when a lockfile entry can satisfy the current Git
/// selector in `module.json`.
pub(super) fn locked_selector_satisfies(
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
