//! Lint rules for WDL 1.x.

mod no_curly_commands;
mod whitespace;

pub use no_curly_commands::NoCurlyCommands;
pub use whitespace::Whitespace;

use crate::core::lint::Rule;
use crate::v1;

/// Gets all lint rules available for WDL 1.x.
pub fn rules() -> Vec<Box<dyn Rule<v1::Rule>>> {
    vec![Box::new(Whitespace), Box::new(NoCurlyCommands)]
}
