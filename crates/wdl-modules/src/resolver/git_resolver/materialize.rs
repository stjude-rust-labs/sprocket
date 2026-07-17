//! Git dependency materialization.
//!
//! Owns the sparse-checkout planning, cache materialization, manifest
//! reading, and symbolic-path resolution that back the [`Resolver::materialize`]
//! entry point. The public trait method delegates to
//! [`GitResolver::materialize_file`].
//!
//! [`Resolver::materialize`]: crate::resolver::Resolver::materialize

use std::path::Path;
use std::path::PathBuf;

use semver::Version;

use super::GitResolver;
use super::resolve::locked_selector_satisfies;
use crate::Manifest;
use crate::dependency::DependencyName;
use crate::dependency::DependencySource;
use crate::dependency::GitModulePath;
use crate::dependency::GitSelector;
use crate::hash::NON_MODULE_CONTENT;
use crate::lockfile::GitCommit;
use crate::lockfile::ResolvedSource;
use crate::module::Module;
use crate::relative_path::RelativePath;
use crate::resolver::cache::CacheKey;
use crate::resolver::error::MissingFileKind;
use crate::resolver::error::ResolverError;
use crate::resolver::scope::DependencyScope;
use crate::resolver::scope::ResolutionMode;
use crate::resolver::types::MaterializedFile;
use crate::symbolic_path::SymbolicPath;

/// Pre-computed materialization parameters for a Git dependency.
#[derive(Debug)]
pub(in crate::resolver) struct GitMaterializationPlan {
    /// The selected version from tag resolution, if any.
    pub(in crate::resolver) selected_version: Option<Version>,
    /// The resolved commit SHA.
    pub(in crate::resolver) commit: GitCommit,
    /// The absolute path to the cache leaf directory.
    pub(in crate::resolver) leaf: PathBuf,
    /// The sparse-checkout path (`path_prefix` or `"."`).
    pub(in crate::resolver) sparse_path: String,
    /// The absolute path to the module root within the cache leaf.
    pub(in crate::resolver) module_path: PathBuf,
}

/// Distinguishes resolver-owned cache paths from user-owned local
/// paths. Only `Cached` variants may be evicted.
#[derive(Clone, Debug)]
pub(in crate::resolver) enum MaterializedRoot {
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
    pub(in crate::resolver) fn module_root(&self) -> &Path {
        match self {
            Self::Local(root) => root,
            Self::Cached { module_root, .. } => module_root,
        }
    }
}

impl GitResolver {
    /// Materializes a single symbolic import on disk and returns the path
    /// to the resulting file.
    ///
    /// This is the body backing [`Resolver::materialize`]. See that
    /// method's documentation for the full contract.
    ///
    /// [`Resolver::materialize`]: crate::resolver::Resolver::materialize
    pub(in crate::resolver) async fn materialize_file(
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

    /// Runs the sparse checkout for a Git dependency and returns its root.
    ///
    /// On failure, cleans up the cache leaf so a corrupt partial
    /// checkout does not persist.
    pub(in crate::resolver) async fn materialize_git(
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
    pub(in crate::resolver) async fn plan_git_materialization(
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
                    self.lockfile()
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
        let leaf = key.absolute_path(self.cache_root());
        let sparse_path = path_prefix.clone().unwrap_or_else(|| ".".to_string());
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
pub(in crate::resolver) fn read_manifest(dir: &Path) -> Result<Manifest, ResolverError> {
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
pub(in crate::resolver) fn resolve_normalized_subpath(
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
pub(in crate::resolver) fn exclude_set(
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
