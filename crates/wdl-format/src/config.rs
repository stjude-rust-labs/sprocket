//! Formatting configuration.

mod indent;
mod max_line_length;
mod newline;

pub use indent::Indent;
pub use max_line_length::MaxLineLength;
pub use newline::NewlineStyle;

/// Default for whether input sorting is enabled.
const SORT_INPUTS_DEFAULT: bool = false;

/// Configuration for formatting.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// The indentation configuration.
    pub indent: Indent,
    /// The maximum line length.
    pub max_line_length: MaxLineLength,
    /// Whether to sort input sections.
    pub sort_inputs: bool,
    /// The newline style.
    pub newline_style: NewlineStyle,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            indent: Indent::default(),
            max_line_length: MaxLineLength::default(),
            sort_inputs: SORT_INPUTS_DEFAULT,
            newline_style: NewlineStyle::default(),
        }
    }
}

impl Config {
    /// Overwrite the indentation configuration.
    pub fn indent(mut self, indent: Indent) -> Self {
        self.indent = indent;
        self
    }

    /// Set the newline style.
    pub fn newline_style(mut self, newline_style: NewlineStyle) -> Self {
        self.newline_style = newline_style;
        self
    }

    /// Overwrite the maximum line length configuration.
    pub fn max_line_length(mut self, max_line_length: MaxLineLength) -> Self {
        self.max_line_length = max_line_length;
        self
    }

    /// Set whether input sorting is enabled.
    pub fn sort_inputs(mut self, sort_inputs: bool) -> Self {
        self.sort_inputs = sort_inputs;
        self
    }
}
