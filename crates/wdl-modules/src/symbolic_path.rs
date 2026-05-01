//! Symbolic-module-path parsing.

use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use thiserror::Error;
use wdl_grammar::lexer::v1::is_ident;

use crate::DependencyName;

/// An error parsing a [`SymbolicPath`].
#[derive(Debug, Error)]
#[error(
    "symbolic module path `{0}` does not match `<dep-name>[/<sub-path>]` with non-empty, \
     non-`.`/`..` segments"
)]
pub struct SymbolicPathError(String);

/// A symbolic module path.
///
/// The string form is `<dep-name>[/<sub-path>]`. The `<dep-name>` is the
/// key declared under `dependencies` in the consumer's `module.json`; the
/// optional `<sub-path>` addresses a specific module within a multi-module
/// dependency source. Path components are case-sensitive.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SymbolicPath {
    /// The dependency name component.
    dep_name: DependencyName,
    /// The optional sub-path within the dependency source.
    sub_path: Option<PathBuf>,
}

impl SymbolicPath {
    /// Returns the dependency name component.
    pub fn dep_name(&self) -> &DependencyName {
        &self.dep_name
    }

    /// Returns the sub-path component, if present.
    pub fn sub_path(&self) -> Option<&Path> {
        self.sub_path.as_deref()
    }
}

/// Validates that a string matches `<dep-name>[/<sub-path>]` where every
/// component (the dep name and each sub-path segment) is a WDL identifier.
fn validate(s: String) -> Result<SymbolicPath, SymbolicPathError> {
    let mut iter = s.split('/');
    // SAFETY: `str::split` always yields at least one item, even on the
    // empty string.
    let head = iter.next().unwrap();

    let dep_name =
        DependencyName::try_from(head.to_string()).map_err(|_| SymbolicPathError(s.clone()))?;

    let mut sub_path = PathBuf::new();
    let mut has_tail = false;
    for segment in iter {
        if !is_ident(segment) {
            return Err(SymbolicPathError(s));
        }
        sub_path.push(segment);
        has_tail = true;
    }

    Ok(SymbolicPath {
        dep_name,
        sub_path: if has_tail { Some(sub_path) } else { None },
    })
}

impl TryFrom<String> for SymbolicPath {
    type Error = SymbolicPathError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        validate(s)
    }
}

impl FromStr for SymbolicPath {
    type Err = SymbolicPathError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_string())
    }
}

impl fmt::Display for SymbolicPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.dep_name.inner())?;
        if let Some(sub) = &self.sub_path {
            for component in sub.iter() {
                f.write_str("/")?;
                f.write_str(&component.to_string_lossy())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dep_only() {
        let p: SymbolicPath = "spellbook".parse().unwrap();
        assert_eq!(p.dep_name().inner(), "spellbook");
        assert!(p.sub_path().is_none());
    }

    #[test]
    fn parses_with_sub_path() {
        let p: SymbolicPath = "spellbook/cauldron".parse().unwrap();
        assert_eq!(p.dep_name().inner(), "spellbook");
        assert_eq!(p.sub_path().unwrap(), Path::new("cauldron"));
    }

    #[test]
    fn parses_multi_segment_sub_path() {
        let p: SymbolicPath = "spellbook/cauldron/runes".parse().unwrap();
        assert_eq!(p.sub_path().unwrap(), Path::new("cauldron/runes"));
    }

    #[test]
    fn rejects_invalid_format() {
        for bad in [
            "spellbook/",
            "spellbook//cauldron",
            "spellbook/cauldron/",
            "spellbook/..",
            "spellbook/.",
            "1spellbook/cauldron",
            "spellbook/has-dash",       // non-identifier sub-path segment
            "spellbook/has space",      // whitespace
            "spellbook/cauldron.runes", // non-identifier
        ] {
            assert!(bad.parse::<SymbolicPath>().is_err(), "accepted `{bad}`");
        }
    }

    #[test]
    fn case_sensitive() {
        let lower: SymbolicPath = "spellbook/cauldron".parse().unwrap();
        let mixed: SymbolicPath = "spellbook/Cauldron".parse().unwrap();
        assert_ne!(lower.sub_path(), mixed.sub_path());
    }

    #[test]
    fn round_trips_via_display() {
        for s in [
            "spellbook",
            "spellbook/cauldron",
            "spellbook/cauldron/runes",
        ] {
            let p: SymbolicPath = s.parse().unwrap();
            assert_eq!(p.to_string(), s);
        }
    }
}
