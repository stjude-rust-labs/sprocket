//! Linting.

use pest::iterators::Pairs;
use pest::RuleType;
use to_snake_case::ToSnakeCase as _;

mod group;
mod level;
mod linter;
pub mod warning;

pub use group::Group;
pub use level::Level;
pub use linter::Linter;
pub use warning::Warning;

use crate::core::Code;

/// A [`Result`](std::result::Result) returned from a lint check.
pub type Result = std::result::Result<Option<Vec<Warning>>, Box<dyn std::error::Error>>;

/// A lint rule.
pub trait Rule<R: RuleType>: std::fmt::Debug {
    /// The name of the lint rule.
    ///
    /// This is what will show up in style guides, it is required to be snake
    /// case (even though the rust struct is camel case).
    fn name(&self) -> String {
        format!("{:?}", self).to_snake_case()
    }

    /// Get the code for this lint rule.
    fn code(&self) -> Code;

    /// Get the lint group for this lint rule.
    fn group(&self) -> Group;

    /// Checks the parse tree according to the implemented lint rule.
    ///
    /// **Note:** it would be much better to pass a reference to the parse tree
    /// (`&Pairs<'a, R>`) here to avoid unnecessary cloning of the tree.
    /// Unfortunately, the [`Pest`](https://pest.rs) library does not support a
    /// reference to [`Pairs`] being turned into an iterator at the moment.
    fn check(&self, tree: Pairs<'_, R>) -> Result;
}
