//! Lint rules for WDL 1.x parse trees.

use pest::iterators::Pair;

mod mixed_indentation;
mod no_curly_commands;
mod whitespace;

pub use mixed_indentation::MixedIndentation;
pub use no_curly_commands::NoCurlyCommands;
pub use whitespace::Whitespace;

/// Gets all WDL v1.x parse tree lint rules.
pub fn rules<'a>() -> Vec<Box<dyn wdl_core::concern::lint::Rule<&'a Pair<'a, crate::v1::Rule>>>> {
    vec![
        // v1::W001
        Box::new(Whitespace),
        // v1::W002
        Box::new(NoCurlyCommands),
        // v1::W004
        Box::new(MixedIndentation),
    ]
}
