//! `[modules]` configuration parsed from `sprocket.toml`.

use std::path::PathBuf;
use std::str::FromStr;

use thiserror::Error;
use toml_spanner::Toml;
use toml_spanner::helper::display;
use toml_spanner::helper::parse_string;

/// The `[modules]` configuration section.
#[derive(Clone, Debug, PartialEq, Eq, Toml)]
#[toml(Toml, deny_unknown_fields)]
pub struct ModulesConfig {
    /// Override the global cache location for this project.
    pub cache_path: Option<PathBuf>,

    /// The platform used to expand `owner/repo` dependency shorthands.
    #[toml(default)]
    pub default_git_platform: GitPlatform,

    /// Threshold for the large-file warning, or [`LargeFileWarning::Disabled`]
    /// when the user opts out. Defaults to 1 MiB.
    #[toml(default, FromToml with = parse_string, ToToml with = display)]
    pub large_file_warning: LargeFileWarning,

    /// Reject any unsigned module in the dependency tree.
    #[toml(default)]
    pub require_signed: bool,

    /// Policy for accepting signer keys.
    #[toml(default)]
    pub trust_mode: TrustMode,

    /// URL schemes permitted for top-level Git dependencies. Defaults
    /// to `["https", "ssh"]`.
    #[toml(default = default_top_level_schemes())]
    pub allowed_schemes: Vec<String>,

    /// URL schemes permitted for transitive Git dependencies. Defaults
    /// to `["https"]` so remote manifests cannot silently trigger SSH
    /// authentication against an attacker-controlled host.
    #[toml(default = default_transitive_schemes())]
    pub allowed_transitive_schemes: Vec<String>,

    /// Maximum number of advertised refs accepted from a remote.
    /// Defaults to 100,000.
    #[toml(default = DEFAULT_MAX_REFS)]
    pub max_advertised_refs: u64,

    /// Hosts denied for all Git dependencies. Defaults to localhost
    /// addresses.
    #[toml(default = default_denied_hosts())]
    pub denied_hosts: Vec<String>,

    /// Hosts permitted for top-level Git dependencies. Empty means any
    /// non-denied host is allowed.
    #[toml(default)]
    pub allowed_hosts: Vec<String>,

    /// Hosts permitted for transitive Git dependencies. Defaults to
    /// `["github.com", "gitlab.com"]`. When non-empty, a transitive
    /// dependency may only be fetched from a host on this list, and Git
    /// credentials are presented only to those hosts, so a transitive
    /// manifest cannot direct the user's credentials at a host the user
    /// has not vouched for. An empty list permits any non-denied host but
    /// presents no credentials to transitive dependencies.
    #[toml(default = default_allowed_transitive_hosts())]
    pub allowed_transitive_hosts: Vec<String>,

    /// Maximum number of files allowed in a single materialized module
    /// tree. `None` (the default) disables the limit. Checked against
    /// the Git tree object after fetch but before sparse checkout; this
    /// bounds materialized content, not network transfer.
    pub max_materialized_files: Option<u64>,

    /// Maximum total bytes of regular files allowed in a single
    /// materialized module tree. `None` (the default) disables the
    /// limit. Same enforcement point as `max_materialized_files`.
    pub max_materialized_bytes: Option<u64>,
}

/// The default maximum advertised-ref count.
const DEFAULT_MAX_REFS: u64 = 100_000;

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
            default_git_platform: GitPlatform::default(),
            large_file_warning: LargeFileWarning::default(),
            require_signed: false,
            trust_mode: TrustMode::default(),
            allowed_schemes: default_top_level_schemes(),
            allowed_transitive_schemes: default_transitive_schemes(),
            max_advertised_refs: DEFAULT_MAX_REFS,
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

/// A hosted Git platform used for dependency shorthand expansion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Toml)]
#[toml(Toml, rename_all = "lowercase")]
pub enum GitPlatform {
    /// GitHub repository shorthand.
    #[default]
    Github,
    /// GitLab repository shorthand.
    Gitlab,
    /// Bitbucket repository shorthand.
    Bitbucket,
}

impl GitPlatform {
    /// Expands an `owner/repo` shorthand into a hosted Git URL.
    pub fn expand_shorthand(self, source: &str) -> Option<Result<url::Url, url::ParseError>> {
        let shorthand = source.parse::<HostedGitShorthand>().ok()?;
        Some(self.repository_url(&shorthand.owner, &shorthand.repo))
    }

    /// Returns the inferred dependency name for an `owner/repo` shorthand.
    pub fn shorthand_repo_name(source: &str) -> Option<String> {
        let shorthand = source.parse::<HostedGitShorthand>().ok()?;
        Some(strip_git_suffix(&shorthand.repo).to_string())
    }

    /// Builds the hosted Git URL for an owner and repository.
    fn repository_url(self, owner: &str, repo: &str) -> Result<url::Url, url::ParseError> {
        let url = format!(
            "https://{host}/{owner}/{repo}.git",
            host = self.host(),
            repo = strip_git_suffix(repo)
        );
        url.parse()
    }

    /// Returns the canonical host name for this platform.
    fn host(self) -> &'static str {
        match self {
            Self::Github => "github.com",
            Self::Gitlab => "gitlab.com",
            Self::Bitbucket => "bitbucket.org",
        }
    }
}

