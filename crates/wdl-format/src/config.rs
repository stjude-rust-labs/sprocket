//! Formatting configuration.

mod indent;
mod max_line_length;
mod newline;

pub use indent::Indent;
pub use max_line_length::MaxLineLength;
pub use newline::NewlineStyle;
use toml_spanner::Toml;
use toml_spanner::helper::display;
use toml_spanner::helper::parse_string;

/// Default for whether import sorting is enabled.
const SORT_IMPORTS_DEFAULT: bool = true;
/// Default for whether input sorting is enabled.
const SORT_INPUTS_DEFAULT: bool = false;
/// Default for whether trailing commas are enabled.
const TRAILING_COMMAS_DEFAULT: bool = true;

/// Configuration for formatting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Toml)]
#[toml(Toml, deny_unknown_fields)]
pub struct Config {
    /// The indentation configuration.
    #[toml(default)]
    pub indent: Indent,
    /// The maximum line length.
    #[toml(default)]
    pub max_line_length: MaxLineLength,
    /// Whether to sort import statements alphabetically.
    #[toml(default = SORT_IMPORTS_DEFAULT)]
    pub sort_imports: bool,
    /// Whether to sort input sections.
    #[toml(default = SORT_INPUTS_DEFAULT)]
    pub sort_inputs: bool,
    /// Whether to add trailing commas to multiline lists.
    #[toml(default = TRAILING_COMMAS_DEFAULT)]
    pub trailing_commas: bool,
    /// The newline style.
    #[toml(default, FromToml with = parse_string, ToToml with = display)]
    pub newline_style: NewlineStyle,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            indent: Indent::default(),
            max_line_length: MaxLineLength::default(),
            sort_imports: SORT_IMPORTS_DEFAULT,
            sort_inputs: SORT_INPUTS_DEFAULT,
            trailing_commas: TRAILING_COMMAS_DEFAULT,
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

    /// Set whether import sorting is enabled.
    pub fn sort_imports(mut self, sort_imports: bool) -> Self {
        self.sort_imports = sort_imports;
        self
    }

    /// Set whether input sorting is enabled.
    pub fn sort_inputs(mut self, sort_inputs: bool) -> Self {
        self.sort_inputs = sort_inputs;
        self
    }

    /// Set whether trailing commas are enabled.
    pub fn trailing_commas(mut self, trailing_commas: bool) -> Self {
        self.trailing_commas = trailing_commas;
        self
    }
}
