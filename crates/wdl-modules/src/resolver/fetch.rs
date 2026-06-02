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
        crate::resolver::git::ensure_materialized(
            leaf,
            url,
            commit,
            paths.iter().copied(),
            self.policy.credential_mode(scope, url.host_str()),
            self.policy.max_materialized_files,
            self.policy.max_materialized_bytes,
        )?;
        Ok(())
    }
}
