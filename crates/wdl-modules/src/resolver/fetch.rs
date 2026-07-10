//! Centralized Git remote access with policy enforcement.

use std::path::Path;
use std::sync::Arc;

use url::Url;

use crate::dependency::DependencyName;
use crate::resolver::DependencyScope;
use crate::resolver::error::ResolverError;
use crate::resolver::policy::ResolverPolicy;
use crate::resolver::versions::RemoteRefs;

/// Centralized Git remote access. Every remote operation enforces
/// URL scheme, host, credential, and ref-count policy before
/// touching the network.
pub(crate) struct GitFetcher {
    /// The resolver policy applied to all remote operations.
    policy: Arc<ResolverPolicy>,
}

impl GitFetcher {
    /// Creates a fetcher from a resolver policy.
    pub fn new(policy: Arc<ResolverPolicy>) -> Self {
        Self { policy }
    }

    /// Lists tags from the remote, enforcing URL and credential policy.
    pub fn list_tags(
        &self,
        dep: &DependencyName,
        url: &Url,
        scope: DependencyScope,
    ) -> Result<RemoteRefs, ResolverError> {
        self.policy.check_git_url(dep, url, scope)?;
        let net = self.policy.git_policy(scope);
        crate::resolver::versions::discover_remote_tags(
            url,
            net.max_advertised_refs,
            self.policy.credential_mode(scope, url.host_str()),
        )
        .map_err(ResolverError::from)
    }

    /// Lists branches from the remote, enforcing URL and credential
    /// policy.
    pub fn list_branches(
        &self,
        dep: &DependencyName,
        url: &Url,
        scope: DependencyScope,
    ) -> Result<RemoteRefs, ResolverError> {
        self.policy.check_git_url(dep, url, scope)?;
        let net = self.policy.git_policy(scope);
        crate::resolver::versions::discover_remote_branches(
            url,
            net.max_advertised_refs,
            self.policy.credential_mode(scope, url.host_str()),
        )
        .map_err(ResolverError::from)
    }

    /// Discovers the remote's default branch.
    pub fn default_branch(
        &self,
        _dep: &DependencyName,
        url: &Url,
        scope: DependencyScope,
    ) -> Result<String, ResolverError> {
        let net = self.policy.git_policy(scope);
        crate::resolver::git::discover_default_branch(
            url,
            self.policy.credential_mode(scope, url.host_str()),
            net.max_advertised_refs,
        )
        .map_err(ResolverError::from)
    }

    /// Expands a commit-SHA prefix to the full SHA, enforcing URL and
    /// credential policy.
    ///
    /// First tries `ls-remote`: the Git wire protocol advertises full ref
    /// SHAs, so a prefix that names a ref tip (a branch or tag head) is
    /// expanded without any clone. Only when the prefix does not uniquely
    /// match an advertised ref does it fall back to cloning into
    /// `work_dir` (which must not already exist and is the caller's to
    /// remove afterward), since the protocol cannot expand a prefix that
    /// points into history.
    pub fn resolve_commit_prefix(
        &self,
        dep: &DependencyName,
        url: &Url,
        prefix: &str,
        scope: DependencyScope,
        work_dir: &Path,
    ) -> Result<String, ResolverError> {
        self.policy.check_git_url(dep, url, scope)?;
        let net = self.policy.git_policy(scope);
        let mode = self.policy.credential_mode(scope, url.host_str());

        // Fast path: a prefix of an advertised ref's SHA needs no clone.
        let refs = crate::resolver::git::list_advertised_refs(url, net.max_advertised_refs, mode)?;
        if let Some(sha) = crate::resolver::git::unique_ref_prefix_match(&refs, prefix) {
            return Ok(sha.to_string());
        }

        crate::resolver::git::resolve_commit_prefix(work_dir, url, prefix, mode)
            .map_err(ResolverError::from)
    }

    /// Ensures a cache leaf is materialized, enforcing URL, credential,
    /// and tree-size policy.
    pub fn ensure_materialized(
        &self,
        dep: &DependencyName,
        url: &Url,
        commit: &str,
        paths: &[&str],
        scope: DependencyScope,
        leaf: &Path,
    ) -> Result<bool, ResolverError> {
        self.policy.check_git_url(dep, url, scope)?;
        let fetched = crate::resolver::git::ensure_materialized(
            leaf,
            url,
            commit,
            paths.iter().copied(),
            self.policy.credential_mode(scope, url.host_str()),
            self.policy.max_materialized_files,
            self.policy.max_materialized_bytes,
        )?;
        Ok(fetched)
    }
}
