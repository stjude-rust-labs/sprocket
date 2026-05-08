//! Resolver policy types derived from
//! [`ModulesConfig`](super::config::ModulesConfig).

use url::Url;

use crate::DependencyName;
use crate::resolver::config::ModulesConfig;
use crate::resolver::error::ResolverError;
use crate::resolver::git::CredentialMode;
use crate::resolver::scope::DependencyScope;

/// Host access policy for a dependency scope.
#[derive(Clone, Debug)]
pub(crate) enum HostPolicy {
    /// Any host is allowed (deny list and IP-range checks still apply).
    Any,
    /// Only these specific hosts are allowed.
    AllowList(Vec<String>),
}

impl HostPolicy {
    fn allows(&self, host: &str) -> bool {
        match self {
            Self::Any => true,
            Self::AllowList(list) => list.iter().any(|h| h.eq_ignore_ascii_case(host)),
        }
    }
}

/// Network and resource policy for a specific dependency scope.
#[derive(Clone, Debug)]
pub(crate) struct GitNetworkPolicy {
    /// Permitted URL schemes.
    pub allowed_schemes: Vec<String>,
    /// Host access policy.
    pub host_policy: HostPolicy,
    /// Whether credentials are enabled.
    pub credential_mode: CredentialMode,
    /// Maximum advertised refs.
    pub max_advertised_refs: usize,
}

/// The full resolver policy, derived from config at construction.
#[derive(Clone, Debug)]
pub(crate) struct ResolverPolicy {
    top_level: GitNetworkPolicy,
    transitive: GitNetworkPolicy,
    /// Hosts explicitly denied for all scopes.
    denied_hosts: Vec<String>,
    /// Maximum materialized files per module tree.
    pub max_materialized_files: Option<usize>,
    /// Maximum materialized bytes per module tree.
    pub max_materialized_bytes: Option<u64>,
}

impl From<&ModulesConfig> for ResolverPolicy {
    fn from(config: &ModulesConfig) -> Self {
        let top_host = if config.allowed_hosts.is_empty() {
            HostPolicy::Any
        } else {
            HostPolicy::AllowList(config.allowed_hosts.clone())
        };
        let transitive_host = if config.allowed_transitive_hosts.is_empty() {
            HostPolicy::Any
        } else {
            HostPolicy::AllowList(config.allowed_transitive_hosts.clone())
        };
        Self {
            top_level: GitNetworkPolicy {
                allowed_schemes: config.allowed_schemes.clone(),
                host_policy: top_host,
                credential_mode: CredentialMode::Enabled,
                max_advertised_refs: config.max_advertised_refs,
            },
            transitive: GitNetworkPolicy {
                allowed_schemes: config.allowed_transitive_schemes.clone(),
                host_policy: transitive_host,
                credential_mode: if config.allow_transitive_credentials {
                    CredentialMode::Enabled
                } else {
                    CredentialMode::Disabled
                },
                max_advertised_refs: config.max_advertised_refs,
            },
            denied_hosts: config.denied_hosts.clone(),
            max_materialized_files: config.max_materialized_files,
            max_materialized_bytes: config.max_materialized_bytes,
        }
    }
}

impl ResolverPolicy {
    /// Returns the Git network policy for the given scope.
    pub fn git_policy(&self, scope: DependencyScope) -> &GitNetworkPolicy {
        match scope {
            DependencyScope::TopLevel => &self.top_level,
            DependencyScope::Transitive => &self.transitive,
        }
    }

    /// Checks that a Git URL's scheme and host are allowed.
    pub fn check_git_url(
        &self,
        name: &DependencyName,
        url: &Url,
        scope: DependencyScope,
    ) -> Result<(), ResolverError> {
        let net = self.git_policy(scope);
        if !net
            .allowed_schemes
            .iter()
            .any(|s| s.eq_ignore_ascii_case(url.scheme()))
        {
            return Err(ResolverError::GitUrlPolicyViolation {
                dep: name.clone(),
                url: url.to_string(),
                scheme: url.scheme().to_string(),
            });
        }
        if let Some(host) = url.host_str() {
            if self
                .denied_hosts
                .iter()
                .any(|h| h.eq_ignore_ascii_case(host))
            {
                return Err(ResolverError::GitHostPolicyViolation {
                    dep: name.clone(),
                    url: url.to_string(),
                    host: host.to_string(),
                });
            }
            if super::config::is_non_public_ip(host) {
                return Err(ResolverError::GitHostPolicyViolation {
                    dep: name.clone(),
                    url: url.to_string(),
                    host: host.to_string(),
                });
            }
            if !net.host_policy.allows(host) {
                return Err(ResolverError::GitHostPolicyViolation {
                    dep: name.clone(),
                    url: url.to_string(),
                    host: host.to_string(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DependencyName;
    use crate::resolver::config::ModulesConfig;
    use crate::resolver::error::ResolverError;

    #[test]
    fn blocks_file_scheme() {
        let policy = ResolverPolicy::from(&ModulesConfig::default());
        let dep = DependencyName::try_from("foo".to_string()).unwrap();
        let url: url::Url = "file:///tmp/repo".parse().unwrap();
        let err = policy
            .check_git_url(&dep, &url, DependencyScope::TopLevel)
            .unwrap_err();
        assert!(
            matches!(err, ResolverError::GitUrlPolicyViolation { .. }),
            "got: {err}"
        );
    }

    #[test]
    fn allows_ssh_top_level_blocks_transitive() {
        let policy = ResolverPolicy::from(&ModulesConfig::default());
        let dep = DependencyName::try_from("foo".to_string()).unwrap();
        let url: url::Url = "ssh://git@github.com/x/y".parse().unwrap();
        policy
            .check_git_url(&dep, &url, DependencyScope::TopLevel)
            .unwrap();
        let err = policy
            .check_git_url(&dep, &url, DependencyScope::Transitive)
            .unwrap_err();
        assert!(matches!(err, ResolverError::GitUrlPolicyViolation { .. }));
    }

    #[test]
    fn allows_https_by_default() {
        let policy = ResolverPolicy::from(&ModulesConfig::default());
        let dep = DependencyName::try_from("foo".to_string()).unwrap();
        let url: url::Url = "https://github.com/x/y".parse().unwrap();
        policy
            .check_git_url(&dep, &url, DependencyScope::TopLevel)
            .unwrap();
        policy
            .check_git_url(&dep, &url, DependencyScope::Transitive)
            .unwrap();
    }

    #[test]
    fn credential_mode_from_config() {
        let cfg = ModulesConfig {
            allow_transitive_credentials: true,
            ..ModulesConfig::default()
        };
        let policy = ResolverPolicy::from(&cfg);
        assert_eq!(
            policy
                .git_policy(DependencyScope::Transitive)
                .credential_mode,
            CredentialMode::Enabled
        );
        assert_eq!(
            policy.git_policy(DependencyScope::TopLevel).credential_mode,
            CredentialMode::Enabled
        );

        let default_policy = ResolverPolicy::from(&ModulesConfig::default());
        assert_eq!(
            default_policy
                .git_policy(DependencyScope::Transitive)
                .credential_mode,
            CredentialMode::Disabled
        );
        assert_eq!(
            default_policy
                .git_policy(DependencyScope::TopLevel)
                .credential_mode,
            CredentialMode::Enabled
        );
    }
}
