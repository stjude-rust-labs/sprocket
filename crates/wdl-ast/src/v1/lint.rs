//! Lint rules for WDL 1.x abstract syntax trees.

use crate::v1;

mod matching_parameter_meta;

pub use matching_parameter_meta::MatchingParameterMeta;

/// Gets all WDL v1.x abstract syntax tree lint rules.
pub fn rules<'a>() -> Vec<Box<dyn wdl_core::concern::lint::Rule<&'a v1::Document>>> {
    vec![
        // v1::W003
        Box::new(MatchingParameterMeta),
    ]
}
