//! Dependency names and sources for `module.json`.

use std::fmt;
use std::hash::Hash;
use std::hash::Hasher;
use std::str::FromStr;

use serde_with::DeserializeFromStr;
use serde_with::SerializeDisplay;
use thiserror::Error;

mod source;

pub use source::DependencySource;
pub use source::DependencySourceError;
pub use source::GitModulePath;
pub use source::GitModulePathError;
pub use source::GitSelector;

/// An error parsing a [`DependencyName`].
#[derive(Debug, Error, PartialEq, Eq)]
#[error("dependency name `{0}` does not match `[A-Za-z][A-Za-z0-9_-]*`")]
pub struct DependencyNameError(String);

/// Returns `true` if `s` matches the dependency-name grammar
/// `[A-Za-z][A-Za-z0-9_-]*`.
fn is_dependency_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// A dependency name.
///
/// Dependency names begin with an ASCII letter and continue with ASCII
/// letters, digits, underscores, or hyphens. Following Cargo's
/// convention, hyphens and underscores are interchangeable: `spell-book`
/// and `spell_book` refer to the same dependency.
///
/// Two forms are stored: the **manifest** form preserves the exact
/// spelling from `module.json`, and the **identifier** form replaces
/// hyphens with underscores to produce a valid WDL identifier suitable
/// for use in symbolic imports. The identifier form must not be a
/// reserved keyword.
///
/// `Eq`, `Ord`, and `Hash` operate on the **identifier** form only,
/// so `spell-book` and `spell_book` are the same key in maps and
/// sets. This enforces the spec rule that hyphens and underscores are
/// interchangeable for the purpose of identity. Use
/// [`manifest()`](Self::manifest) when exact-spelling fidelity is
/// needed (e.g., serialization or display).
#[derive(Clone, Debug, SerializeDisplay, DeserializeFromStr)]
pub struct DependencyName {
    /// The name as written in `module.json`.
    manifest: String,
    /// The WDL identifier form (hyphens replaced with underscores).
    identifier: String,
}

impl PartialEq for DependencyName {
    fn eq(&self, other: &Self) -> bool {
        self.identifier == other.identifier
    }
}

impl Eq for DependencyName {}

impl PartialOrd for DependencyName {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DependencyName {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.identifier.cmp(&other.identifier)
    }
}

impl Hash for DependencyName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.identifier.hash(state);
    }
}

impl DependencyName {
    /// Returns the name as written in `module.json`.
    pub fn manifest(&self) -> &str {
        &self.manifest
    }

    /// Returns the WDL identifier form of the name (hyphens replaced
    /// with underscores).
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Consumes the [`DependencyName`] and returns the manifest form.
    pub fn into_manifest(self) -> String {
        self.manifest
    }

    /// Consumes the [`DependencyName`] and returns the identifier form.
    pub fn into_identifier(self) -> String {
        self.identifier
    }
}

impl TryFrom<&str> for DependencyName {
    type Error = DependencyNameError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        if !is_dependency_name(s) {
            return Err(DependencyNameError(s.to_string()));
        }

        let identifier = s.replace('-', "_");
        if !wdl_grammar::lexer::v1::is_ident(&identifier) {
            return Err(DependencyNameError(s.to_string()));
        }

        Ok(Self {
            manifest: s.to_string(),
            identifier,
        })
    }
}

impl FromStr for DependencyName {
    type Err = DependencyNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s)
    }
}

impl fmt::Display for DependencyName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.manifest.fmt(f)
    }
}

impl AsRef<str> for DependencyName {
    fn as_ref(&self) -> &str {
        &self.manifest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_names() {
        for name in [
            "a",
            "spellbook",
            "spell_book",
            "spell-book",
            "Spell2",
            "X_1_2_3",
            "my-crate",
        ] {
            assert!(name.parse::<DependencyName>().is_ok(), "rejected `{name}`");
        }
    }

    #[test]
    fn normalizes_hyphens_to_underscores() {
        let hyphen: DependencyName = "spell-book".parse().unwrap();
        let underscore: DependencyName = "spell_book".parse().unwrap();
        assert_eq!(hyphen.identifier(), "spell_book");
        assert_eq!(hyphen.manifest(), "spell-book");
        assert_eq!(underscore.manifest(), "spell_book");
    }

    #[test]
    fn hyphen_and_underscore_are_equal() {
        let hyphen: DependencyName = "spell-book".parse().unwrap();
        let underscore: DependencyName = "spell_book".parse().unwrap();
        assert_eq!(hyphen, underscore);
        assert_eq!(hyphen.cmp(&underscore), std::cmp::Ordering::Equal);
    }

    #[test]
    fn rejects_invalid_format() {
        for bad in [
            "",
            "1spellbook",
            "_spellbook",
            "-spellbook",
            "spell book",
            "spell.book",
            "spell/book",
        ] {
            assert!(bad.parse::<DependencyName>().is_err(), "accepted `{bad}`");
        }
    }

    #[test]
    fn rejects_reserved_keywords() {
        for bad in ["task", "workflow", "import", "if", "as"] {
            assert!(
                bad.parse::<DependencyName>().is_err(),
                "accepted reserved keyword `{bad}` as a dependency name"
            );
        }
    }

    #[test]
    fn round_trips_via_serde() {
        let name: DependencyName = "spell-book".parse().unwrap();
        let json = serde_json::to_string(&name).unwrap();
        assert_eq!(json, r#""spell-book""#);
        let parsed: DependencyName = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, name);
        assert_eq!(parsed.manifest(), "spell-book");
    }

    #[test]
    fn deserialize_rejects_invalid() {
        let err = serde_json::from_str::<DependencyName>(r#""1spellbook""#).unwrap_err();
        assert!(err.to_string().contains("does not match"));
    }
}
