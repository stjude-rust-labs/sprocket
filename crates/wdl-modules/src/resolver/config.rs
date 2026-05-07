//! `[modules]` configuration parsed from `sprocket.toml`.

use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

/// The `[modules]` configuration section.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModulesConfig {
    /// Override the global cache location for this project.
    pub cache_path: Option<PathBuf>,

    /// Threshold for the large-file warning, or [`LargeFileWarning::Disabled`]
    /// when the user opts out. Defaults to 1 MiB.
    pub large_file_warning: LargeFileWarning,

    /// Reject any unsigned module in the dependency tree.
    pub require_signed: bool,

    /// TOFU policy for new signer keys.
    pub trust_mode: TrustMode,

    /// URL schemes permitted for top-level Git dependencies. Defaults
    /// to `["https", "ssh"]`.
    #[serde(default = "default_top_level_schemes")]
    pub allowed_schemes: Vec<String>,

    /// URL schemes permitted for transitive Git dependencies. Defaults
    /// to `["https"]` so remote manifests cannot silently trigger SSH
    /// authentication against an attacker-controlled host.
    #[serde(default = "default_transitive_schemes")]
    pub allowed_transitive_schemes: Vec<String>,

    /// Maximum number of advertised refs accepted from a remote.
    /// Defaults to 100,000.
    #[serde(default = "default_max_refs")]
    pub max_advertised_refs: usize,

    /// Hosts denied for all Git dependencies. Defaults to localhost
    /// addresses.
    #[serde(default = "default_denied_hosts")]
    pub denied_hosts: Vec<String>,

    /// Hosts permitted for top-level Git dependencies. Empty means any
    /// non-denied host is allowed.
    #[serde(default)]
    pub allowed_hosts: Vec<String>,

    /// Hosts permitted for transitive Git dependencies. Empty means
    /// any non-denied host is allowed.
    #[serde(default)]
    pub allowed_transitive_hosts: Vec<String>,

    /// Whether transitive dependencies may use configured Git
    /// credential helpers and ssh-agent. Defaults to `false`.
    #[serde(default)]
    pub allow_transitive_credentials: bool,

    /// Maximum number of files allowed in a single materialized module
    /// tree. `None` (the default) disables the limit. This is an
    /// opt-in safety valve; set it in `sprocket.toml` if your
    /// environment needs to bound resource consumption from untrusted
    /// dependencies.
    #[serde(default)]
    pub max_materialized_files: Option<usize>,

    /// Maximum total bytes of regular files allowed in a single
    /// materialized module tree. `None` (the default) disables the
    /// limit. Like `max_materialized_files`, this is opt-in.
    #[serde(default)]
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

/// Returns the default denied-host list.
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
            allowed_transitive_hosts: Vec::new(),
            allow_transitive_credentials: false,
            max_materialized_files: None,
            max_materialized_bytes: None,
        }
    }
}

impl ModulesConfig {
    /// Returns `true` if the given URL scheme is permitted for a
    /// dependency at this level of the tree.
    pub fn scheme_allowed(&self, scheme: &str, is_transitive: bool) -> bool {
        let allowed = if is_transitive {
            &self.allowed_transitive_schemes
        } else {
            &self.allowed_schemes
        };
        allowed.iter().any(|s| s.eq_ignore_ascii_case(scheme))
    }

    /// Returns `true` if the given host is permitted for a dependency
    /// at this level of the tree.
    pub fn host_allowed(&self, host: &str, is_transitive: bool) -> bool {
        if self
            .denied_hosts
            .iter()
            .any(|h| h.eq_ignore_ascii_case(host))
        {
            return false;
        }
        let allowed = if is_transitive {
            &self.allowed_transitive_hosts
        } else {
            &self.allowed_hosts
        };
        allowed.is_empty() || allowed.iter().any(|h| h.eq_ignore_ascii_case(host))
    }

    /// Returns `true` if Git credential helpers and ssh-agent may be
    /// used for a dependency at this level of the tree.
    pub fn credentials_allowed(&self, is_transitive: bool) -> bool {
        !is_transitive || self.allow_transitive_credentials
    }
}

/// Threshold for the large-file warning emitted at sign- and fetch-time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
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
        if s.eq_ignore_ascii_case("off") {
            return Ok(Self::Disabled);
        }
        let bytes = s
            .parse::<bytesize::ByteSize>()
            .map_err(|_| LargeFileWarningError(s.to_string()))?
            .as_u64();
        Ok(Self::Threshold(bytes))
    }
}

impl TryFrom<String> for LargeFileWarning {
    type Error = LargeFileWarningError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<LargeFileWarning> for String {
    fn from(v: LargeFileWarning) -> Self {
        match v {
            LargeFileWarning::Disabled => "off".to_string(),
            LargeFileWarning::Threshold(b) => bytesize::ByteSize(b).to_string(),
        }
    }
}

/// Error parsing a [`LargeFileWarning`] string.
#[derive(Debug, Error)]
#[error("`{0}` is not a valid file-size string (expected e.g. `1MiB`, `500KB`, or `off`)")]
pub struct LargeFileWarningError(String);

/// TOFU policy for new signer keys.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrustMode {
    /// New signer keys are recorded in the lockfile silently.
    #[default]
    Auto,
    /// The user is prompted to confirm any newly-trusted signer key.
    Confirm,
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parses_off_sentinel() {
        for s in ["off", "OFF", "Off"] {
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
        assert!(!cfg.host_allowed("localhost", false));
        assert!(!cfg.host_allowed("127.0.0.1", true));
        assert!(!cfg.host_allowed("::1", true));
        assert!(!cfg.host_allowed("0.0.0.0", false));
    }

    #[test]
    fn default_policy_allows_public_hosts() {
        let cfg = ModulesConfig::default();
        assert!(cfg.host_allowed("github.com", false));
        assert!(cfg.host_allowed("github.com", true));
    }

    #[test]
    fn allowlist_limits_transitive_hosts() {
        let cfg = ModulesConfig {
            allowed_transitive_hosts: vec!["github.com".into()],
            ..ModulesConfig::default()
        };
        assert!(cfg.host_allowed("github.com", true));
        assert!(!cfg.host_allowed("gitlab.com", true));
        assert!(cfg.host_allowed("gitlab.com", false));
    }

    #[test]
    fn transitive_credentials_disabled_by_default() {
        let cfg = ModulesConfig::default();
        assert!(!cfg.credentials_allowed(true));
        assert!(cfg.credentials_allowed(false));
    }

    #[test]
    fn transitive_credentials_enabled_when_configured() {
        let cfg = ModulesConfig {
            allow_transitive_credentials: true,
            ..ModulesConfig::default()
        };
        assert!(cfg.credentials_allowed(true));
    }
}
