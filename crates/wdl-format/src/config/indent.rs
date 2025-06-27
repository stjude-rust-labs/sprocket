//! Indentation within formatting configuration.

use crate::SPACE;
use crate::TAB;

/// The default number of spaces to represent one indentation level.
const DEFAULT_SPACE_INDENT: usize = 4;
/// The default indentation.
pub const DEFAULT_INDENT: Indent = Indent::Spaces(DEFAULT_SPACE_INDENT);
/// The maximum number of spaces to represent one indentation level.
pub const MAX_SPACE_INDENT: usize = 16;

/// An indentation level.
#[derive(Clone, Copy, Debug)]
pub enum Indent {
    /// Tabs.
    Tabs,
    /// Spaces.
    Spaces(usize),
}

impl Default for Indent {
    fn default() -> Self {
        DEFAULT_INDENT
    }
}

impl Indent {
    /// Attempts to create a new indentation level configuration.
    pub fn try_new(tab: bool, num_spaces: Option<usize>) -> Result<Self, String> {
        match (tab, num_spaces) {
            (true, None) => Ok(Indent::Tabs),
            (true, Some(_)) => {
                Err("Indentation with tabs cannot have a number of spaces".to_string())
            }
            (false, Some(n)) => {
                if n > MAX_SPACE_INDENT {
                    Err(format!(
                        "Indentation with spaces cannot have more than {MAX_SPACE_INDENT} \
                         characters"
                    ))
                } else {
                    Ok(Indent::Spaces(n))
                }
            }
            (false, None) => Ok(Indent::Spaces(DEFAULT_SPACE_INDENT)),
        }
    }

    /// Gets the number of characters to indent.
    pub fn num(&self) -> usize {
        match self {
            Indent::Tabs => 1,
            Indent::Spaces(n) => *n,
        }
    }

    /// Gets the character used for indentation.
    pub fn character(&self) -> &str {
        match self {
            Indent::Tabs => TAB,
            Indent::Spaces(_) => SPACE,
        }
    }

    /// Gets the string representation of the indentation.
    pub fn string(&self) -> String {
        match self {
            Indent::Tabs => self.character().to_string(),
            Indent::Spaces(n) => self.character().repeat(*n),
        }
    }
}
