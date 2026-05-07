//! Resolver layer.
//!
//! Gated behind the `resolver` cargo feature. Pulls in `git2`, `tokio`,
//! `dirs`, `bytesize`, `toml`, and `tracing`. Consumers that only need
//! the manifest/lockfile/hashing types (e.g. `wdl-doc`) do not enable
//! this feature and therefore do not pay for those deps.

pub(crate) mod cache;
pub mod config;
pub mod error;
mod git;
pub mod lock;
pub mod trust;
pub(crate) mod types;
pub(crate) mod versions;

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use async_trait::async_trait;
use bon::Builder;
use futures::future::BoxFuture;
use futures::future::FutureExt;
use semver::Version;

use crate::DependencyName;
use crate::DependencySource;
use crate::GitModulePath;
use crate::GitSelector;
use crate::Manifest;
use crate::ModulePath;
use crate::ResolvedSource;
use crate::SymbolicPath;
pub use crate::resolver::cache::CacheKey;
pub use crate::resolver::config::LargeFileWarning;
pub use crate::resolver::config::LargeFileWarningError;
pub use crate::resolver::config::ModulesConfig;
pub use crate::resolver::config::TrustMode;
pub use crate::resolver::error::GitRefKind;
pub use crate::resolver::error::MissingFileKind;
pub use crate::resolver::error::ResolverError;
pub use crate::resolver::git::GitError;
pub use crate::resolver::lock::DependencyAddition;
pub use crate::resolver::lock::DependencyUpdate;
pub use crate::resolver::lock::LockfileDiff;
pub use crate::resolver::lock::NewSigner;
pub use crate::resolver::lock::RelockOutcome;
pub use crate::resolver::lock::RelockStats;
pub use crate::resolver::lock::partial_relock;
pub use crate::resolver::trust::TrustEntry;
pub use crate::resolver::trust::TrustStore;
pub use crate::resolver::trust::TrustStoreError;
pub use crate::resolver::types::MaterializedFile;
pub use crate::resolver::types::ResolvedDependency;
pub use crate::resolver::types::ResolvedModule;
pub use crate::resolver::types::ResolvedTree;

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
            let is_transitive = parent.is_some();
            for (name, source) in deps {
                if is_transitive_local_disallowed(parent, source) {
                    return Err(ResolverError::LocalPathInTransitive { dep: name.clone() });
                }
                if let DependencySource::Git { url, .. } = source {
                    check_git_url_scheme(name, url, is_transitive, &self.config)?;
                }
                let resolved = self.resolve_dependency(name, source, chain).await?;
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
        chain: &'a mut Vec<(DependencyName, ResolvedSource)>,
    ) -> BoxFuture<'a, Result<ResolvedDependency, ResolverError>> {
        async move {
            let (resolved_source, manifest, module_root) =
                self.materialize_dependency(name, source).await?;

            if let Some(at) = chain.iter().position(|(_, s)| *s == resolved_source) {
                let mut path: Vec<DependencyName> =
                    chain[at..].iter().map(|(n, _)| n.clone()).collect();
                path.push(name.clone());
                return Err(ResolverError::Cycle { path });
            }

            chain.push((name.clone(), resolved_source.clone()));
            let inner = self
                .resolve_deps(&manifest.dependencies, Some(&resolved_source), chain)
                .await
                .inspect_err(|_| {
                    chain.pop();
                })?;
            chain.pop();

            let VerifiedModule { checksum, signer } =
                self.verify_materialized_dependency(name, &module_root)?;
            Ok(ResolvedDependency {
                source: resolved_source,
                modules: BTreeMap::from([(
                    ModulePath::Root,
                    ResolvedModule {
                        version: manifest.version,
                        checksum,
                        signer,
                        dependencies: inner,
                    },
                )]),
            })
        }
        .boxed()
    }

    /// Walks `module_root` and emits a `tracing::warn!` for every file
    /// whose size meets or exceeds the configured
    /// [`LargeFileWarning::Threshold`].
    fn warn_on_large_files(
        &self,
        name: &DependencyName,
        module_root: &Path,
    ) -> Result<(), ResolverError> {
        let LargeFileWarning::Threshold(threshold) = self.config.large_file_warning else {
            return Ok(());
        };
        walk_files(module_root, &mut |entry, size| {
            if size >= threshold {
                tracing::warn!(
                    dep = %name,
                    file = %entry.display(),
                    size,
                    threshold,
                    "module contains a large file",
                );
            }
            Ok(())
        })
    }

    /// Reads a `module.sig` next to `module_root` if present, verifies
    /// it against the observed `checksum`, and applies the trust-store
    /// policy (explicit pinning beats TOFU). Returns the signer key
    /// when verification succeeds, `None` when no signature file exists
    /// (and `require_signed` is off).
    fn read_and_verify_signature(
        &self,
        name: &DependencyName,
        module_root: &Path,
        checksum: &crate::ContentHash,
    ) -> Result<Option<crate::VerifyingKey>, ResolverError> {
        let sig_path = module_root.join(crate::SIGNATURE_FILENAME);
        let bytes = match std::fs::read(&sig_path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if self.config.require_signed {
                    return Err(ResolverError::RequireSignedViolation { dep: name.clone() });
                }
                return Ok(None);
            }
            Err(source) => {
                return Err(ResolverError::Io {
                    path: sig_path,
                    source,
                });
            }
        };
        let sig = crate::ModuleSignature::parse(&bytes).map_err(|source| {
            ResolverError::SignatureParse {
                dep: name.clone(),
                source,
            }
        })?;
        sig.verify(checksum)
            .map_err(|_| ResolverError::SignatureVerificationFailed {
                dep: name.clone(),
                signer: Box::new(sig.public_key),
            })?;
        if let Some(trusted) = self.trust.lookup(name)
            && &sig.public_key != trusted
        {
            return Err(ResolverError::SignerKeyMismatch {
                dep: name.clone(),
                expected: Box::new(*trusted),
                observed: Box::new(sig.public_key),
            });
        }
        Ok(Some(sig.public_key))
    }

    /// Hashes a materialized module root, applies the large-file
    /// warning, and reads/verifies `module.sig` if present.
    fn verify_materialized_dependency(
        &self,
        name: &DependencyName,
        module_root: &Path,
    ) -> Result<VerifiedModule, ResolverError> {
        let checksum = crate::hash::hash_directory(module_root)?;
        self.warn_on_large_files(name, module_root)?;
        let signer = self.read_and_verify_signature(name, module_root, &checksum)?;
        Ok(VerifiedModule { checksum, signer })
    }

    /// Resolves a [`GitSelector`] against the remote at `url` to a
    /// concrete commit SHA.
    async fn resolve_git_selector(
        &self,
        name: &DependencyName,
        url: &url::Url,
        selector: &GitSelector,
        path_prefix: Option<&str>,
    ) -> Result<(Option<Version>, crate::GitCommit), ResolverError> {
        match selector {
            GitSelector::Version(requirement) => {
                let url = url.clone();
                let requirement = requirement.clone();
                let path_prefix_owned = path_prefix.map(str::to_string);
                let max_refs = self.config.max_advertised_refs;
                let refs = tokio::task::spawn_blocking(move || {
                    crate::resolver::versions::list_remote_refs(&url, max_refs)
                })
                .await
                // SAFETY: `list_remote_refs` performs only Git work; a
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
                        dep: name.clone(),
                        requirement,
                        considered,
                    },
                })?;
                Ok((Some(version), commit))
            }
            GitSelector::Tag(tag) => {
                let url = url.clone();
                let max_refs = self.config.max_advertised_refs;
                let refs = tokio::task::spawn_blocking(move || {
                    crate::resolver::versions::list_remote_refs(&url, max_refs)
                })
                .await
                // SAFETY: `list_remote_refs` does not panic.
                .unwrap()?;
                let commit =
                    refs.get(tag)
                        .cloned()
                        .ok_or_else(|| ResolverError::UnknownGitRef {
                            dep: name.clone(),
                            kind: GitRefKind::Tag,
                            name: tag.clone(),
                        })?;
                Ok((None, commit))
            }
            GitSelector::Branch(branch) => {
                let url = url.clone();
                let max_refs = self.config.max_advertised_refs;
                let refs = tokio::task::spawn_blocking(move || {
                    crate::resolver::versions::list_remote_branches(&url, max_refs)
                })
                .await
                // SAFETY: `list_remote_branches` does not panic.
                .unwrap()?;
                let commit =
                    refs.get(branch)
                        .cloned()
                        .ok_or_else(|| ResolverError::UnknownGitRef {
                            dep: name.clone(),
                            kind: GitRefKind::Branch,
                            name: branch.clone(),
                        })?;
                Ok((None, commit))
            }
            GitSelector::Commit(commit) => {
                let commit = crate::GitCommit::try_from(commit.clone()).map_err(|_| {
                    ResolverError::InvalidCommit {
                        dep: name.clone(),
                        value: commit.clone(),
                    }
                })?;
                Ok((None, commit))
            }
        }
    }

    /// Materializes a dependency on disk and parses its manifest.
    /// Returns the resolved source, the parsed manifest, and the
    /// absolute path to the directory containing `module.json`.
    async fn materialize_dependency(
        &self,
        name: &DependencyName,
        source: &DependencySource,
    ) -> Result<(ResolvedSource, Manifest, PathBuf), ResolverError> {
        match source {
            DependencySource::LocalPath { path, .. } => {
                let manifest = read_manifest(path)?;
                Ok((
                    ResolvedSource::Path { path: path.clone() },
                    manifest,
                    path.clone(),
                ))
            }
            DependencySource::Git {
                url,
                selector,
                path,
                ..
            } => {
                let path_prefix = path.as_ref().map(GitModulePath::as_str).map(str::to_string);
                let (selected_version, commit) = self
                    .resolve_git_selector(name, url, selector, path_prefix.as_deref())
                    .await?;

                let key = CacheKey::from_url(url, &commit);
                let leaf = key.absolute_path(&self.cache_root);
                let sparse_path = path_prefix.clone().unwrap_or_else(|| ".".to_string());

                let url_for_clone = url.clone();
                let leaf_for_clone = leaf.clone();
                let commit_for_clone = commit.clone();
                tokio::task::spawn_blocking(move || {
                    crate::resolver::git::ensure_materialized(
                        &leaf_for_clone,
                        &url_for_clone,
                        commit_for_clone.inner(),
                        [sparse_path.as_str()],
                    )
                })
                .await
                // SAFETY: the closure performs only Git and filesystem
                // work; it does not panic.
                .unwrap()?;

                let module_root = match path.as_ref() {
                    Some(p) => leaf.join(p.as_path()),
                    None => leaf,
                };
                let manifest = read_manifest(&module_root)?;
                check_tag_manifest_match(
                    path_prefix.as_deref(),
                    selected_version.as_ref(),
                    &manifest.version,
                )?;
                Ok((
                    ResolvedSource::Git {
                        git: url.clone(),
                        commit,
                        path: path.clone(),
                    },
                    manifest,
                    module_root,
                ))
            }
        }
    }
}

