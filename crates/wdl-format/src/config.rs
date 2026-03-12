//! Formatting configuration.

mod indent;
mod max_line_length;

pub use indent::Indent;
pub use max_line_length::MaxLineLength;
use serde::Deserialize;
use serde::Serialize;

/// Default for whether import sorting is enabled.
const SORT_IMPORTS_DEFAULT: bool = true;
/// Default for whether input sorting is enabled.
const SORT_INPUTS_DEFAULT: bool = false;
/// Default for whether trailing commas are enabled.
const TRAILING_COMMAS_DEFAULT: bool = true;
/// Default for whether doc comment normalization (Markdown formatting) is
/// enabled.
const NORMALIZE_DOC_COMMENTS_DEFAULT: bool = true;

/// Configuration for formatting.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// The indentation configuration.
    pub indent: Indent,
    /// The maximum line length.
    pub max_line_length: MaxLineLength,
    /// Whether to sort import statements alphabetically.
    pub sort_imports: bool,
    /// Whether to sort input sections.
    pub sort_inputs: bool,
    /// Whether to add trailing commas to multiline lists.
    pub trailing_commas: bool,
    /// Whether to normalize (reformat) embedded Markdown in doc comments
    /// (`##`).
    ///
    /// When `true` (the default), doc comment blocks are parsed as Markdown and
    /// reflowed to respect the configured maximum line length. Set to `false`
    /// to preserve doc comment text exactly as written.
    pub normalize_doc_comments: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            indent: Indent::default(),
            max_line_length: MaxLineLength::default(),
            sort_imports: SORT_IMPORTS_DEFAULT,
            sort_inputs: SORT_INPUTS_DEFAULT,
            trailing_commas: TRAILING_COMMAS_DEFAULT,
            normalize_doc_comments: NORMALIZE_DOC_COMMENTS_DEFAULT,
        }
    }
}

impl Config {
    /// Overwrite the indentation configuration.
    pub fn indent(mut self, indent: Indent) -> Self {
        self.indent = indent;
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

    /// Set whether doc comment normalization is enabled.
    pub fn normalize_doc_comments(mut self, normalize_doc_comments: bool) -> Self {
        self.normalize_doc_comments = normalize_doc_comments;
        self
    }
}
