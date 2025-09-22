//! Formatting configuration.

mod builder;
mod indent;
mod max_line_length;

pub use builder::Builder;
pub use indent::Indent;
pub use max_line_length::MaxLineLength;

/// Configuration for formatting.
#[derive(Clone, Copy, Debug, Default)]
pub struct Config {
    /// The indentation configuration.
    indent: Indent,
    /// The maximum line length.
    max_line_length: MaxLineLength,
}

impl Config {
    /// Gets the indentation configuration.
    pub fn indent(&self) -> Indent {
        self.indent
    }

    /// Gets the maximum line length of the configuration.
    pub fn max_line_length(&self) -> Option<usize> {
        self.max_line_length.get()
    }
}
