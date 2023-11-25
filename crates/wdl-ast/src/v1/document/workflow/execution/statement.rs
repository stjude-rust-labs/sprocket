//! Statements.

use grammar::v1::Rule;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;

use crate::v1::document::private_declarations;
use crate::v1::document::private_declarations::PrivateDeclarations;

pub mod call;
pub mod conditional;
pub mod scatter;

pub use call::Call;
pub use conditional::Conditional;
pub use scatter::Scatter;

/// An error related to a [`Statement`].
#[derive(Debug)]
pub enum Error {
    /// A workflow call error.
    Call(call::Error),

    /// A conditional error.
    Conditional(Box<conditional::Error>),

    /// A workflow execution statement had no children when one was expected.
    MissingChildren,

    /// A workflow execution statement has more than one child node when only
    /// one is expected.
    MultipleChildren,

    /// A private declarations error.
    PrivateDeclarations(private_declarations::Error),

    /// A scatter error.
    Scatter(Box<scatter::Error>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Call(err) => write!(f, "call error: {err}"),
            Error::Conditional(err) => write!(f, "conditional error: {err}"),
            Error::MissingChildren => {
                write!(f, "no children found for workflow execution statement")
            }
            Error::MultipleChildren => write!(
                f,
                "multiple children found for workflow execution statement"
            ),
            Error::PrivateDeclarations(err) => write!(f, "private declarations error: {err}"),
            Error::Scatter(err) => write!(f, "scatter error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A workflow execution statement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Statement {
    /// A conditional statement.
    Conditional(Conditional),

    /// A scatter statement.
    Scatter(Scatter),

    /// A function call statement.
    Call(Call),

    /// A set of private declarations.
    PrivateDeclarations(PrivateDeclarations),
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Statement {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self, Self::Error> {
        check_node!(node, workflow_execution_statement);
        let mut children = node.into_inner();

        let node = match children.len() {
            0 => return Err(Error::MissingChildren),
            // SAFETY: we ensure by checking the length that this will always
            // unwrap.
            1 => children.next().unwrap(),
            _n => return Err(Error::MultipleChildren),
        };

        match node.as_rule() {
            Rule::workflow_conditional => {
                let conditional =
                    Conditional::try_from(node).map_err(|err| Error::Conditional(Box::new(err)))?;
                Ok(Statement::Conditional(conditional))
            }
            Rule::workflow_scatter => {
                let scatter =
                    Scatter::try_from(node).map_err(|err| Error::Scatter(Box::new(err)))?;
                Ok(Statement::Scatter(scatter))
            }
            Rule::workflow_call => {
                let call = Call::try_from(node).map_err(Error::Call)?;
                Ok(Statement::Call(call))
            }
            Rule::private_declarations => {
                let declarations =
                    PrivateDeclarations::try_from(node).map_err(Error::PrivateDeclarations)?;
                Ok(Statement::PrivateDeclarations(declarations))
            }
            rule => unreachable!("workflow execution statement should not contain {:?}", rule),
        }
    }
}
