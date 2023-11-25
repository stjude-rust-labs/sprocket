//! Validation rules for WDL 1.x abstract syntax trees.

use crate::v1;

/// Gets all WDL v1.x abstract syntax tree validation rules.
pub fn rules<'a>() -> Vec<Box<dyn wdl_core::concern::validation::Rule<&'a v1::Document>>> {
    vec![]
}
