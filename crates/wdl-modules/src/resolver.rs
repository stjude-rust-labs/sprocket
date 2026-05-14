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
pub(crate) mod helpers;
pub(crate) mod lock;
pub(crate) mod module_root;
pub(crate) mod policy;
pub(crate) mod scope;
pub(crate) mod tree_walk;
pub(crate) mod trust;
pub(crate) mod types;
pub(crate) mod verify;
pub(crate) mod versions;

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use async_trait::async_trait;
use bon::Builder;
use futures::future::BoxFuture;
use futures::future::FutureExt;
use semver::Version;

use crate::DependencyEntry;
use crate::DependencyName;
use crate::DependencySource;
use crate::GitCommit;
use crate::GitModulePath;
use crate::GitSelector;
use crate::Lockfile;
use crate::Manifest;
use crate::ResolvedSource;
use crate::SymbolicPath;
use crate::resolver::cache::CacheKey;
pub use crate::resolver::config::LargeFileWarning;
pub use crate::resolver::config::LargeFileWarningError;
pub use crate::resolver::config::ModulesConfig;
pub use crate::resolver::config::TrustMode;
pub use crate::resolver::error::GitRefKind;
pub use crate::resolver::error::MissingFileKind;
pub use crate::resolver::error::ResolverError;
use crate::resolver::fetch::GitFetcher;
use crate::resolver::helpers::check_tag_manifest_match;
use crate::resolver::helpers::exclude_set;
use crate::resolver::helpers::is_transitive_local_disallowed;
use crate::resolver::helpers::read_manifest;
pub use crate::resolver::lock::DependencyAddition;
pub use crate::resolver::lock::DependencyUpdate;
pub use crate::resolver::lock::LockfileDiff;
pub use crate::resolver::lock::NewSigner;
pub use crate::resolver::lock::RelockOutcome;
pub use crate::resolver::lock::RelockStats;
pub use crate::resolver::lock::partial_relock;
use crate::resolver::module_root::MaterializedRoot;
use crate::resolver::module_root::ModuleRoot;
use crate::resolver::module_root::resolve_content_file;
use crate::resolver::policy::ResolverPolicy;
pub use crate::resolver::scope::DependencyScope;
use crate::resolver::scope::ResolutionMode;
pub use crate::resolver::trust::TrustEntry;
pub use crate::resolver::trust::TrustStore;
pub use crate::resolver::trust::TrustStoreError;
pub use crate::resolver::types::MaterializedFile;
pub use crate::resolver::types::ResolvedDependency;
pub use crate::resolver::types::ResolvedModule;
pub use crate::resolver::types::ResolvedTree;
use crate::resolver::verify::ModuleVerifier;
use crate::resolver::verify::VerifiedModule;

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
    /// Detects cycles.
    async fn resolve_tree(&self, consumer: &Manifest) -> Result<ResolvedTree, ResolverError>;

    /// Lists discovered versions for a dependency source that satisfy
    /// the requirement, in descending semver order.
    ///
    /// Used by CLI commands that surface available versions to the user
    /// and internally by `resolve_tree` to select the version a Git dep
    /// resolves to.
    async fn discover_versions(
        &self,
        source: &DependencySource,
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
    /// Path of the user-level trust store (`modules-trust.toml`),
    /// recorded for diagnostic output. Loading is the caller's
    /// responsibility; `trust` carries the loaded contents.
    #[builder(into)]
    trust_path: PathBuf,
    /// The project's `[modules]` configuration.
    #[builder(default)]
    config: ModulesConfig,
    /// The user-level trust store, loaded by the caller.
    trust: TrustStore,
    /// The lockfile to verify materialized dependencies against.
    /// `materialize` compares each dependency's observed content hash
    /// against the locked checksum and rejects mismatches.
    lockfile: Lockfile,
}

impl GitResolver {
    /// Returns the cache root.
    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    /// Returns the trust-store path.
    pub fn trust_path(&self) -> &Path {
        &self.trust_path
    }

