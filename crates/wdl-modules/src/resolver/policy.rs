//! Network and resource enforcement policy for the module resolver.
//!
//! This module translates a [`ModulesConfig`](super::config::ModulesConfig)
//! into a [`ResolverPolicy`] that is evaluated at fetch time. Each Git URL is
//! checked against the policy before any network activity occurs: the URL
//! scheme must appear in the configured allow list, the hostname must not be on
//! the explicit deny list, the hostname must not be a non-public IP address or
//! resolve to one, and the hostname must satisfy the per-scope host policy
//! (open or allowlisted). Top-level and transitive dependencies carry separate
//! [`GitNetworkPolicy`] instances, which allows stricter rules for code pulled
//! in transitively (e.g., no SSH, no credentials).

use std::num::TryFromIntError;

use thiserror::Error;
use url::Url;

use crate::dependency::DependencyName;
use crate::resolver::DependencyScope;
use crate::resolver::config::LargeFileWarning;
use crate::resolver::config::ModulesConfig;
use crate::resolver::error::ResolverError;
use crate::resolver::git::CredentialMode;

/// An error parsing a [`DependencyName`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ResolverPolicyError {
    /// An invalid maximum advertise references value was encountered.
    #[error("invalid maximum advertised references value")]
    InvalidMaxAdvertisedRefs {
        /// The value of the maximum advertised references.
        value: u64,
        /// The underlying error.
        #[source]
        source: TryFromIntError,
    },
    /// An invalid maximum materialized files value was encountered.
    #[error("invalid maximum materialized files value")]
    InvalidMaxMaterializedFiles {
        /// The value of the maximum materialized files.
        value: u64,
        /// The underlying error.
        #[source]
        source: TryFromIntError,
    },
}

/// Host access policy for a dependency scope.
#[derive(Clone, Debug)]
pub(crate) enum HostPolicy {
    /// Any host is allowed.
    ///
    /// Note that deny list and IP-range checks still apply.
    Any,
    /// Only these specific hosts are allowed.
    AllowList(Vec<String>),
}

impl HostPolicy {
    /// Returns `true` if the given `host` is permitted by this policy.
    fn allows(&self, host: &str) -> bool {
        match self {
            Self::Any => true,
            Self::AllowList(list) => list.iter().any(|h| h.eq_ignore_ascii_case(host)),
        }
    }

    /// Returns `true` only if `host` is named in an explicit allow list.
    ///
    /// An open (`Any`) policy returns `false`, since it expresses no
    /// explicit trust in any particular host.
    fn explicitly_allows(&self, host: &str) -> bool {
        match self {
            Self::Any => false,
            Self::AllowList(list) => list.iter().any(|h| h.eq_ignore_ascii_case(host)),
        }
    }
}

/// Network and resource policy for a specific dependency scope.
#[derive(Clone, Debug)]
pub(crate) struct GitNetworkPolicy {
    /// Permitted URL schemes.
    pub(crate) allowed_schemes: Vec<String>,
    /// Host access policy.
    pub(crate) host_policy: HostPolicy,
    /// Maximum advertised refs.
    pub(crate) max_advertised_refs: usize,
}

/// The full resolver policy, derived from config at construction.
#[derive(Clone, Debug)]
pub struct ResolverPolicy {
    /// Network policy applied to top-level dependencies.
    top_level: GitNetworkPolicy,
    /// Network policy applied to transitive dependencies.
    transitive: GitNetworkPolicy,
    /// Hosts explicitly denied for all scopes.
    denied_hosts: Vec<String>,
    /// Maximum materialized files per module tree.
    pub(crate) max_materialized_files: Option<usize>,
    /// Maximum materialized bytes per module tree.
    pub(crate) max_materialized_bytes: Option<u64>,
    /// Large-file warning threshold.
    pub(crate) large_file_warning: LargeFileWarning,
    /// Whether unsigned modules are rejected.
    pub(crate) require_signed: bool,
    /// Whether Git operations may use credential helpers.
    credentials_enabled: bool,
}

impl Default for ResolverPolicy {
    fn default() -> Self {
        Self::try_from(&ModulesConfig::default()).expect("default module configuration is invalid")
    }
}

