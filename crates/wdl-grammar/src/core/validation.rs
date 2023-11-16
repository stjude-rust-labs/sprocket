//! Validation.

use pest::iterators::Pairs;
use pest::RuleType;
use to_snake_case::ToSnakeCase as _;

pub mod error;
pub mod validator;

pub use error::Error;
pub use validator::Validator;

use crate::core::Code;

/// A [`Result`](std::result::Result) with a validation [`Error`].
pub type Result = std::result::Result<(), Error>;

/// A validation rule.
pub trait Rule<R: RuleType>: std::fmt::Debug {
    /// The name of the validation rule.
    ///
    /// This is what will show up in style guides, it is required to be snake
    /// case (even though the rust struct is camel case).
    fn name(&self) -> String {
        format!("{:?}", self).to_snake_case()
    }

    /// Get the code for this validation rule.
    fn code(&self) -> Code;

    /// Checks the parse tree according to the implemented validation rule.
    ///
    /// **Note:** it would be much better to pass a reference to the parse tree
    /// (`&Pairs<'a, R>`) here to avoid unnecessary cloning of the tree.
    /// Unfortunately, the [`Pest`](https://pest.rs) library does not support a
    /// reference to [`Pairs`] being turned into an iterator at the moment.
    fn validate(&self, tree: Pairs<'_, R>) -> Result;
}
