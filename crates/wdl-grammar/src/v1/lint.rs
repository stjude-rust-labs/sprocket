//! Lint rules for WDL 1.x.

mod whitespace;

pub use whitespace::Whitespace;

use crate::core::lint::Rule;
use crate::v1;

/// Gets all lint rules available for WDL 1.x.
pub fn rules() -> Vec<Box<dyn Rule<v1::Rule>>> {
    vec![Box::new(Whitespace)]
}
