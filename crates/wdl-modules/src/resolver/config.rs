//! `[modules]` configuration parsed from `sprocket.toml`.

use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

/// The `[modules]` configuration section.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModulesConfig {
    /// Override the global cache location for this project.
    pub cache_path: Option<PathBuf>,

    /// Threshold for the large-file warning, or [`LargeFileWarning::Disabled`]
    /// when the user opts out. Default: 1 MiB.
    pub large_file_warning: LargeFileWarning,

    /// Reject any unsigned module in the dependency tree.
    pub require_signed: bool,

    /// TOFU policy for new signer keys.
    pub trust_mode: TrustMode,
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
}
