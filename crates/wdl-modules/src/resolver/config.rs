//! `[modules]` configuration parsed from `sprocket.toml`.

use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use serde_with::DeserializeFromStr;
use serde_with::SerializeDisplay;
use thiserror::Error;

/// The `[modules]` configuration section.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModulesConfig {
    /// Override the global cache location for this project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_path: Option<PathBuf>,

    /// Threshold for the large-file warning, or [`LargeFileWarning::Disabled`]
    /// when the user opts out. Defaults to 1 MiB.
    pub large_file_warning: LargeFileWarning,

    /// Reject any unsigned module in the dependency tree.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub require_signed: bool,

    /// TOFU policy for new signer keys.
    pub trust_mode: TrustMode,

    /// URL schemes permitted for top-level Git dependencies. Defaults
    /// to `["https", "ssh"]`.
    #[serde(
        default = "default_top_level_schemes",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub allowed_schemes: Vec<String>,

    /// URL schemes permitted for transitive Git dependencies. Defaults
    /// to `["https"]` so remote manifests cannot silently trigger SSH
    /// authentication against an attacker-controlled host.
    #[serde(
        default = "default_transitive_schemes",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub allowed_transitive_schemes: Vec<String>,

    /// Maximum number of advertised refs accepted from a remote.
    /// Defaults to 100,000.
    #[serde(default = "default_max_refs")]
    pub max_advertised_refs: usize,

    /// Hosts denied for all Git dependencies. Defaults to localhost
    /// addresses.
    #[serde(
        default = "default_denied_hosts",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub denied_hosts: Vec<String>,

    /// Hosts permitted for top-level Git dependencies. Empty means any
    /// non-denied host is allowed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_hosts: Vec<String>,

    /// Hosts permitted for transitive Git dependencies. Defaults to
    /// `["github.com", "gitlab.com"]`. When non-empty, a transitive
    /// dependency may only be fetched from a host on this list, and Git
    /// credentials are presented only to those hosts, so a transitive
    /// manifest cannot direct the user's credentials at a host the user
    /// has not vouched for. An empty list permits any non-denied host but
    /// presents no credentials to transitive dependencies.
    #[serde(
        default = "default_allowed_transitive_hosts",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub allowed_transitive_hosts: Vec<String>,

    /// Maximum number of files allowed in a single materialized module
    /// tree. `None` (the default) disables the limit. Checked against
    /// the Git tree object after fetch but before sparse checkout; this
    /// bounds materialized content, not network transfer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_materialized_files: Option<usize>,

    /// Maximum total bytes of regular files allowed in a single
    /// materialized module tree. `None` (the default) disables the
    /// limit. Same enforcement point as `max_materialized_files`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_materialized_bytes: Option<u64>,
}

/// Returns the default maximum advertised-ref count.
fn default_max_refs() -> usize {
    100_000
}

/// Returns the default set of allowed URL schemes for top-level Git
/// dependencies.
fn default_top_level_schemes() -> Vec<String> {
    vec!["https".into(), "ssh".into()]
}

/// Returns the default set of allowed URL schemes for transitive Git
/// dependencies.
fn default_transitive_schemes() -> Vec<String> {
    vec!["https".into()]
}

/// Returns the default set of allowed hosts for transitive Git
/// dependencies.
///
/// The two major public Git hosts are trusted by default so transitive
/// dependencies resolve out of the box while still presenting
/// credentials only to well-known hosts.
fn default_allowed_transitive_hosts() -> Vec<String> {
    vec!["github.com".into(), "gitlab.com".into()]
}

/// Returns the default denied-host list.
///
/// Loopback and unspecified addresses are blocked to prevent a
/// dependency's `module.json` from directing the resolver at a
/// service running on the user's machine. Without this, a malicious
/// transitive dependency could exfiltrate data or probe internal
/// services by pointing its `git` URL at `localhost`.
fn default_denied_hosts() -> Vec<String> {
    vec![
        "localhost".into(),
        "127.0.0.1".into(),
        "::1".into(),
        "0.0.0.0".into(),
    ]
}

