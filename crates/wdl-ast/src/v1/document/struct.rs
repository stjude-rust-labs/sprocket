//! Structs.

use grammar::v1::Rule;
use nonempty::NonEmpty;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;

use crate::v1::document::declaration::unbound;
use crate::v1::document::declaration::unbound::Declaration;
use crate::v1::document::identifier::singular;
use crate::v1::document::identifier::singular::Identifier;

mod builder;

pub use builder::Builder;

/// An error related to a [`Struct`].
#[derive(Debug)]
pub enum Error {
    /// A builder error.
    Builder(builder::Error),

    /// An identifier error.
    Identifier(singular::Error),

    /// An unbound declaration error.
    UnboundDeclaration(unbound::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Builder(err) => write!(f, "builder error: {err}"),
            Error::Identifier(err) => write!(f, "identifier error: {err}"),
            Error::UnboundDeclaration(err) => write!(f, "unbound declaration error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// Unbound declarations within a struct.
pub type Declarations = NonEmpty<Declaration>;

/// A struct.
#[derive(Clone, Debug)]
pub struct Struct {
    /// The unbound declarations (if they exist).
    declarations: Option<Declarations>,

    /// The name.
    name: Identifier,
}

impl Struct {
    /// Gets the [`Declarations`] for this [`Struct`] by reference (if they
    /// exist).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::declaration;
    /// use ast::v1::document::declaration::r#type::Kind;
    /// use ast::v1::document::declaration::Type;
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::r#struct::Builder;
    /// use wdl_ast as ast;
    ///
    /// let declaration = declaration::unbound::Builder::default()
    ///     .name(Identifier::try_from("hello_world")?)?
    ///     .r#type(Type::new(Kind::Boolean, false))?
    ///     .try_build()?;
    ///
    /// let r#struct = Builder::default()
    ///     .name(Identifier::try_from("a_struct").unwrap())?
    ///     .push_unbound_declaration(declaration.clone())
    ///     .try_build()?;
    ///
    /// assert_eq!(r#struct.declarations().unwrap().len(), 1);
    /// assert_eq!(r#struct.declarations().unwrap().first(), &declaration);
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn declarations(&self) -> Option<&Declarations> {
        self.declarations.as_ref()
    }

    /// Gets the name from the [`Struct`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::r#struct::Builder;
    /// use wdl_ast as ast;
    ///
    /// let r#struct = Builder::default()
    ///     .name(Identifier::try_from("a_struct").unwrap())?
    ///     .try_build()?;
    ///
    /// assert_eq!(r#struct.name().as_str(), "a_struct");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn name(&self) -> &Identifier {
        &self.name
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Struct {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self, Self::Error> {
        check_node!(node, r#struct);
        let mut builder = Builder::default();

        for node in node.into_inner() {
            match node.as_rule() {
                Rule::struct_name => {
                    let identifier = Identifier::try_from(node.as_str().to_owned())
                        .map_err(Error::Identifier)?;
                    builder = builder.name(identifier).map_err(Error::Builder)?;
                }
                Rule::unbound_declaration => {
                    let declaration =
                        Declaration::try_from(node).map_err(Error::UnboundDeclaration)?;
                    builder = builder.push_unbound_declaration(declaration);
                }
                Rule::WHITESPACE => {}
                Rule::COMMENT => {}
                rule => unreachable!("struct should not contain {:?}", rule),
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
        let r#struct = valid_node!(r#"struct Hello { String? world }"#, r#struct, Struct);
        assert_eq!(r#struct.name().as_str(), "Hello");

        let declaration = r#struct.declarations().unwrap().into_iter().next().unwrap();
        assert_eq!(declaration.name().as_str(), "world");
        assert_eq!(declaration.r#type().kind(), &Kind::String);
        assert!(declaration.r#type().optional());
    }

    create_invalid_node_test!(
        "version 1.1\n\ntask hello { command <<<>>> }",
        document,
        r#struct,
        Struct,
        it_fails_to_parse_from_an_unsupported_node_type
    );
}
