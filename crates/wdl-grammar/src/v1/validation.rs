//! Validation rules for WDL 1.x.

mod invalid_escape_character;

pub use invalid_escape_character::InvalidEscapeCharacter;

use crate::core::validation::Rule;
use crate::v1;

/// Gets all validation rules available for WDL 1.x.
pub fn rules() -> Vec<Box<dyn Rule<v1::Rule>>> {
    vec![Box::new(InvalidEscapeCharacter)]
}
