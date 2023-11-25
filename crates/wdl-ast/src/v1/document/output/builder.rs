//! Builder for an [`Output`].

use nonempty::NonEmpty;

use crate::v1::document::declaration::bound::Declaration;
use crate::v1::document::output::Declarations;
use crate::v1::document::Output;

/// A builder for an [`Output`].
#[derive(Debug, Default)]
pub struct Builder {
    /// The bound declarations (if they exist).
    declarations: Option<Declarations>,
}

impl Builder {
    /// Pushes a [bound declaration](Declaration) into the [`Builder`].
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
    pub fn push_bound_declaration(mut self, declaration: Declaration) -> Self {
        let declarations = match self.declarations {
            Some(mut declarations) => {
                declarations.push(declaration);
                declarations
            }
            None => NonEmpty::new(declaration),
        };

        self.declarations = Some(declarations);
        self
    }

    /// Consumes `self` to build an [`Output`].
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
    pub fn build(self) -> Output {
        Output {
            declarations: self.declarations,
        }
    }
}

#[cfg(test)]
mod tests {
    use wdl_macros::test::create_invalid_node_test;
    use wdl_macros::test::valid_node;

    use super::*;
    use crate::v1::document::declaration::r#type::Kind;
    use crate::v1::document::expression::Literal;
    use crate::v1::document::Expression;

    #[test]
    fn it_parses_from_a_supported_node_type() {
        let output = valid_node!(r#"output { String baz = None }"#, output, Output);
        assert_eq!(output.declarations().unwrap().len(), 1);

        let declaration = output.declarations().unwrap().into_iter().next().unwrap();
        assert_eq!(declaration.name().as_str(), "baz");
        assert_eq!(declaration.r#type().kind(), &Kind::String);
        assert!(!declaration.r#type().optional());
        assert_eq!(declaration.value(), &Expression::Literal(Literal::None));
    }

    create_invalid_node_test!(
        "version 1.1\n\ntask hello { command <<<>>> }",
        document,
        output,
        Output,
        it_fails_to_parse_from_an_unsupported_node_type
    );
}
