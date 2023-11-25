//! Lint levels.

use serde::Deserialize;
use serde::Serialize;

/// A lint level.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Level {
    /// The lowest priority lint level.
    Low,

    /// A moderate lint level.
    Medium,

    /// The highest priority lint level.
    High,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order() {
        assert!(Level::Low < Level::Medium);
        assert!(Level::Medium < Level::High);
    }
}