impl Default for ModulesConfig {
    fn default() -> Self {
        Self {
            cache_path: None,
            large_file_warning: LargeFileWarning::default(),
            require_signed: false,
            trust_mode: TrustMode::default(),
            allowed_schemes: default_top_level_schemes(),
            allowed_transitive_schemes: default_transitive_schemes(),
            max_advertised_refs: default_max_refs(),
            denied_hosts: default_denied_hosts(),
            allowed_hosts: Vec::new(),
            allowed_transitive_hosts: default_allowed_transitive_hosts(),
            max_materialized_files: None,
            max_materialized_bytes: None,
        }
    }
}

#[cfg(test)]
use crate::resolver::DependencyScope;

#[cfg(test)]
impl ModulesConfig {
    /// Returns `true` if the given host is permitted for a dependency
    /// at this level of the tree.
    fn host_allowed(&self, host: &str, scope: DependencyScope) -> bool {
        if self
            .denied_hosts
            .iter()
            .any(|h| h.eq_ignore_ascii_case(host))
        {
            return false;
        }
        if is_non_public_ip(host) {
            return false;
        }
        let allowed = if matches!(scope, DependencyScope::Transitive) {
            &self.allowed_transitive_hosts
        } else {
            &self.allowed_hosts
        };
        allowed.is_empty() || allowed.iter().any(|h| h.eq_ignore_ascii_case(host))
    }
}

/// Returns `true` if `host` parses as a non-public IP address
/// (loopback, private RFC1918, link-local, unique-local, multicast,
/// unspecified, or the AWS/cloud metadata service at
/// `169.254.169.254`).
pub(crate) fn is_non_public_ip(host: &str) -> bool {
    use std::net::IpAddr;
    let Ok(ip) = host.parse::<IpAddr>() else {
        return false;
    };
    match ip {
        IpAddr::V4(v4) => {
            // 127.0.0.0/8
            v4.is_loopback()
                // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16 (RFC 1918)
                || v4.is_private()
                // 169.254.0.0/16 — includes the cloud metadata endpoint 169.254.169.254
                || v4.is_link_local()
                // 224.0.0.0/4
                || v4.is_multicast()
                // 0.0.0.0
                || v4.is_unspecified()
                // 255.255.255.255
                || v4.is_broadcast()
                // 100.64.0.0/10 — carrier-grade NAT (RFC 6598)
                || v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64
        }
        IpAddr::V6(v6) => {
            // ::ffff:0:0/96 — IPv4-mapped IPv6; check the inner v4 address
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_non_public_ip(&mapped.to_string());
            }
            // ::1
            v6.is_loopback()
                // ff00::/8
                || v6.is_multicast()
                // ::
                || v6.is_unspecified()
                // fc00::/7 — unique local addresses (RFC 4193)
                || (v6.segments()[0] & 0xFE00) == 0xFC00
                // fe80::/10 — link-local addresses
                || (v6.segments()[0] & 0xFFC0) == 0xFE80
        }
    }
}

/// Threshold for the large-file warning emitted at sign- and fetch-time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, SerializeDisplay, DeserializeFromStr)]
pub enum LargeFileWarning {
    /// The warning is disabled.
    Disabled,
    /// Files at or above this byte count trigger a warning.
    Threshold(u64),
}

impl Default for LargeFileWarning {
    fn default() -> Self {
        // 1 MiB
        Self::Threshold(1024 * 1024)
    }
}

impl FromStr for LargeFileWarning {
    type Err = LargeFileWarningError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("none") {
            return Ok(Self::Disabled);
        }
        let bytes = s
            .parse::<bytesize::ByteSize>()
            .map_err(|_| LargeFileWarningError(s.to_string()))?
            .as_u64();
        Ok(Self::Threshold(bytes))
    }
}

impl std::fmt::Display for LargeFileWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LargeFileWarning::Disabled => f.write_str("none"),
            LargeFileWarning::Threshold(b) => write!(f, "{}", bytesize::ByteSize(*b)),
        }
    }
}

/// Error parsing a [`LargeFileWarning`] string.
#[derive(Debug, Error)]
#[error("`{0}` is not a valid file-size string (expected e.g. `1MiB`, `500KB`, or `none`)")]
pub struct LargeFileWarningError(String);

