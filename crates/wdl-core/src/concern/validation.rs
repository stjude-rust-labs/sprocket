//! Validation.

use std::collections::VecDeque;

use convert_case::Case;
use convert_case::Casing;
use nonempty::NonEmpty;

use crate::concern::Code;
use crate::file::location;

pub mod failure;

pub use failure::Failure;

/// An unrecoverable error that occurs during validation.
#[derive(Debug)]
pub enum Error {
    /// A location error.
    Location(location::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Location(err) => write!(f, "location error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) returned from a validation check.
pub type Result = std::result::Result<Option<NonEmpty<Failure>>, Error>;

/// A parse tree validator.
#[derive(Debug)]
pub struct Validator;

impl Validator {
    /// Validates a tree according to a set of validation rules.
    pub fn validate<'a, E>(tree: &'a E, rules: Vec<Box<dyn Rule<&'a E>>>) -> Result {
        let mut failures = rules
            .iter()
            .map(|rule| rule.validate(tree))
            .collect::<std::result::Result<Vec<Option<NonEmpty<Failure>>>, Error>>()?
            .into_iter()
            .flatten()
            .flatten()
            .collect::<VecDeque<Failure>>();

        match failures.pop_front() {
            Some(front) => {
                let mut result = NonEmpty::new(front);
                result.extend(failures);
                Ok(Some(result))
            }
            None => Ok(None),
        }
    }
}

/// A validation rule.
pub trait Rule<E>: std::fmt::Debug + Sync {
    /// The name of the validation rule.
    ///
    /// This is what will show up in style guides, it is required to be snake
    /// case (even though the rust struct is camel case).
    fn name(&self) -> String {
        format!("{:?}", self).to_case(Case::Snake)
    }

    /// Get the code for this validation rule.
    fn code(&self) -> Code;

    /// Checks the tree according to the implemented validation rule.
    fn validate(&self, tree: E) -> Result;
}