impl TryFrom<&ModulesConfig> for ResolverPolicy {
    type Error = ResolverPolicyError;

    fn try_from(config: &ModulesConfig) -> Result<Self, Self::Error> {
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

        Ok(Self {
            top_level: GitNetworkPolicy {
                allowed_schemes: config.allowed_schemes.clone(),
                host_policy: top_host,
                max_advertised_refs: config.max_advertised_refs.try_into().map_err(|e| {
                    ResolverPolicyError::InvalidMaxAdvertisedRefs {
                        value: config.max_advertised_refs,
                        source: e,
                    }
                })?,
            },
            transitive: GitNetworkPolicy {
                allowed_schemes: config.allowed_transitive_schemes.clone(),
                host_policy: transitive_host,
                max_advertised_refs: config.max_advertised_refs.try_into().map_err(|e| {
                    ResolverPolicyError::InvalidMaxAdvertisedRefs {
                        value: config.max_advertised_refs,
                        source: e,
                    }
                })?,
            },
            denied_hosts: config.denied_hosts.clone(),
            max_materialized_files: config
                .max_materialized_files
                .map(|v| {
                    v.try_into()
                        .map_err(|e| ResolverPolicyError::InvalidMaxMaterializedFiles {
                            value: v,
                            source: e,
                        })
                })
                .transpose()?,
            max_materialized_bytes: config.max_materialized_bytes,
            large_file_warning: config.large_file_warning,
            require_signed: config.require_signed,
            credentials_enabled: true,
        })
    }
}

impl ResolverPolicy {
    /// Returns this policy with Git credentials disabled for every scope.
    pub fn without_credentials(mut self) -> Self {
        self.credentials_enabled = false;
        self
    }

    /// Returns the Git network policy for the given scope.
    pub(crate) fn git_policy(&self, scope: DependencyScope) -> &GitNetworkPolicy {
        match scope {
            DependencyScope::TopLevel => &self.top_level,
            DependencyScope::Transitive => &self.transitive,
        }
    }

    /// Returns the credential mode for a Git operation against `host`.
    ///
    /// Top-level dependencies always present credentials, since the user
    /// declared their URLs directly. Transitive dependencies present
    /// credentials only for hosts named explicitly in
    /// `allowed_transitive_hosts`, so a transitive manifest cannot direct
    /// the user's credentials at a host the user has not vouched for.
    pub(crate) fn credential_mode(
        &self,
        scope: DependencyScope,
        host: Option<&str>,
    ) -> CredentialMode {
        if !self.credentials_enabled {
            return CredentialMode::Disabled;
        }

        match scope {
            DependencyScope::TopLevel => CredentialMode::Enabled,
            DependencyScope::Transitive => match host {
                Some(host) if self.transitive.host_policy.explicitly_allows(host) => {
                    CredentialMode::Enabled
                }
                _ => CredentialMode::Disabled,
            },
        }
    }

    /// Checks that a Git URL's scheme and host are allowed.
    pub(crate) fn check_git_url(
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
                dep: name.manifest().to_string(),
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
                    dep: name.manifest().to_string(),
                    url: url.to_string(),
                    host: host.to_string(),
                });
            }
            if super::config::is_non_public_ip(host) {
                return Err(ResolverError::GitHostPolicyViolation {
                    dep: name.manifest().to_string(),
                    url: url.to_string(),
                    host: host.to_string(),
                });
            }
            if !net.host_policy.allows(host) {
                return Err(ResolverError::GitHostNotAllowed {
                    dep: name.manifest().to_string(),
                    url: url.to_string(),
                    host: host.to_string(),
                    config_key: match scope {
                        DependencyScope::TopLevel => "allowed_hosts",
                        DependencyScope::Transitive => "allowed_transitive_hosts",
                    },
                });
            }
            // Resolve the hostname to IP addresses and reject if any
            // resolved address is non-public.
            //
            // Port 0 is passed only because `to_socket_addrs` requires a
            // port; the value is insignificant for the address lookup and
            // the DNS result is identical for any port. The policy does not
            // restrict which port the URL itself uses; that is left to the
            // caller's URL.
            //
            // Both DNS failure and empty results are treated as rejection
            // (fail-closed). libgit2 re-resolves during connect/clone, so
            // a DNS rebinding attack between this check and the fetch
            // remains possible; fully preventing it would require
            // peer-IP validation in a custom transport.
            if host.parse::<std::net::IpAddr>().is_err() && url.scheme() != "file" {
                let addrs: Vec<std::net::SocketAddr> =
                    match std::net::ToSocketAddrs::to_socket_addrs(&(host, 0)) {
                        Ok(iter) => iter.collect(),
                        Err(_) => {
                            return Err(ResolverError::GitHostResolutionFailed {
                                dep: name.manifest().to_string(),
                                url: url.to_string(),
                                host: host.to_string(),
                            });
                        }
                    };
                if let Err(bad_ip) = validate_resolved_addresses(&addrs) {
                    return match bad_ip {
                        Some(ip) => Err(ResolverError::GitHostPolicyViolation {
                            dep: name.manifest().to_string(),
                            url: url.to_string(),
                            host: format!("{host} (resolves to {ip})"),
                        }),
                        None => Err(ResolverError::GitHostResolutionFailed {
                            dep: name.manifest().to_string(),
                            url: url.to_string(),
                            host: host.to_string(),
                        }),
                    };
                }
            }
        }
        Ok(())
    }
}