/// Trust-on-first-use (TOFU) policy for new signer keys.
///
/// When the resolver encounters a signed module whose signer key is not
/// yet recorded in the lockfile, this setting controls whether the key
/// is accepted silently or requires explicit user confirmation. The
/// library computes a [`LockfileDiff`](super::lock::LockfileDiff) that
/// flags new signers; the CLI is responsible for acting on the policy
/// (e.g., prompting the user when `Confirm` is set).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrustMode {
    /// New signer keys are recorded in the lockfile without prompting.
    /// This is the default and is suitable for non-interactive or
    /// CI environments where manual confirmation is impractical.
    #[default]
    Auto,
    /// The CLI must prompt the user to confirm any newly-trusted signer
    /// key before writing the lockfile. Intended for interactive use
    /// where the user wants to review each new signer.
    Confirm,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::DependencyScope;

    #[test]
    fn parses_default_threshold_when_absent() {
        let cfg: ModulesConfig = toml::from_str("").unwrap();
        assert!(matches!(
            cfg.large_file_warning,
            LargeFileWarning::Threshold(b) if b == 1024 * 1024
        ));
    }

    #[test]
    fn parses_size_string() {
        let cfg: ModulesConfig = toml::from_str(r#"large_file_warning = "5MiB""#).unwrap();
        assert!(matches!(
            cfg.large_file_warning,
            LargeFileWarning::Threshold(b) if b == 5 * 1024 * 1024
        ));
    }

    #[test]
    fn parses_none_sentinel() {
        for s in ["none", "NONE", "None"] {
            let cfg: ModulesConfig =
                toml::from_str(&format!(r#"large_file_warning = "{s}""#)).unwrap();
            assert!(matches!(cfg.large_file_warning, LargeFileWarning::Disabled));
        }
    }

    #[test]
    fn rejects_invalid_size_string() {
        let err = toml::from_str::<ModulesConfig>(r#"large_file_warning = "abc""#).unwrap_err();
        assert!(err.to_string().contains("abc"), "wrong message: {err}");
    }

    #[test]
    fn default_policy_denies_localhost_hosts() {
        let cfg = ModulesConfig::default();
        assert!(!cfg.host_allowed("localhost", DependencyScope::TopLevel));
        assert!(!cfg.host_allowed("127.0.0.1", DependencyScope::Transitive));
        assert!(!cfg.host_allowed("::1", DependencyScope::Transitive));
        assert!(!cfg.host_allowed("0.0.0.0", DependencyScope::TopLevel));
    }

    #[test]
    fn default_policy_denies_private_and_metadata_ips() {
        let cfg = ModulesConfig::default();
        let denied = [
            "169.254.169.254",
            "10.0.0.1",
            "192.168.1.1",
            "172.16.0.1",
            "100.64.0.1",
            "127.0.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "ff02::1",
            // IPv4-mapped IPv6
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "::ffff:10.0.0.1",
            "::ffff:192.168.1.1",
        ];
        for ip in denied {
            for scope in [DependencyScope::TopLevel, DependencyScope::Transitive] {
                assert!(
                    !cfg.host_allowed(ip, scope),
                    "`{ip}` should be denied for `{scope:?}`"
                );
            }
        }
    }

    #[test]
    fn default_policy_allows_public_hosts() {
        let cfg = ModulesConfig::default();
        assert!(cfg.host_allowed("github.com", DependencyScope::TopLevel));
        assert!(cfg.host_allowed("github.com", DependencyScope::Transitive));
        assert!(
            cfg.host_allowed("::ffff:140.82.121.3", DependencyScope::TopLevel),
            "public IPv4-mapped IPv6 should be allowed"
        );
    }

    #[test]
    fn allowlist_limits_transitive_hosts() {
        let cfg = ModulesConfig {
            allowed_transitive_hosts: vec!["github.com".into()],
            ..ModulesConfig::default()
        };
        assert!(cfg.host_allowed("github.com", DependencyScope::Transitive));
        assert!(!cfg.host_allowed("gitlab.com", DependencyScope::Transitive));
        assert!(cfg.host_allowed("gitlab.com", DependencyScope::TopLevel));
    }
}
