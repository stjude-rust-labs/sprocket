//! Centralized Git remote access with policy enforcement.

use std::path::Path;

use url::Url;

use crate::DependencyName;
use crate::resolver::error::ResolverError;
use crate::resolver::policy::ResolverPolicy;
use crate::resolver::scope::DependencyScope;
use crate::resolver::versions::RemoteRefs;

/// Centralized Git remote access. Every remote operation enforces
/// URL scheme, host, credential, and ref-count policy before
/// touching the network.
pub(crate) struct GitFetcher {
    /// The resolver policy applied to all remote operations.
    policy: ResolverPolicy,
}

impl GitFetcher {
    /// Creates a fetcher from a resolver policy.
    pub fn new(policy: ResolverPolicy) -> Self {
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
        crate::resolver::versions::list_remote_refs(
            url,
            net.max_advertised_refs,
            net.credential_mode,
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
        crate::resolver::versions::list_remote_branches(
            url,
            net.max_advertised_refs,
            net.credential_mode,
        )
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
    ) -> Result<(), ResolverError> {
        self.policy.check_git_url(dep, url, scope)?;
        let net = self.policy.git_policy(scope);
        crate::resolver::git::ensure_materialized(
            leaf,
            url,
            commit,
            paths.iter().copied(),
            net.credential_mode,
            self.policy.max_materialized_files,
            self.policy.max_materialized_bytes,
        )?;
        Ok(())
    }
}
