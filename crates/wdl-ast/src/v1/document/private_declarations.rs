//! Private declarations.

use grammar::v1::Rule;
use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;

use crate::v1::document::declaration::bound;
use crate::v1::document::declaration::bound::Declaration;

mod builder;

pub use builder::Builder;

/// An error related to [`PrivateDeclarations`].
#[derive(Debug)]
pub enum Error {
    /// A builder error.
    Builder(builder::Error),

    /// A declaration error.
    Declaration(bound::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Builder(err) => write!(f, "builder error: {err}"),
            Error::Declaration(err) => write!(f, "declaration error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// The inner list of [bound declarations](`Declaration`) for
/// [`PrivateDeclarations`].
type Declarations = NonEmpty<Declaration>;

/// A set of private declarations.
///
/// Private declarations are comprised of one or more [bound
/// declarations](Declaration).
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PrivateDeclarations(Declarations);

impl PrivateDeclarations {
    /// Gets the inner value from the [`PrivateDeclarations`] by reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::bound::Builder;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::Declaration;
    /// use ast::v1::document::Expression;
    /// use ast::v1::document::PrivateDeclarations;
    /// use nonempty::NonEmpty;
    /// use wdl_ast as ast;
    ///
    /// let declaration = Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::None))?
    ///     .try_build()?;
    ///
    /// let private_declarations = PrivateDeclarations::from(NonEmpty::new(declaration.clone()));
    ///
    /// assert_eq!(
    ///     private_declarations.inner().into_iter().next().unwrap(),
    ///     &declaration
    /// );
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn inner(&self) -> &Declarations {
        &self.0
    }

    /// Consumes `self` to return the inner value from the
    /// [`PrivateDeclarations`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::bound::Builder;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::Declaration;
    /// use ast::v1::document::Expression;
    /// use ast::v1::document::PrivateDeclarations;
    /// use nonempty::NonEmpty;
    /// use wdl_ast as ast;
    ///
    /// let declaration = Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::None))?
    ///     .try_build()?;
    ///
    /// let private_declarations = PrivateDeclarations::from(NonEmpty::new(declaration.clone()));
    ///
    /// assert_eq!(
    ///     private_declarations
    ///         .into_inner()
    ///         .into_iter()
    ///         .next()
    ///         .unwrap(),
    ///     declaration
    /// );
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn into_inner(self) -> Declarations {
        self.0
    }
}

impl From<Declarations> for PrivateDeclarations {
    fn from(declarations: Declarations) -> Self {
        PrivateDeclarations(declarations)
    }
}

impl TryFrom<Pair<'_, Rule>> for PrivateDeclarations {
    type Error = Error;

    fn try_from(node: Pair<'_, Rule>) -> Result<Self, Self::Error> {
        check_node!(node, private_declarations);
        let mut builder = Builder::default();

        for node in node.into_inner() {
            match node.as_rule() {
                Rule::bound_declaration => {
                    let declaration = Declaration::try_from(node).map_err(Error::Declaration)?;
                    builder = builder.push_bound_declaration(declaration);
                }
                Rule::WHITESPACE => {}
                Rule::COMMENT => {}
                rule => unreachable!("private declarations should not contain {:?}", rule),
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
    use crate::v1::document::declaration::r#type::Kind;

    #[test]
    fn it_parses_from_a_supported_node_type() {
        let declarations = valid_node!(
            r#"Boolean hello = false"#,
            private_declarations,
            PrivateDeclarations
        );

        assert_eq!(declarations.inner().len(), 1);

        let declaration = declarations.inner().iter().next().unwrap();
        assert_eq!(declaration.name().as_str(), "hello");
        assert_eq!(declaration.r#type().kind(), &Kind::Boolean);
        assert!(!declaration.r#type().optional());
    }

    create_invalid_node_test!(
        "version 1.1\n\ntask hello { command <<<>>> }",
        document,
        private_declarations,
        PrivateDeclarations,
        it_fails_to_parse_from_an_unsupported_node_type
    );
}
