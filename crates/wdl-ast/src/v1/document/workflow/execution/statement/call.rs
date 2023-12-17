//! Calls.

use grammar::v1::Rule;
use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;
use wdl_macros::dive_one;
use wdl_macros::unwrap_one;

use crate::v1::document::identifier;
use crate::v1::document::identifier::singular;
use crate::v1::document::Identifier;

pub mod body;
mod builder;

pub use body::Body;
pub use builder::Builder;

/// An error rleated to a [`Call`].
#[derive(Debug)]
pub enum Error {
    /// A body error.
    Body(body::Error),

    /// A builder error.
    Builder(builder::Error),

    /// An identifier error.
    Identifier(identifier::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Body(err) => write!(f, "body error: {err}"),
            Error::Builder(err) => write!(f, "builder error: {err}"),
            Error::Identifier(err) => write!(f, "identifier error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// A call statement.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Call {
    /// The after clauses.
    afters: Option<NonEmpty<singular::Identifier>>,

    /// The body.
    body: Option<Body>,

    /// The name.
    name: Identifier,

    /// The as clause.
    r#as: Option<singular::Identifier>,
}

impl Call {
    /// Gets the after clauses from the [`Call`] by reference (if the exist).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::workflow::execution::statement::call::Builder;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let name = Identifier::from(singular::Identifier::try_from("foo")?);
    /// let after = singular::Identifier::try_from("baz")?;
    /// let call = Builder::default()
    ///     .name(name)?
    ///     .push_after(after.clone())
    ///     .try_build()?;
    ///
    /// assert_eq!(call.afters().unwrap().len(), 1);
    /// assert_eq!(call.afters().unwrap().iter().next().unwrap(), &after);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn afters(&self) -> Option<&NonEmpty<singular::Identifier>> {
        self.afters.as_ref()
    }

    /// Gets the body for this [`Call`] by reference (if it exists).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    ///
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::workflow::execution::statement::call::body::Value;
    /// use ast::v1::document::workflow::execution::statement::call::Body;
    /// use ast::v1::document::workflow::execution::statement::call::Builder;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let mut map = BTreeMap::new();
    /// map.insert(singular::Identifier::try_from("a")?, Value::ImplicitBinding);
    ///
    /// let name = Identifier::from(singular::Identifier::try_from("foo")?);
    /// let body = Body::from(map);
    ///
    /// let call = Builder::default()
    ///     .name(name)?
    ///     .body(body.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(call.body(), Some(&body));
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn body(&self) -> Option<&Body> {
        self.body.as_ref()
    }

    /// Gets the name for this [`Call`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::workflow::execution::statement::call::Builder;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let name = Identifier::from(singular::Identifier::try_from("foo")?);
    /// let call = Builder::default().name(name.clone())?.try_build()?;
    ///
    /// assert_eq!(call.name(), &name);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Gets the as clause into this [`Call`] by reference (if it exists).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular;
    /// use ast::v1::document::workflow::execution::statement::call::Builder;
    /// use ast::v1::document::Identifier;
    /// use wdl_ast as ast;
    ///
    /// let name = Identifier::from(singular::Identifier::try_from("foo")?);
    /// let r#as = singular::Identifier::try_from("bar")?;
    /// let call = Builder::default()
    ///     .name(name)?
    ///     .r#as(r#as.clone())?
    ///     .try_build()?;
    ///
    /// assert_eq!(call.r#as().unwrap(), &r#as);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn r#as(&self) -> Option<&singular::Identifier> {
        self.r#as.as_ref()
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Call {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self> {
        check_node!(node, workflow_call);
        let mut builder = Builder::default();

        for node in node.into_inner() {
            match node.as_rule() {
                Rule::workflow_call_name => {
                    let inner = unwrap_one!(node, workflow_call_name);
                    let name = Identifier::try_from(inner).map_err(Error::Identifier)?;
                    builder = builder.name(name).map_err(Error::Builder)?;
                }
                Rule::workflow_call_body => {
                    let body = Body::try_from(node).map_err(Error::Body)?;
                    builder = builder.body(body).map_err(Error::Builder)?;
                }
                Rule::workflow_call_as => {
                    let identifier_node = dive_one!(node, singular_identifier, workflow_call_as);
                    let identifier = singular::Identifier::try_from(identifier_node)
                        .map_err(|err| Error::Identifier(identifier::Error::Singular(err)))?;
                    builder = builder.r#as(identifier).map_err(Error::Builder)?;
                }
                Rule::workflow_call_after => {
                    let identifier_node = dive_one!(node, singular_identifier, workflow_call_after);
                    let identifier = singular::Identifier::try_from(identifier_node)
                        .map_err(|err| Error::Identifier(identifier::Error::Singular(err)))?;
                    builder = builder.push_after(identifier);
                }
                Rule::WHITESPACE => {}
                Rule::COMMENT => {}
                rule => {
                    unreachable!("workflow call should not contain {:?}", rule)
                }
            }
        }

        builder.try_build().map_err(Error::Builder)
    }
}

#[cfg(test)]
mod tests {
    use wdl_macros::test::create_invalid_node_test;
    use wdl_macros::test::valid_node;

    use super::*;
    use crate::v1::document::expression::Literal;
    use crate::v1::document::workflow::execution::statement::call::body::Value;
    use crate::v1::document::Expression;

    #[test]
    fn it_parses_from_a_supported_node_type() {
        let call = valid_node!(r#"call foo {input: a, b=true, c=baz}"#, workflow_call, Call);

        assert_eq!(call.name().as_singular().unwrap().as_str(), "foo");

        let body = call.body.unwrap();
        assert_eq!(body.get("a"), Some(&Value::ImplicitBinding));
        assert_eq!(
            body.get("b"),
            Some(&Value::Expression(Expression::Literal(Literal::Boolean(
                true
            ))))
        );
        assert_eq!(
            body.get("c"),
            Some(&Value::Expression(Expression::Literal(
                Literal::Identifier(singular::Identifier::try_from("baz").unwrap())
            )))
        );

        let call = valid_node!(
            r#"call foo.bar {input: a, b=true, c=baz}"#,
            workflow_call,
            Call
        );

        assert_eq!(call.name().as_qualified().unwrap().to_string(), "foo.bar");
    }

    create_invalid_node_test!(
        "version 1.1\n\ntask hello { command <<<>>> }",
        document,
        workflow_call,
        Call,
        it_fails_to_parse_from_an_unsupported_node_type
    );
}
