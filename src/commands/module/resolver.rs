//! Module resolver environment construction.

use std::path::PathBuf;

use wdl_modules::Lockfile;
use wdl_modules::resolver::GitResolver;
use wdl_modules::resolver::ResolverPolicy;
use wdl_modules::resolver::TrustStore;

use super::TrustStoreFile;
use crate::config::Config;

/// The shared cache, trust, and policy inputs used to build resolvers for
/// module porcelain commands.
#[derive(Clone, Debug)]
pub(crate) struct ResolverEnvironment {
    /// Root directory of the module cache.
    cache_root: PathBuf,
    /// Resolver policy derived from module configuration.
    policy: ResolverPolicy,
    /// Trust store loaded from the user trust path.
    trust: TrustStore,
}

impl ResolverEnvironment {
    /// Builds the resolver environment from module configuration, loading the
    /// trust store from the default trust path.
    pub(crate) fn from_config(config: &Config) -> anyhow::Result<Self> {
        let configured_cache = config.modules.cache_path.is_some();
        let cache_root = config
            .modules
            .cache_path
            .clone()
            .unwrap_or_else(crate::analysis::default_cache_root);

        let trust_path = crate::analysis::default_trust_path();
        tracing::info!(
            cache = %cache_root.display(),
            configured = configured_cache,
            "using module cache"
        );
        tracing::info!(
            trust_store = %trust_path.display(),
            "using module trust store"
        );
        let trust_file = TrustStoreFile::load(trust_path)?;
        let trust = trust_file.into_store();
        let policy = ResolverPolicy::try_from(&config.modules)?;
        Ok(Self {
            cache_root,
            policy,
            trust,
        })
    }

    /// Builds a Git resolver bound to the given lockfile with an initialized
    /// cache.
    pub(crate) fn resolver(&self, lockfile: Lockfile) -> anyhow::Result<GitResolver> {
        tracing::debug!(
            cache = %self.cache_root.display(),
            trusted = self.trust.keys.len(),
            locked = lockfile.dependencies.len(),
            "built module resolver"
        );
        let resolver = GitResolver::builder()
            .cache_root(self.cache_root.clone())
            .trust(self.trust.clone())
            .lockfile(lockfile)
            .policy(self.policy.clone())
            .build();
        resolver.initialize_cache()?;
        Ok(resolver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_environment_uses_configured_cache() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.modules.cache_path = Some(directory.path().join("cache"));

        let environment = ResolverEnvironment::from_config(&config).unwrap();
        let resolver = environment.resolver(Lockfile::default()).unwrap();

        assert_eq!(resolver.cache_root(), directory.path().join("cache"));
    }
}
