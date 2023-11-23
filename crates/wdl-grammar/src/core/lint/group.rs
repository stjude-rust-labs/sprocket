//! Lint groups.

/// A lint group.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Group {
    /// Rules associated with the style of an input.
    Style,

    /// Rules often considered overly opinionated.
    Pedantic,
}

impl std::fmt::Display for Group {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Group::Style => write!(f, "Style"),
            Group::Pedantic => write!(f, "Pedantic"),
        }
    }
}
