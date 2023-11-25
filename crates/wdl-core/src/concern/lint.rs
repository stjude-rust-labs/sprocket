//! Linting.

use std::collections::VecDeque;

use nonempty::NonEmpty;
use to_snake_case::ToSnakeCase as _;

use crate::concern::Code;
use crate::fs::location;

mod group;
mod level;
pub mod warning;

pub use group::Group;
pub use level::Level;
pub use warning::Warning;

/// An unrecoverable error that occurs during linting.
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

/// A [`Result`](std::result::Result) returned from a lint check.
pub type Result = std::result::Result<Option<NonEmpty<Warning>>, Error>;

/// A tree linter.
#[derive(Debug)]
pub struct Linter;

impl Linter {
    /// Lints a tree according to a set of lint rules and returns a
    /// set of lint warnings (if any are detected).
    pub fn lint<'a, E>(tree: &'a E, rules: Vec<Box<dyn Rule<&'a E>>>) -> Result {
        let mut warnings = rules
            .iter()
            .map(|rule| rule.check(tree))
            .collect::<std::result::Result<Vec<Option<NonEmpty<Warning>>>, Error>>()?
            .into_iter()
            .flatten()
            .flatten()
            .collect::<VecDeque<Warning>>();

        match warnings.pop_front() {
            Some(front) => {
                let mut result = NonEmpty::new(front);
                result.extend(warnings);
                Ok(Some(result))
            }
            None => Ok(None),
        }
    }
}

/// A lint rule.
pub trait Rule<E>: std::fmt::Debug + Sync {
    /// The name of the lint rule.
    fn name(&self) -> String {
        format!("{:?}", self).to_snake_case()
    }

    /// Get the code for this lint rule.
    fn code(&self) -> Code;

    /// Get the lint group for this lint rule.
    fn group(&self) -> Group;

    /// Checks the tree according to the implemented lint rule.
    fn check(&self, tree: E) -> Result;
}
