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
        cache: crate::resolver::git::CacheLocation<'_>,
    ) -> Result<bool, ResolverError> {
        self.policy.check_git_url(dep, url, scope)?;
        let fetched = crate::resolver::git::ensure_materialized(
            cache,
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;

    use git2::Repository;
    use git2::Signature;
    use tempfile::tempdir;

    use super::*;
    use crate::resolver::config::ModulesConfig;

    #[test]
    fn local_remote_operations_materialize_module() -> Result<(), Box<dyn std::error::Error>> {
        let upstream = tempdir()?;
        let repo = Repository::init(upstream.path())?;
        let module = upstream.path().join("module");
        fs::create_dir(&module)?;
        fs::write(
            module.join(crate::MANIFEST_FILENAME),
            br#"{"name":"dep","license":"MIT"}"#,
        )?;
        fs::write(module.join("index.wdl"), b"version 1.3\nworkflow w {}\n")?;

        let mut index = repo.index()?;
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;
        let tree = repo.find_tree(index.write_tree()?)?;
        let signature = Signature::now("test", "test@example.com")?;
        let oid = repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])?;
        repo.tag_lightweight("v1.0.0", &repo.find_object(oid, None)?, false)?;
        let branch = repo
            .head()?
            .shorthand()
            .map_err(|source| io::Error::new(io::ErrorKind::InvalidData, source))?
            .to_string();

        let url = Url::from_file_path(upstream.path())
            .map_err(|()| io::Error::other("failed to create file URL"))?;
        let policy = ResolverPolicy::try_from(&ModulesConfig {
            allowed_schemes: vec!["file".to_string()],
            ..ModulesConfig::default()
        })?;
        let fetcher = GitFetcher::new(Arc::new(policy));
        let dependency: DependencyName = "dep".parse()?;
        let sha = oid.to_string();

        let tags = fetcher.list_tags(&dependency, &url, DependencyScope::TopLevel)?;
        assert_eq!(
            tags.get("v1.0.0").map(|commit| commit.as_str()),
            Some(sha.as_str())
        );
        let branches = fetcher.list_branches(&dependency, &url, DependencyScope::TopLevel)?;
        assert_eq!(
            branches.get(&branch).map(|commit| commit.as_str()),
            Some(sha.as_str())
        );
        assert_eq!(
            fetcher.default_branch(&dependency, &url, DependencyScope::TopLevel)?,
            branch
        );
        let resolution = tempdir()?;
        assert_eq!(
            fetcher.resolve_commit_prefix(
                &dependency,
                &url,
                &sha[..8],
                DependencyScope::TopLevel,
                &resolution.path().join("fallback"),
            )?,
            sha
        );

        let cache = tempdir()?;
        let leaf = cache.path().join(&sha);
        let location = crate::resolver::git::CacheLocation {
            root: cache.path(),
            leaf: &leaf,
        };
        assert!(fetcher.ensure_materialized(
            &dependency,
            &url,
            &sha,
            &["module"],
            DependencyScope::TopLevel,
            location,
        )?);
        assert!(leaf.join("module").join(crate::MANIFEST_FILENAME).is_file());
        assert!(!fetcher.ensure_materialized(
            &dependency,
            &url,
            &sha,
            &["module"],
            DependencyScope::TopLevel,
            location,
        )?);
        Ok(())
    }
}