impl FromStr for GitPlatform {
    type Err = GitPlatformError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "github" => Ok(Self::Github),
            "gitlab" => Ok(Self::Gitlab),
            "bitbucket" => Ok(Self::Bitbucket),
            _ => Err(GitPlatformError(s.to_string())),
        }
    }
}

/// Error parsing a Git platform name.
#[derive(Debug, Error)]
#[error("`{0}` is not a valid git platform (expected `github`, `gitlab`, or `bitbucket`)")]
pub struct GitPlatformError(String);

/// A parsed `owner/repo` hosted Git shorthand.
struct HostedGitShorthand {
    /// The repository owner or organization.
    owner: String,
    /// The repository name.
    repo: String,
}

impl FromStr for HostedGitShorthand {
    type Err = HostedGitShorthandError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        let mut parts = source.split('/');
        let owner = parts.next().ok_or(HostedGitShorthandError)?;
        let repo = parts.next().ok_or(HostedGitShorthandError)?;
        if parts.next().is_some()
            || owner.is_empty()
            || repo.is_empty()
            || source.starts_with('.')
            || source.starts_with('/')
            || owner == "."
            || owner == ".."
            || repo == "."
            || repo == ".."
        {
            return Err(HostedGitShorthandError);
        }
        Ok(Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// An error parsing a hosted Git shorthand.
struct HostedGitShorthandError;

/// Removes a trailing `.git` suffix from a repository name.
fn strip_git_suffix(name: &str) -> &str {
    name.strip_suffix(".git").unwrap_or(name)
}

/// Threshold for the large-file warning emitted at sign- and fetch-time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// Policy for accepting signer keys.
///
/// When the resolver encounters a signed module whose signer key is not
/// yet recorded in the lockfile, this setting controls whether the key
/// may be accepted automatically or requires explicit user confirmation. The
/// library computes a [`LockfileDiff`](super::lock::LockfileDiff) that
/// flags new signers; the CLI is responsible for acting on the policy
/// (e.g., prompting the user when `Confirm` is set).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Toml)]
#[toml(Toml, rename_all = "lowercase")]
pub enum TrustMode {
    /// Signer keys may be recorded without prompting when a caller
    /// explicitly opts into automatic trust.
    Auto,
    /// Signer keys may be recorded without prompting when a caller
    /// explicitly opts into trusting first observed keys.
    Tofu,
    /// The CLI must prompt the user to confirm signer keys before
    /// writing the lockfile.
    #[default]
    Confirm,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::DependencyScope;

    #[test]
    fn parses_default_threshold_when_absent() {
        let cfg: ModulesConfig = toml_spanner::from_str("").unwrap();
        assert!(matches!(
            cfg.large_file_warning,
            LargeFileWarning::Threshold(b) if b == 1024 * 1024
        ));
    }

    #[test]
    fn parses_default_git_platform() {
        let cfg: ModulesConfig =
            toml_spanner::from_str(r#"default_git_platform = "gitlab""#).unwrap();
        assert_eq!(cfg.default_git_platform, GitPlatform::Gitlab);
    }

    #[test]
    fn parses_trust_modes() {
        let cfg: ModulesConfig = toml_spanner::from_str(r#"trust_mode = "confirm""#).unwrap();
        assert_eq!(cfg.trust_mode, TrustMode::Confirm);

        let cfg: ModulesConfig = toml_spanner::from_str(r#"trust_mode = "auto""#).unwrap();
        assert_eq!(cfg.trust_mode, TrustMode::Auto);

        let cfg: ModulesConfig = toml_spanner::from_str(r#"trust_mode = "tofu""#).unwrap();
        assert_eq!(cfg.trust_mode, TrustMode::Tofu);
    }

    #[test]
    fn expands_hosted_git_shorthand() {
        let url = GitPlatform::Bitbucket
            .expand_shorthand("stjudecloud/workflows.git")
            .and_then(Result::ok);
        assert_eq!(
            url.as_ref().map(url::Url::as_str),
            Some("https://bitbucket.org/stjudecloud/workflows.git")
        );
        assert_eq!(
            GitPlatform::shorthand_repo_name("stjudecloud/workflows.git").as_deref(),
            Some("workflows")
        );
        assert!(
            GitPlatform::Github
                .expand_shorthand("./stjudecloud/workflows")
                .is_none()
        );
    }

    #[test]
    fn parses_size_string() {
        let cfg: ModulesConfig = toml_spanner::from_str(r#"large_file_warning = "5MiB""#).unwrap();
        assert!(matches!(
            cfg.large_file_warning,
            LargeFileWarning::Threshold(b) if b == 5 * 1024 * 1024
        ));
    }

    #[test]
    fn parses_none_sentinel() {
        for s in ["none", "NONE", "None"] {
            let cfg: ModulesConfig =
                toml_spanner::from_str(&format!(r#"large_file_warning = "{s}""#)).unwrap();
            assert!(matches!(cfg.large_file_warning, LargeFileWarning::Disabled));
        }
    }

    #[test]
    fn rejects_invalid_size_string() {
        let err =
            toml_spanner::from_str::<ModulesConfig>(r#"large_file_warning = "abc""#).unwrap_err();
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
