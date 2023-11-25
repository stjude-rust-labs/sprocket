//! Validation rules for WDL 1.x parse trees.

use pest::iterators::Pair;

mod duplicate_runtime_keys;
mod invalid_escape_character;
mod invalid_version;

pub use duplicate_runtime_keys::DuplicateRuntimeKeys;
pub use invalid_escape_character::InvalidEscapeCharacter;
pub use invalid_version::InvalidVersion;

/// Gets all WDL v1.x parse tree validation rules.
pub fn rules<'a>(
) -> Vec<Box<dyn wdl_core::concern::validation::Rule<&'a Pair<'a, crate::v1::Rule>>>> {
    vec![
        // v1::E001
        Box::new(InvalidEscapeCharacter),
        // v1::E002
        Box::new(InvalidVersion),
        // v1::E003
        Box::new(DuplicateRuntimeKeys),
    ]
}
