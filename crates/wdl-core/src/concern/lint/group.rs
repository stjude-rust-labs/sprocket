//! Lint groups.

use serde::Deserialize;
use serde::Serialize;

/// A lint group.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Group {
    /// Rules associated with having a complete document.
    Completeness,

    /// Rules associated with the names of WDL elements.
    Naming,

    /// Rules associated with the whitespace in a document.
    Spacing,

    /// Rules associated with the style of a document.
    Style,

    /// Rules often considered overly opinionated.
    ///
    /// These rules are disabled by default but can be turned on individually.
    Pedantic,
}

impl std::fmt::Display for Group {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Group::Completeness => write!(f, "Completeness"),
            Group::Naming => write!(f, "Naming"),
            Group::Spacing => write!(f, "Spacing"),
            Group::Style => write!(f, "Style"),
            Group::Pedantic => write!(f, "Pedantic"),
        }
    }
}