/// Validates that a set of resolved socket addresses contains at least
/// one entry and that none resolve to non-public IPs. Returns the
/// offending IP string on failure.
fn validate_resolved_addresses(addrs: &[std::net::SocketAddr]) -> Result<(), Option<String>> {
    if addrs.is_empty() {
        return Err(None);
    }
    for addr in addrs {
        let ip = addr.ip().to_string();
        if super::config::is_non_public_ip(&ip) {
            return Err(Some(ip));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency::DependencyName;
    use crate::resolver::config::ModulesConfig;
    use crate::resolver::error::ResolverError;

    fn dependency() -> DependencyName {
        "dep".parse().unwrap()
    }

    fn url(source: &str) -> Url {
        source.parse().unwrap()
    }

    #[test]
    fn blocks_file_scheme() {
        let policy = ResolverPolicy::default();
        let dep = dependency();
        let url = url("file:///repo");
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
        let policy = ResolverPolicy::default();
        let dep = dependency();
        let url = url("ssh://git@github.com/x/y");
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
        let policy = ResolverPolicy::default();
        let dep = dependency();
        let url = url("https://github.com/x/y");
        policy
            .check_git_url(&dep, &url, DependencyScope::TopLevel)
            .unwrap();
        policy
            .check_git_url(&dep, &url, DependencyScope::Transitive)
            .unwrap();
    }

    #[test]
    fn transitive_credentials_require_host_allowlist() {
        let default_policy = ResolverPolicy::default();
        assert_eq!(
            default_policy.credential_mode(DependencyScope::Transitive, Some("github.com")),
            CredentialMode::Enabled
        );
        assert_eq!(
            default_policy.credential_mode(DependencyScope::Transitive, Some("bitbucket.org")),
            CredentialMode::Disabled
        );
        assert_eq!(
            default_policy.credential_mode(DependencyScope::TopLevel, Some("bitbucket.org")),
            CredentialMode::Enabled
        );

        let open = ResolverPolicy::try_from(&ModulesConfig {
            allowed_transitive_hosts: Vec::new(),
            ..ModulesConfig::default()
        })
        .unwrap();
        assert_eq!(
            open.credential_mode(DependencyScope::Transitive, Some("github.com")),
            CredentialMode::Disabled
        );
    }

    #[test]
    fn rejects_loopback_and_private_hosts_through_public_policy_boundary() {
        let policy = ResolverPolicy::default();
        let dep = dependency();
        for (scope, source) in [
            (
                DependencyScope::TopLevel,
                "https://localhost/repository.git",
            ),
            (
                DependencyScope::Transitive,
                "https://127.0.0.1/repository.git",
            ),
            (DependencyScope::TopLevel, "https://0.0.0.0/repository.git"),
        ] {
            let error = policy
                .check_git_url(&dep, &url(source), scope)
                .expect_err("loopback and private hosts must be rejected");
            assert!(
                matches!(error, ResolverError::GitHostPolicyViolation { .. }),
                "got: {error}"
            );
        }
    }

    #[test]
    fn rejects_ipv6_literal_hosts_fail_closed_at_resolution() {
        let policy = ResolverPolicy::default();
        let dep = dependency();
        for source in [
            "https://[::1]/repository.git",
            "https://[::ffff:127.0.0.1]/repository.git",
            "https://[::ffff:169.254.169.254]/repository.git",
            "https://[::ffff:10.0.0.1]/repository.git",
        ] {
            let error = policy
                .check_git_url(&dep, &url(source), DependencyScope::TopLevel)
                .expect_err("IPv6-literal URLs must fail closed");
            assert!(
                matches!(error, ResolverError::GitHostResolutionFailed { .. }),
                "got: {error}"
            );
        }
    }

    #[test]
    fn allows_public_ipv4_host() {
        let policy = ResolverPolicy::default();
        let dep = dependency();
        policy
            .check_git_url(
                &dep,
                &url("https://140.82.121.3/repository.git"),
                DependencyScope::TopLevel,
            )
            .expect("configured public host should be allowed");
    }

    #[test]
    fn allowlists_apply_per_scope_for_complete_urls() {
        let policy = ResolverPolicy::try_from(&ModulesConfig {
            allowed_hosts: vec!["github.com".into()],
            allowed_transitive_hosts: vec!["gitlab.com".into()],
            ..ModulesConfig::default()
        })
        .unwrap();
        let dep = dependency();

        policy
            .check_git_url(
                &dep,
                &url("https://github.com/org/repository.git"),
                DependencyScope::TopLevel,
            )
            .expect("configured host should be allowed");
        policy
            .check_git_url(
                &dep,
                &url("https://gitlab.com/org/repository.git"),
                DependencyScope::Transitive,
            )
            .expect("configured host should be allowed");

        let error = policy
            .check_git_url(
                &dep,
                &url("https://github.com/org/repository.git"),
                DependencyScope::Transitive,
            )
            .expect_err("top-level allowlist should not apply to transitive dependencies");
        assert!(
            matches!(
                error,
                ResolverError::GitHostNotAllowed {
                    config_key: "allowed_transitive_hosts",
                    ..
                }
            ),
            "got: {error}"
        );

        let error = policy
            .check_git_url(
                &dep,
                &url("https://gitlab.com/org/repository.git"),
                DependencyScope::TopLevel,
            )
            .expect_err("transitive allowlist should not apply to top-level dependencies");
        assert!(
            matches!(
                error,
                ResolverError::GitHostNotAllowed {
                    config_key: "allowed_hosts",
                    ..
                }
            ),
            "got: {error}"
        );
    }

    #[test]
    fn credentials_can_be_disabled_for_every_scope() {
        let policy = ResolverPolicy::default().without_credentials();
        assert_eq!(
            policy.credential_mode(DependencyScope::TopLevel, Some("github.com")),
            CredentialMode::Disabled
        );
        assert_eq!(
            policy.credential_mode(DependencyScope::Transitive, Some("github.com")),
            CredentialMode::Disabled
        );
    }

    #[test]
    fn empty_address_list_is_rejected() {
        assert!(validate_resolved_addresses(&[]).is_err());
    }

    #[test]
    fn loopback_address_is_rejected() {
        let addr: std::net::SocketAddr = "127.0.0.1:443".parse().unwrap();
        let result = validate_resolved_addresses(&[addr]);
        assert!(result.is_err());
        assert!(result.unwrap_err().is_some());
    }

    #[test]
    fn public_address_is_accepted() {
        let addr: std::net::SocketAddr = "140.82.121.3:443".parse().unwrap();
        assert!(validate_resolved_addresses(&[addr]).is_ok());
    }

    #[test]
    fn dns_failure_rejects_url() {
        let policy = ResolverPolicy::default();
        let dep = dependency();
        let url = url("https://this-host-does-not-exist-xyzzy.invalid/x/y");
        let err = policy
            .check_git_url(&dep, &url, DependencyScope::TopLevel)
            .unwrap_err();
        assert!(
            matches!(err, ResolverError::GitHostResolutionFailed { .. }),
            "expected `GitHostResolutionFailed`, got: {err}"
        );
    }
}