/// Compiles a manifest's `exclude` patterns into a [`globset::GlobSet`]
/// for gitignore-style matching against import sub-paths.
fn exclude_set(patterns: &[crate::RelativePath]) -> Result<globset::GlobSet, ResolverError> {
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

/// Recursively walks every file under `root`, calling `visit` with the
/// file's path and size. Symlinks are followed; the symlink-escape
/// guard is enforced upstream by [`crate::validate_tree`].
fn walk_files(
    root: &Path,
    visit: &mut dyn FnMut(&Path, u64) -> Result<(), ResolverError>,
) -> Result<(), ResolverError> {
    let entries = std::fs::read_dir(root).map_err(|source| ResolverError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ResolverError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let meta = entry.metadata().map_err(|source| ResolverError::Io {
            path: path.clone(),
            source,
        })?;
        if meta.is_dir() {
            walk_files(&path, visit)?;
        } else if meta.is_file() {
            visit(&path, meta.len())?;
        }
    }
    Ok(())
}

/// Returns `Err(TagManifestMismatch)` when a Git tag's selected
/// semver `expected` does not equal the manifest's `declared` version.
fn check_tag_manifest_match(
    path_prefix: Option<&str>,
    expected: Option<&Version>,
    declared: &Version,
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

/// Checks that a Git URL's scheme is allowed by the configured policy.
fn check_git_url_scheme(
    name: &DependencyName,
    url: &url::Url,
    is_transitive: bool,
    config: &ModulesConfig,
) -> Result<(), ResolverError> {
    if !config.scheme_allowed(url.scheme(), is_transitive) {
        return Err(ResolverError::GitUrlPolicyViolation {
            dep: name.clone(),
            url: url.to_string(),
            scheme: url.scheme().to_string(),
        });
    }
    Ok(())
}

/// Returns `true` if `child` is a local-path source declared by a
/// non-local parent. Top-level deps (no parent in scope) and any
/// non-local child are always allowed by this rule.
fn is_transitive_local_disallowed(
    parent: Option<&ResolvedSource>,
    child: &DependencySource,
) -> bool {
    matches!(child, DependencySource::LocalPath { .. })
        && matches!(parent, Some(ResolvedSource::Git { .. }))
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

/// Artifacts produced by [`GitResolver::verify_materialized_dependency`].
struct VerifiedModule {
    /// The module's content hash.
    checksum: crate::ContentHash,
    /// The signer's public key, if the module was signed.
    signer: Option<crate::VerifyingKey>,
}

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
                    name: name.inner().to_string(),
                })?;

        let (resolved_source, manifest, module_root) =
            self.materialize_dependency(name, source).await?;

        self.verify_materialized_dependency(name, &module_root)?;

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
                dep: name.clone(),
                path: rel,
                kind: MissingFileKind::Excluded,
            });
        }

        let abs = module_root.join(&rel);
        if !abs.exists() {
            return Err(ResolverError::MissingFile {
                dep: name.clone(),
                path: rel,
                kind,
            });
        }

        let canonical_root = module_root
            .canonicalize()
            .map_err(|source| ResolverError::Io {
                path: module_root.clone(),
                source,
            })?;
        let canonical_abs = abs.canonicalize().map_err(|source| ResolverError::Io {
            path: abs.clone(),
            source,
        })?;
        if !canonical_abs.starts_with(&canonical_root) {
            return Err(ResolverError::MaterializedSymlinkEscape {
                dep: name.clone(),
                path: abs,
            });
        }

        Ok(MaterializedFile {
            path: canonical_abs,
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
                // Tag, branch, and commit selectors don't enumerate versions;
                // the caller is asking for one specific revision.
                let GitSelector::Version(requirement) = selector else {
                    return Ok(Vec::new());
                };
                let url = url.clone();
                let path_prefix = path.as_ref().map(GitModulePath::as_str).map(str::to_string);
                let requirement = requirement.clone();
                let max_refs = self.config.max_advertised_refs;
                tokio::task::spawn_blocking(move || -> Result<Vec<Version>, ResolverError> {
                    let refs = crate::resolver::versions::list_remote_refs(&url, max_refs)?;
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

    /// Builds a `module.json` at `dir` with the given name, version, and
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
        GitResolver::builder()
            .cache_root(cache.path())
            .trust_path(cache.path().join("trust.toml"))
            .trust(TrustStore::default())
            .build()
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
        let dep_src = format!("{{\"path\":\"{}\"}}", dep_dir.display());
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
        let module = dep.modules.get(&ModulePath::Root).unwrap();
        assert!(module.dependencies.is_empty());
        assert_eq!(module.version, Version::parse("1.0.0").unwrap());
    }

    #[tokio::test]
    async fn materialize_resolves_default_entrypoint() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);
        fs::write(dep_dir.join("index.wdl"), b"workflow w {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", dep_dir.display());
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap();
        let consumer = Manifest::parse(&bytes).unwrap();

        let cache = tempdir().unwrap();
        let mat = resolver(&cache)
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
        let dep_src = format!("{{\"path\":\"{}\"}}", dep_dir.display());
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap();
        let consumer = Manifest::parse(&bytes).unwrap();

        let cache = tempdir().unwrap();
        let mat = resolver(&cache)
            .materialize(&consumer, &"dep/cut".to_string().try_into().unwrap())
            .await
            .unwrap();
        assert_eq!(mat.path, dep_dir.join("cut.wdl").canonicalize().unwrap());
    }

    #[tokio::test]
    async fn invalid_commit_selector_produces_invalid_commit_error() {
        let workdir = tempdir().unwrap();
        let consumer_dir = workdir.path().join("consumer");
        let bad_src = "{\"git\":\"https://example.com/repo.git\",\"commit\":\"not-a-sha\"}";
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", bad_src)]);
        let bytes = fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap();
        let consumer = Manifest::parse(&bytes).unwrap();

        let cache = tempdir().unwrap();
        let err = resolver(&cache).resolve_tree(&consumer).await.unwrap_err();
        assert!(
            matches!(err, ResolverError::InvalidCommit { .. }),
            "expected `InvalidCommit`, got: {err}"
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
        let dep_src = format!("{{\"path\":\"{}\"}}", dep_dir.display());
        write_manifest(&consumer_dir, "consumer", "0.1.0", &[("dep", &dep_src)]);
        let consumer =
            Manifest::parse(&fs::read(consumer_dir.join(crate::MANIFEST_FILENAME)).unwrap())
                .unwrap();

        let cache = tempdir().unwrap();
        let err = resolver(&cache)
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
        let dep_src = format!("{{\"path\":\"{}\"}}", dep_dir.display());
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
            .build();
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
        fs::write(dep_dir.join("extra.wdl"), b"workflow extra {}").unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", dep_dir.display());
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
            matches!(err, ResolverError::SignatureVerificationFailed { .. }),
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
        let dep_src = format!("{{\"path\":\"{}\"}}", dep_dir.display());
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
            .build();
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
        let link = dep_dir.join("index.wdl");
        let target = outside.join("evil.wdl");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&target, &link).unwrap();

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", dep_dir.display());
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
            matches!(
                err,
                ResolverError::MaterializedSymlinkEscape { .. }
                    | ResolverError::Hash(crate::HashError::SymlinkEscapesRoot(_))
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
        let dep_src = format!("{{\"path\":\"{}\"}}", dep_dir.display());
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
        let module = dep.modules.get(&ModulePath::Root).unwrap();
        assert_eq!(module.signer.as_ref(), Some(&signer.verifying_key()));
    }

    #[tokio::test]
    async fn require_signed_rejects_unsigned_dependency() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("dep");
        write_manifest(&dep_dir, "dep", "1.0.0", &[]);

        let consumer_dir = workdir.path().join("consumer");
        let dep_src = format!("{{\"path\":\"{}\"}}", dep_dir.display());
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
            .build();
        let err = r.resolve_tree(&consumer).await.unwrap_err();
        assert!(
            matches!(err, ResolverError::RequireSignedViolation { .. }),
            "expected `RequireSignedViolation`, got: {err}"
        );
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
        let dep_src = format!("{{\"path\":\"{}\"}}", dep_dir.display());
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
        let dep_src = format!("{{\"path\":\"{}\"}}", dep_dir.display());
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
        let dep_src = format!("{{\"path\":\"{}\"}}", dep_dir.display());
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

    #[test]
    fn tag_manifest_mismatch_helper_errors_on_disagreement() {
        let v_expected = Version::parse("2.0.0").unwrap();
        let v_declared = Version::parse("1.0.0").unwrap();
        let err =
            super::check_tag_manifest_match(None, Some(&v_expected), &v_declared).unwrap_err();
        let ResolverError::TagManifestMismatch { tag, declared } = err else {
            panic!("got: {err:?}");
        };
        assert_eq!(tag, "v2.0.0");
        assert_eq!(declared, v_declared);
    }

    #[test]
    fn tag_manifest_mismatch_helper_ok_when_agree() {
        let v = Version::parse("1.2.3").unwrap();
        super::check_tag_manifest_match(Some("csvkit"), Some(&v), &v).unwrap();
    }

    #[test]
    fn tag_manifest_mismatch_helper_ok_when_no_expected() {
        super::check_tag_manifest_match(None, None, &Version::parse("0.0.1").unwrap()).unwrap();
    }

    #[test]
    fn check_git_url_scheme_blocks_file_scheme() {
        let cfg = ModulesConfig::default();
        let dep = DependencyName::try_from("foo".to_string()).unwrap();
        let url: url::Url = "file:///tmp/repo".parse().unwrap();
        let err = super::check_git_url_scheme(&dep, &url, false, &cfg).unwrap_err();
        assert!(
            matches!(err, ResolverError::GitUrlPolicyViolation { .. }),
            "got: {err}"
        );
    }

    #[test]
    fn check_git_url_scheme_allows_ssh_top_level_blocks_transitive() {
        let cfg = ModulesConfig::default();
        let dep = DependencyName::try_from("foo".to_string()).unwrap();
        let url: url::Url = "ssh://git@github.com/x/y".parse().unwrap();
        super::check_git_url_scheme(&dep, &url, false, &cfg).unwrap();
        let err = super::check_git_url_scheme(&dep, &url, true, &cfg).unwrap_err();
        assert!(matches!(err, ResolverError::GitUrlPolicyViolation { .. }));
    }

    #[test]
    fn check_git_url_scheme_allows_https_by_default() {
        let cfg = ModulesConfig::default();
        let dep = DependencyName::try_from("foo".to_string()).unwrap();
        let url: url::Url = "https://github.com/x/y".parse().unwrap();
        super::check_git_url_scheme(&dep, &url, false, &cfg).unwrap();
        super::check_git_url_scheme(&dep, &url, true, &cfg).unwrap();
    }

    #[test]
    fn local_in_transitive_helper_classifies_correctly() {
        let local = ResolvedSource::Path {
            path: "/tmp/local".into(),
        };
        let git = ResolvedSource::Git {
            git: "https://github.com/x/y".parse().unwrap(),
            commit: "0000000000000000000000000000000000000000".parse().unwrap(),
            path: None,
        };
        let local_dep = DependencySource::LocalPath {
            path: "/tmp/dep".into(),
            extra: Default::default(),
        };
        let git_dep = DependencySource::Git {
            url: "https://github.com/x/y".parse().unwrap(),
            selector: GitSelector::Tag("v1".into()),
            path: None,
            extra: Default::default(),
        };
        assert!(!super::is_transitive_local_disallowed(
            Some(&local),
            &local_dep
        ));
        assert!(super::is_transitive_local_disallowed(
            Some(&git),
            &local_dep
        ));
        assert!(!super::is_transitive_local_disallowed(None, &local_dep));
        assert!(!super::is_transitive_local_disallowed(Some(&git), &git_dep));
    }

    #[tokio::test]
    async fn resolve_tree_detects_self_cycle() {
        let workdir = tempdir().unwrap();
        let dep_dir = workdir.path().join("self-loop");

        // The dep declares itself as one of its own dependencies.
        let self_src = format!("{{\"path\":\"{}\"}}", dep_dir.display());
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
}
