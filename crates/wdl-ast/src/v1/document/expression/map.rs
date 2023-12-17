//! A map.

use std::collections::BTreeMap;

use grammar::v1::Rule;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;
use wdl_macros::dive_one;
use wdl_macros::unwrap_one;

use crate::v1::document::expression;
use crate::v1::document::Expression;

/// An error related to a [`Map`].
#[derive(Debug)]
pub enum Error {
    /// An expression error.
    Expression(Box<expression::Error>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Expression(err) => write!(f, "expression error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// The inner map within a [`Map`].
type Inner = BTreeMap<Expression, Expression>;

/// A map within an [`Expression`].
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Map(Inner);

impl std::ops::Deref for Map {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Map {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self> {
        check_node!(node, map_literal);

        let mut map = Inner::default();

        for node in node.into_inner() {
            match node.as_rule() {
                Rule::expression_based_kv_pair => {
                    //================//
                    // Key extraction //
                    //================//

                    // (1) Pull the `expression_based_kv_key` out of the
                    // `expression_based_kv_pair`.

                    // TODO: a clone is required here because Pest's `FlatPairs`
                    // type does not support creating an iterator without taking
                    // ownership (at the time of writing). This can be made
                    // better with a PR to Pest.
                    let key_node = dive_one!(
                        node.clone(),
                        expression_based_kv_key,
                        expression_based_kv_pair
                    );

                    // (2) Pull out the expression.
                    let key_expression_node = unwrap_one!(key_node, expression_based_kv_key);

                    // (3) Ensure that the node is an expression.
                    check_node!(key_expression_node, expression);

                    // (4) Parse the key expression.
                    let key = Expression::try_from(key_expression_node)
                        .map_err(|err| Error::Expression(Box::new(err)))?;

                    //==================//
                    // Value extraction //
                    //==================//

                    // (1) Pull the `kv_value` out of the
                    // `expression_based_kv_pair`.
                    let value_node = dive_one!(node, kv_value, expression_based_kv_pair);

                    // (2) Pull out the expression.
                    let value_expression_node = unwrap_one!(value_node, kv_value);

                    // (3) Ensure that the node is an expression.
                    check_node!(value_expression_node, expression);

                    // (4) Parse the key expression.
                    let value = Expression::try_from(value_expression_node)
                        .map_err(|err| Error::Expression(Box::new(err)))?;

                    map.insert(key, value);
                }
                Rule::WHITESPACE => {}
                Rule::COMMA => {}
                Rule::COMMENT => {}
                rule => {
                    unreachable!("map literals should not contain {:?}", rule)
                }
            }
        }

        Ok(Map(map))
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
        let map = valid_node!(r#"{"hello": "world"}"#, map_literal, Map);
        assert_eq!(
            map.get(&Expression::Literal(Literal::String(String::from("hello")))),
            Some(&Expression::Literal(Literal::String(String::from("world"))))
        );
    }

    create_invalid_node_test!(
        "version 1.1\n\ntask hello { command <<<>>> }",
        document,
        map_literal,
        Map,
        it_fails_to_parse_from_an_unsupported_node_type
    );
}