    /// Returns the active `[modules]` configuration.
    pub fn config(&self) -> &ModulesConfig {
        &self.config
    }

    /// Returns the active trust store.
    pub fn trust_store(&self) -> &TrustStore {
        &self.trust
    }

    /// Returns the resolved policy.
    fn policy(&self) -> ResolverPolicy {
        ResolverPolicy::from(&self.config)
    }

    /// Returns a policy-enforcing Git fetcher.
    fn fetcher(&self) -> GitFetcher {
        GitFetcher::new(self.policy())
    }

    /// Returns the lockfile.
    pub fn lockfile(&self) -> &Lockfile {
        &self.lockfile
    }

    /// Resolves every entry in `deps`, threading the cycle-detection
    /// `chain` of `(name, source)` pairs through each recursion step.
    fn resolve_deps<'a>(
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
                if is_transitive_local_disallowed(parent, source) {
                    return Err(ResolverError::LocalPathInTransitive {
                        dep: name.manifest().to_string(),
                    });
                }
                if let DependencySource::Git { url, .. } = source {
                    self.policy().check_git_url(name, url, scope)?;
                }
                let resolved = self.resolve_dependency(name, source, scope, chain).await?;
                out.insert(name.clone(), resolved);
            }
            Ok(out)
        }
        .boxed()
    }

    /// Resolves a single dependency, recursing into its own
    /// `dependencies`. Detects cycles using `chain`.
    fn resolve_dependency<'a>(
        &'a self,
        name: &'a DependencyName,
        source: &'a DependencySource,
        scope: DependencyScope,
        chain: &'a mut Vec<(DependencyName, ResolvedSource)>,
    ) -> BoxFuture<'a, Result<ResolvedDependency, ResolverError>> {
        async move {
            let (resolved_source, manifest, module_root) = self
                .materialize_dependency(name, source, scope, ResolutionMode::Fresh)
                .await?;

            if let Some(at) = chain.iter().position(|(_, s)| *s == resolved_source) {
                let mut path: Vec<String> =
                    chain[at..].iter().map(|(n, _)| n.manifest().to_string()).collect();
                path.push(name.manifest().to_string());
                return Err(ResolverError::Cycle { path });
            }

            let VerifiedModule { checksum, signer } = self
                .verify(name, module_root.module_root().as_ref())
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

            chain.push((name.clone(), resolved_source.clone()));
            let inner = self
                .resolve_deps(&manifest.dependencies, Some(&resolved_source), chain)
                .await
                .inspect_err(|_| {
                    chain.pop();
                })?;
            chain.pop();

            Ok(ResolvedDependency {
                source: resolved_source,
                version: manifest.version,
                checksum,
                signer,
                dependencies: inner,
            })
        }
        .boxed()
    }

    /// Returns a verifier that borrows this resolver's config, trust
    /// store, and lockfile.
    fn verify(
        &self,
        name: &DependencyName,
        module_root: &Path,
    ) -> Result<VerifiedModule, ResolverError> {
        let policy = self.policy();
        let verifier = ModuleVerifier::builder()
            .config(&self.config)
            .policy(&policy)
            .trust(&self.trust)
            .lockfile(&self.lockfile)
            .build();
        verifier.verify(name, module_root)
    }

    /// Verifies a dependency's content hash and signer against the
    /// lockfile.
    fn verify_against_lockfile(
        &self,
        name: &DependencyName,
        checksum: &crate::ContentHash,
        signer: Option<&crate::VerifyingKey>,
    ) -> Result<(), ResolverError> {
        let policy = self.policy();
        let verifier = ModuleVerifier::builder()
            .config(&self.config)
            .policy(&policy)
            .trust(&self.trust)
            .lockfile(&self.lockfile)
            .build();
        verifier.verify_against_lockfile(name, checksum, signer)
    }

    /// Resolves a [`GitSelector`] against the remote at `url` to a
    /// concrete commit SHA.
    async fn resolve_git_selector(
        &self,
        name: &DependencyName,
        url: &url::Url,
        selector: &GitSelector,
        path_prefix: Option<&str>,
        scope: DependencyScope,
    ) -> Result<(Option<Version>, crate::GitCommit), ResolverError> {
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
            GitSelector::Commit(commit) => Ok((None, commit.clone())),
        }
    }

    /// Materializes a dependency on disk and parses its manifest.
    /// Returns the resolved source, the parsed manifest, and the
    /// absolute path to the directory containing `module.json`.
    async fn materialize_dependency(
        &self,
        name: &DependencyName,
        source: &DependencySource,
        scope: DependencyScope,
        mode: ResolutionMode,
    ) -> Result<(ResolvedSource, Manifest, MaterializedRoot), ResolverError> {
        match source {
            DependencySource::LocalPath { path, .. } => {
                if matches!(mode, ResolutionMode::Locked) {
                    let locked_entry = self.lockfile.dependencies.get(name).ok_or_else(|| {
                        ResolverError::NotInLockfile {
                            dep: name.manifest().to_string(),
                        }
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
                }
                let manifest = read_manifest(path)?;
                Ok((
                    ResolvedSource::Path { path: path.clone() },
                    manifest,
                    MaterializedRoot::Local(ModuleRoot::new(path.clone())),
                ))
            }
            DependencySource::Git {
                url,
                selector,
                path,
                ..
            } => {
                let plan = self
                    .plan_git_materialization(name, url, selector, path, scope, mode)
                    .await?;

                let fetcher = self.fetcher();
                let dep_for_clone = name.clone();
                let url_for_clone = url.clone();
                let leaf_for_clone = plan.leaf.clone();
                let commit_for_clone = plan.commit.clone();
                let sparse_path = plan.sparse_path.clone();
                let materialize_result = tokio::task::spawn_blocking(move || {
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
                .unwrap();

                if let Err(err) = materialize_result {
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

                let manifest = read_manifest(&plan.module_path)?;
                check_tag_manifest_match(
                    plan.path_prefix.as_deref(),
                    plan.selected_version.as_ref(),
                    &manifest.version,
                )?;
                Ok((
                    ResolvedSource::Git {
                        git: url.clone(),
                        commit: plan.commit,
                        path: path.clone(),
                        selector: Some(selector.clone()),
                    },
                    manifest,
                    MaterializedRoot::Cached {
                        module_root: ModuleRoot::new(plan.module_path),
                        cache_leaf: plan.leaf,
                    },
                ))
            }
        }
    }

    /// Computes the materialization plan for a Git dependency: resolves
    /// the commit (locked or fresh), derives cache paths, and validates
    /// lockfile consistency when in locked mode.
    async fn plan_git_materialization(
        &self,
        name: &DependencyName,
        url: &url::Url,
        selector: &GitSelector,
        path: &Option<GitModulePath>,
        scope: DependencyScope,
        mode: ResolutionMode,
    ) -> Result<GitMaterializationPlan, ResolverError> {
        let path_prefix = path.as_ref().map(GitModulePath::as_str).map(str::to_string);

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
                        locked_selector.as_ref(),
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

        Ok(GitMaterializationPlan {
            selected_version,
            commit,
            path_prefix,
            leaf,
            sparse_path,
            module_path,
        })
    }
}

/// Returns true when a lockfile entry can satisfy the current Git
/// selector in `module.json`.
fn locked_selector_satisfies(
    entry: &DependencyEntry,
    selector: &GitSelector,
    locked_commit: &GitCommit,
    locked_selector: Option<&GitSelector>,
) -> bool {
    match selector {
        GitSelector::Version(requirement) => requirement.matches(&entry.version),
        GitSelector::Commit(commit) => commit == locked_commit,
        GitSelector::Tag(tag) => {
            matches!(locked_selector, Some(GitSelector::Tag(locked)) if locked == tag)
        }
        GitSelector::Branch(branch) => {
            matches!(locked_selector, Some(GitSelector::Branch(locked)) if locked == branch)
        }
    }
}

/// Pre-computed materialization parameters for a Git dependency.
#[derive(Debug)]
struct GitMaterializationPlan {
    /// The selected version from tag resolution, if any.
    selected_version: Option<Version>,
    /// The resolved commit SHA.
    commit: crate::GitCommit,
    /// The path prefix (from [`GitModulePath`]) for tag-version matching.
    path_prefix: Option<String>,
    /// The absolute path to the cache leaf directory.
    leaf: PathBuf,
    /// The sparse-checkout path (`path_prefix` or `"."`).
    sparse_path: String,
    /// The absolute path to the module root within the cache leaf.
    module_path: PathBuf,
}

/// Compiles a manifest's `exclude` patterns into a [`globset::GlobSet`]
/// for gitignore-style matching against import sub-paths.
#[async_trait]
impl Resolver for GitResolver {
    async fn materialize(
        &self,
        consumer: &Manifest,
        path: &SymbolicPath,
    ) -> Result<MaterializedFile, ResolverError> {
        let name = path.dep_name();
        let source =
            consumer
                .dependencies
                .get(name)
                .ok_or_else(|| ResolverError::NotADependency {
                    name: name.manifest().to_string(),
                })?;

        // Materialization through the trait is always top-level (not
        // transitive); the analyzer materializes the consumer's direct
        // deps, and transitive resolution goes through `resolve_tree`.
        let scope = DependencyScope::TopLevel;
        if let DependencySource::Git { url, .. } = source {
            self.policy().check_git_url(name, url, scope)?;
        }
        let (resolved_source, manifest, module_root) = self
            .materialize_dependency(name, source, scope, ResolutionMode::Locked)
            .await?;

        let root_path = module_root.module_root().as_ref();
        let verified = self.verify(name, root_path)?;
        self.verify_against_lockfile(name, &verified.checksum, verified.signer.as_ref())?;

        let (rel, kind) = match path.sub_path() {
            None => (
                manifest.entrypoint_filename().to_path_buf(),
                MissingFileKind::Entrypoint,
            ),
            Some(sub) => {
                let mut p = sub.to_path_buf();
                p.set_extension("wdl");
                (p, MissingFileKind::SubPath)
            }
        };

        if exclude_set(&manifest.exclude)?.is_match(&rel) {
            return Err(ResolverError::MissingFile {
                dep: name.manifest().to_string(),
                path: rel,
                kind: MissingFileKind::Excluded,
            });
        }

        let canonical =
            resolve_content_file(module_root.module_root(), &rel, name).map_err(|e| match e {
                ResolverError::Io { source, .. }
                    if source.kind() == std::io::ErrorKind::NotFound =>
                {
                    ResolverError::MissingFile {
                        dep: name.manifest().to_string(),
                        path: rel.clone(),
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
        let mut chain: Vec<(DependencyName, ResolvedSource)> = Vec::new();
        let dependencies = self
            .resolve_deps(&consumer.dependencies, None, &mut chain)
            .await?;
        Ok(ResolvedTree { dependencies })
    }

    async fn discover_versions(
        &self,
        source: &DependencySource,
    ) -> Result<Vec<Version>, ResolverError> {
        match source {
            DependencySource::Git {
                url,
                selector,
                path,
                ..
            } => {
                let scope = DependencyScope::TopLevel;
                let GitSelector::Version(requirement) = selector else {
                    return Ok(Vec::new());
                };
                let fetcher = self.fetcher();
                let dep = DependencyName::try_from("discovery".to_string()).unwrap();
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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;
    use tempfile::tempdir;

    use super::*;

    fn checksum() -> crate::ContentHash {
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
            .trust_path(cache.path().join("trust.toml"))
            .trust(TrustStore::default())
            .lockfile(lockfile)
            .build()
    }

    /// Resolves a consumer's tree and builds a lockfile from it.
    async fn resolve_and_lock(cache: &TempDir, consumer: &Manifest) -> (GitResolver, Lockfile) {
        resolve_and_lock_with_config(
            cache,
            consumer,
            ModulesConfig::default(),
            TrustStore::default(),
        )
        .await
    }

    async fn resolve_and_lock_with_config(
        cache: &TempDir,
        consumer: &Manifest,
        config: ModulesConfig,
        trust: TrustStore,
    ) -> (GitResolver, Lockfile) {
        let r = resolver(cache);
        let tree = r.resolve_tree(consumer).await.unwrap();
        let outcome =
            crate::resolver::lock::partial_relock(consumer, &Lockfile::default(), &tree).unwrap();
        let locked = GitResolver::builder()
            .cache_root(cache.path())
            .trust_path(cache.path().join("trust.toml"))
            .trust(trust)
            .lockfile(outcome.lockfile.clone())
            .config(config)
            .build();
        (locked, outcome.lockfile)
    }

    /// Writes a `module.sig` next to `dir`'s `module.json` over the
    /// directory's content hash.
    fn write_signature(dir: &Path, signer: &crate::SigningKey) {
        let digest = crate::hash::hash_directory(dir).unwrap();
        let signature = signer.sign(&digest);
        let sig = crate::ModuleSignature {
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
        let trust_path = tempdir().unwrap().path().join("trust.toml");
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust_path(&trust_path)
            .trust(TrustStore::default())
            .lockfile(Lockfile::default())
            .build();
        assert_eq!(r.cache_root(), cache.path());
        assert_eq!(r.trust_path(), trust_path);
        assert!(r.trust_store().entries.is_empty());
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

    fn hash_from_byte(byte: u8) -> crate::ContentHash {
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
            .materialize(&consumer, &"dep".to_string().try_into().unwrap())
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
            .materialize(&consumer, &"dep".to_string().try_into().unwrap())
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
            .materialize(&consumer, &"dep".to_string().try_into().unwrap())
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
            .materialize(&consumer, &"dep".to_string().try_into().unwrap())
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
            .materialize(&consumer2, &"dep".to_string().try_into().unwrap())
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

    fn locked_git_entry(selector: Option<GitSelector>) -> DependencyEntry {
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
        let r = locked_git_resolver(&cache, "dep", locked_git_entry(None));
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
        let r = locked_git_resolver(&cache, "dep", locked_git_entry(None));
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
            locked_git_entry(Some(GitSelector::Tag("v1.0.0".to_string()))),
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
            .materialize(&consumer, &"dep".to_string().try_into().unwrap())
            .await
            .unwrap();
        assert_eq!(mat.path, dep_dir.join("index.wdl").canonicalize().unwrap());
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
            .materialize(&consumer, &"dep/cut".to_string().try_into().unwrap())
            .await
            .unwrap();
        assert_eq!(mat.path, dep_dir.join("cut.wdl").canonicalize().unwrap());
    }

    #[tokio::test]
    async fn invalid_commit_selector_rejected_at_parse_time() {
        let workdir = tempdir().unwrap();
        let consumer_dir = workdir.path().join("consumer");
        let bad_src = "{\"git\":\"https://example.com/repo.git\",\"commit\":\"not-a-sha\"}";
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", bad_src)]);
        let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap();
        let err = Manifest::parse(&bytes).unwrap_err();
        assert!(
            matches!(err, crate::ManifestError::InvalidJson(_)),
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
            .materialize(
                &consumer,
                &"dep/internal/private".to_string().try_into().unwrap(),
            )
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

        let cache = tempdir().unwrap();
        let (r, _) = resolve_and_lock_with_config(
            &cache,
            &consumer,
            ModulesConfig {
                require_signed: true,
                ..ModulesConfig::default()
            },
            TrustStore::default(),
        )
        .await;
        let err = r
            .materialize(&consumer, &"dep".to_string().try_into().unwrap())
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
            .materialize(&consumer, &"dep".to_string().try_into().unwrap())
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
                key: pinned,
            }],
        };
        let cache = tempdir().unwrap();
        let (r, _) =
            resolve_and_lock_with_config(&cache, &consumer, ModulesConfig::default(), trust).await;
        let err = r
            .materialize(&consumer, &"dep".to_string().try_into().unwrap())
            .await
            .unwrap_err();
        assert!(
            matches!(err, ResolverError::SignerKeyMismatch { .. }),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn materialize_rejects_symlinked_entrypoint_outside_root() {
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
            .materialize(&consumer, &"dep".to_string().try_into().unwrap())
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                ResolverError::MaterializedSymlinkEscape { .. }
                    | ResolverError::Hash(crate::HashError::SymlinkEscapesRoot(_))
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
            .materialize(&consumer, &"missing".to_string().try_into().unwrap())
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
    async fn require_signed_rejects_unsigned_dependency() {
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
            .trust_path(cache.path().join("trust.toml"))
            .trust(TrustStore::default())
            .config(ModulesConfig {
                require_signed: true,
                ..ModulesConfig::default()
            })
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
            .trust_path(cache.path().join("trust.toml"))
            .trust(TrustStore::default())
            .config(ModulesConfig {
                require_signed: true,
                ..ModulesConfig::default()
            })
            .lockfile(Lockfile::default())
            .build();

        let err = r.resolve_tree(&consumer).await.unwrap_err();
        let ResolverError::RequireSignedViolation { dep } = err else {
            panic!("expected parent verification to run before transitive dependency traversal");
        };
        assert_eq!(dep, "parent");
    }

    #[tokio::test]
    async fn tampered_content_fails_signature_verification() {
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
    async fn trust_pin_mismatch_errors() {
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
                key: pinned,
            }],
        };
        let cache = tempdir().unwrap();
        let r = GitResolver::builder()
            .cache_root(cache.path())
            .trust_path(cache.path().join("trust.toml"))
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
                ResolverError::Hash(crate::HashError::SymlinkEscapesRoot(_))
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
            .trust_path(cache.path().join("trust.toml"))
            .trust(TrustStore::default())
            .lockfile(Lockfile::default())
            .config(ModulesConfig {
                max_materialized_files: Some(1),
                ..ModulesConfig::default()
            })
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
            .trust_path(cache.path().join("trust.toml"))
            .trust(TrustStore::default())
            .lockfile(Lockfile::default())
            .config(ModulesConfig {
                max_materialized_bytes: Some(100),
                ..ModulesConfig::default()
            })
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
            .trust_path(cache.path().join("trust.toml"))
            .trust(TrustStore::default())
            .lockfile(Lockfile::default())
            .config(ModulesConfig {
                max_materialized_files: Some(1),
                ..ModulesConfig::default()
            })
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
            .materialize(&consumer, &"dep".to_string().try_into().unwrap())
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                ResolverError::Hash(crate::HashError::SymlinkTargetsMetadata(_))
                    | ResolverError::MaterializedSymlinkEscape { .. }
                    | ResolverError::ChecksumMismatch { .. }
            ),
            "symlink to nested metadata must be rejected, got: {err}"
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
        let (_, mut lockfile) = resolve_and_lock(&cache, &consumer).await;
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
            .materialize(&consumer, &"dep".to_string().try_into().unwrap())
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
            .materialize(&consumer, &"dep".to_string().try_into().unwrap())
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
            .materialize(&consumer, &"dep".to_string().try_into().unwrap())
            .await
            .unwrap();
        assert!(mat.path.exists());
    }

    #[tokio::test]
    async fn discover_versions_does_not_panic() {
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
        repo.tag_lightweight("v1.0.0", &repo.find_object(oid.into(), None).unwrap(), false)
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
            .trust_path(cache.path().join("trust.toml"))
            .trust(TrustStore::default())
            .lockfile(Lockfile::default())
            .config(ModulesConfig {
                allowed_schemes: vec!["https".into(), "ssh".into(), "file".into()],
                ..ModulesConfig::default()
            })
            .build();
        let versions = r.discover_versions(&source).await.unwrap();
        assert_eq!(
            versions,
            vec![semver::Version::parse("1.0.0").unwrap()],
            "should discover `v1.0.0` tag"
        );
    }
}
