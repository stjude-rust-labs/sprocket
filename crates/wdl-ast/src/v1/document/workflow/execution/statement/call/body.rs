//! Call bodies.

use std::collections::BTreeMap;

use grammar::v1::Rule;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;

use crate::v1::document::expression;
use crate::v1::document::identifier::singular;
use crate::v1::document::Expression;

mod value;

pub use value::Value;

/// An error related to a [`Body`].
#[derive(Debug)]
pub enum Error {
    /// An expression error.
    Expression(expression::Error),

    /// An identifier error.
    Identifier(singular::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Expression(err) => write!(f, "expression error: {err}"),
            Error::Identifier(err) => write!(f, "identifier error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// A body for a [`Call`](super::Call).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Body(BTreeMap<singular::Identifier, Value>);

impl std::ops::Deref for Body {
    type Target = BTreeMap<singular::Identifier, Value>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<BTreeMap<singular::Identifier, Value>> for Body {
    fn from(body: BTreeMap<singular::Identifier, Value>) -> Self {
        Self(body)
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Body {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self> {
        check_node!(node, workflow_call_body);

        let mut body = BTreeMap::new();

        let nodes = node
            .into_inner()
            .filter(|node| matches!(node.as_rule(), Rule::workflow_call_input))
            .collect::<Vec<_>>();

        for node in nodes {
            let inner = node
                .into_inner()
                .filter(|node| {
                    !matches!(node.as_rule(), Rule::WHITESPACE)
                        && !matches!(node.as_rule(), Rule::COMMENT)
                })
                .collect::<Vec<_>>();

            if inner.len() != 1 && inner.len() != 2 {
                unreachable!(
                    "invalid number of nodes for workflow call input: {}",
                    inner.len()
                );
            }

            let mut nodes = inner.into_iter();

            // SAFETY: we just checked above that at least one node exists.
            let identifier_node = nodes.next().unwrap();
            let identifier =
                singular::Identifier::try_from(identifier_node).map_err(Error::Identifier)?;

            let value = match nodes.next() {
                Some(node) => match node.as_rule() {
                    Rule::expression => {
                        Value::Expression(Expression::try_from(node).map_err(Error::Expression)?)
                    }
                    rule => unreachable!("workflow call input value should not contain {:?}", rule),
                },
                None => Value::ImplicitBinding,
            };

            body.insert(identifier, value);
        }

        Ok(Body(body))
    }
}

#[cfg(test)]
mod tests {
    use wdl_macros::test::create_invalid_node_test;
    use wdl_macros::test::valid_node;

    use super::*;
    use crate::v1::document::expression::Literal;

    #[test]
    fn it_parses_from_a_supported_node_type() {
        let body = valid_node!(r#"{input: x, y=true, z=beta}"#, workflow_call_body, Body);
        assert_eq!(body.get("x"), Some(&Value::ImplicitBinding));
        assert_eq!(
            body.get("y"),
            Some(&Value::Expression(Expression::Literal(Literal::Boolean(
                true
            ))))
        );
        assert_eq!(
            body.get("z"),
            Some(&Value::Expression(Expression::Literal(
                Literal::Identifier(singular::Identifier::try_from("beta").unwrap())
            )))
        );
        assert_eq!(body.get("q"), None);
    }

    create_invalid_node_test!(
        "version 1.1\n\ntask hello { command <<<>>> }",
        document,
        workflow_call_body,
        Body,
        it_fails_to_parse_from_an_unsupported_node_type
    );
}
