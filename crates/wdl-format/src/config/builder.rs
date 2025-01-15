//! Builders for formatting configuration.

use crate::Config;
use crate::config::Indent;
use crate::config::MaxLineLength;

/// A builder for a [`Config`].
#[derive(Default)]
pub struct Builder {
    /// The number of characters to indent.
    indent: Option<Indent>,
    /// The maximum line length.
    max_line_length: Option<MaxLineLength>,
}

impl Builder {
    /// Sets the indentation level.
    ///
    /// This silently overwrites any previously provided value for the
    /// indentation level.
    pub fn indent(mut self, indent: Indent) -> Self {
        self.indent = Some(indent);
        self
    }

    /// Sets the maximum line length.
    ///
    /// This silently overwrites any previously provided value for the maximum
    /// line length.
    pub fn max_line_length(mut self, max_line_length: MaxLineLength) -> Self {
        self.max_line_length = Some(max_line_length);
        self
    }

    /// Consumes `self` to build a [`Config`].
    pub fn build(self) -> Config {
        let indent = self.indent.unwrap_or_default();
        let max_line_length = self.max_line_length.unwrap_or_default();
        Config {
            indent,
            max_line_length,
        }
    }
}
