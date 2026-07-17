//! Formatting configuration.

mod indent;
mod max_line_length;
mod newline;

pub use indent::Indent;
pub use max_line_length::MaxLineLength;
pub use newline::NewlineStyle;
use schemars::JsonSchema;
use toml_spanner::Toml;
use toml_spanner::helper::display;
use toml_spanner::helper::parse_string;

/// Default for whether import sorting is enabled.
fn sort_imports_default() -> bool {
    true
}

/// Default for whether input sorting is enabled.
fn sort_inputs_default() -> bool {
    false
}

/// Default for whether trailing commas are enabled.
fn trailing_commas_default() -> bool {
    true
}

/// Configuration for formatting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Toml, JsonSchema)]
#[toml(Toml, deny_unknown_fields)]
pub struct Config {
    /// The indentation configuration.
    #[toml(default)]
    #[schemars(default)]
    pub indent: Indent,
    /// The maximum line length.
    #[toml(default)]
    #[schemars(default)]
    pub max_line_length: MaxLineLength,
    /// Whether to sort import statements alphabetically.
    #[toml(default = sort_imports_default())]
    #[schemars(default = "sort_imports_default")]
    pub sort_imports: bool,
    /// Whether to sort input sections.
    #[toml(default = sort_inputs_default())]
    #[schemars(default = "sort_inputs_default")]
    pub sort_inputs: bool,
    /// Whether to add trailing commas to multiline lists.
    #[toml(default = trailing_commas_default())]
    #[schemars(default = "trailing_commas_default")]
    pub trailing_commas: bool,
    /// The newline style.
    #[toml(default, FromToml with = parse_string, ToToml with = display)]
    #[schemars(default)]
    pub newline_style: NewlineStyle,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            indent: Indent::default(),
            max_line_length: MaxLineLength::default(),
            sort_imports: sort_imports_default(),
            sort_inputs: sort_inputs_default(),
            trailing_commas: trailing_commas_default(),
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
