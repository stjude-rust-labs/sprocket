//! Indentation within formatting configuration.

use std::num::NonZeroUsize;

/// The default indentation.
pub const DEFAULT_INDENT: Indent = Indent::Spaces(unsafe { NonZeroUsize::new_unchecked(4) });

/// An indentation level.
#[derive(Clone, Copy, Debug)]
pub enum Indent {
    /// Tabs.
    Tabs(NonZeroUsize),

    /// Spaces.
    Spaces(NonZeroUsize),
}

impl Default for Indent {
    fn default() -> Self {
        DEFAULT_INDENT
    }
}
