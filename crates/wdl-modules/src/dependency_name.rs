//! Dependency-name newtype enforcing the WDL identifier rule.

use std::fmt;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

/// An error parsing a [`DependencyName`].
#[derive(Debug, Error, PartialEq, Eq)]
#[error("dependency name `{0}` does not match `[A-Za-z][A-Za-z0-9_]*`")]
pub struct DependencyNameError(String);

/// A dependency name.
///
/// Dependency names are WDL identifiers. They begin with an ASCII letter
/// and continue with ASCII letters, digits, or underscores. The same rule
/// governs the keys of `dependencies` in [`Manifest`](crate::Manifest), the
/// top-level keys of [`Lockfile::dependencies`](crate::Lockfile), and the
/// `<dep-name>` portion of a [`SymbolicPath`](crate::SymbolicPath).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct DependencyName(String);

impl DependencyName {
    /// Returns the dependency name as a string slice.
    pub fn inner(&self) -> &str {
        &self.0
    }

    /// Consumes the [`DependencyName`] and returns the inner [`String`].
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for DependencyName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for DependencyName {
    type Error = DependencyNameError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if wdl_grammar::lexer::v1::is_ident(&s) {
            Ok(Self(s))
        } else {
            Err(DependencyNameError(s))
        }
    }
}

impl FromStr for DependencyName {
    type Err = DependencyNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_string())
    }
}

impl AsRef<str> for DependencyName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_names() {
        for name in ["a", "spellbook", "spell_book", "Spell2", "X_1_2_3"] {
            assert!(name.parse::<DependencyName>().is_ok(), "rejected `{name}`");
        }
    }

    #[test]
    fn rejects_invalid_format() {
        for bad in [
            "",
            "1spellbook",
            "_spellbook",
            "spell-book",
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
        let name: DependencyName = "spellbook".parse().unwrap();
        let json = serde_json::to_string(&name).unwrap();
        assert_eq!(json, r#""spellbook""#);
        let parsed: DependencyName = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, name);
    }

    #[test]
    fn deserialize_rejects_invalid() {
        let err = serde_json::from_str::<DependencyName>(r#""1spellbook""#).unwrap_err();
        assert!(err.to_string().contains("does not match"));
    }
}
