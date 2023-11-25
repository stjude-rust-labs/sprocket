//! Outputs.

use grammar::v1::Rule;
use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;

use crate::v1::document::declaration::bound;
use crate::v1::document::declaration::bound::Declaration;

mod builder;

pub use builder::Builder;

/// An error related to a [`Output`].
#[derive(Debug)]
pub enum Error {
    /// A declaration error.
    Declaration(bound::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Declaration(err) => write!(f, "declaration error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// Bound declarations in an [`Output`].
pub type Declarations = NonEmpty<Declaration>;

/// An output.
///
/// **Note:** this struct could have been designed as a tuple struct. However,
/// it felt non-ergonomic to wrap an optional type and allow dereferencing as is
/// the convention elsewhere in the code base. As such, it is written as a
/// single field struct.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct Output {
    /// The bound declarations (if they exist).
    declarations: Option<Declarations>,
}

impl Output {
    /// Gets the [bound declaration(s)](Declarations) from the [`Output`] by
    /// reference (if they exist).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration::bound;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::expression::Literal;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::output::Builder;
    /// use ast::v1::document::Expression;
    /// use wdl_ast as ast;
    ///
    /// let declaration = bound::Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .value(Expression::Literal(Literal::Identifier(
    ///         Identifier::try_from("foo").unwrap(),
    ///     )))?
    ///     .try_build()?;
    ///
    /// let output = Builder::default()
    ///     .push_bound_declaration(declaration)
    ///     .build();
    ///
    /// let declarations = output.declarations().unwrap();
    /// assert_eq!(declarations.len(), 1);
    ///
    /// let declaration = declarations.into_iter().next().unwrap();
    /// assert_eq!(declaration.name().as_str(), "hello_world");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn declarations(&self) -> Option<&NonEmpty<Declaration>> {
        self.declarations.as_ref()
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Output {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self, Error> {
        check_node!(node, output);
        let mut builder = builder::Builder::default();

        for node in node.into_inner() {
            match node.as_rule() {
                Rule::bound_declaration => {
                    let declaration = Declaration::try_from(node).map_err(Error::Declaration)?;
                    builder = builder.push_bound_declaration(declaration);
                }
                Rule::COMMENT => {}
                Rule::WHITESPACE => {}
                rule => unreachable!("workflow output should not contain {:?}", rule),
            }
        }

        Ok(builder.build())
    }
}
