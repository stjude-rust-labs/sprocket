//! Locked dependency traversal and verification.
//!
//! Owns the lockfile-driven materialization ([`GitResolver::ensure_locked`]),
//! cache-leaf enumeration, and non-fetching verification of a consumer's
//! locked dependency tree. These operations never resolve selectors fresh;
//! they read commits straight from the lockfile.
//!
//! The lockfile is attacker-influenced input, so [`GitResolver::ensure_locked`]
//! still runs exactly one [`ResolverPolicy::check_git_url`] before each locked
//! materialization's fetch; the recorded URL is not trusted implicitly.
//!
//! [`ResolverPolicy::check_git_url`]: crate::resolver::ResolverPolicy::check_git_url

use std::path::Path;
use std::path::PathBuf;

use super::GitResolver;
use super::VerifyLockedReport;
use super::materialize::MaterializedRoot;
use crate::dependency::DependencyName;
use crate::lockfile::DependencyMap;
use crate::lockfile::ResolvedSource;
use crate::module::Module;
use crate::resolver::cache::CacheKey;
use crate::resolver::error::ResolverError;
use crate::resolver::scope::DependencyScope;
use crate::resolver::scope::ResolutionMode;

impl GitResolver {
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
