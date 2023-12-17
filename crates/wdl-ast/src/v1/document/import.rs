//! Imports.

use std::collections::BTreeMap;

use grammar::v1::Rule;
use pest::iterators::Pair;
use wdl_grammar as grammar;

mod builder;

pub use builder::Builder;
use wdl_macros::check_node;
use wdl_macros::dive_one;

use crate::v1::document::identifier::singular;
use crate::v1::document::identifier::singular::Identifier;

/// An error related to an [`Import`].
#[derive(Debug)]
pub enum Error {
    /// A builder error.
    Builder(builder::Error),

    /// An identifier error.
    Identifier(singular::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Builder(err) => write!(f, "builder error: {err}"),
            Error::Identifier(err) => write!(f, "identifier error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// Aliases within an import.
pub type Aliases = BTreeMap<Identifier, Identifier>;

/// An import.
#[derive(Clone, Debug)]
pub struct Import {
    /// Aliases (if they exist).
    aliases: Option<Aliases>,

    /// As (if it exists).
    r#as: Option<Identifier>,

    /// The URI.
    uri: String,
}

impl Import {
    /// Gets the aliases from the [`Import`] by reference (if they exist).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::import::Builder;
    /// use wdl_ast as ast;
    ///
    /// let import = Builder::default()
    ///     .insert_alias(
    ///         Identifier::try_from("hello_world")?,
    ///         Identifier::try_from("foo_bar").unwrap(),
    ///     )
    ///     .r#as(Identifier::try_from("baz_quux").unwrap())?
    ///     .uri(String::from("../mapping.wdl"))?
    ///     .try_build()?;
    ///
    /// assert_eq!(
    ///     import.aliases().unwrap().get("hello_world"),
    ///     Some(&Identifier::try_from("foo_bar").unwrap())
    /// );
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn aliases(&self) -> Option<&Aliases> {
        self.aliases.as_ref()
    }

    /// Gets the as from the [`Import`] by reference (if it exists).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::import::Builder;
    /// use wdl_ast as ast;
    ///
    /// let import = Builder::default()
    ///     .insert_alias(
    ///         Identifier::try_from("hello_world")?,
    ///         Identifier::try_from("foo_bar").unwrap(),
    ///     )
    ///     .r#as(Identifier::try_from("baz_quux").unwrap())?
    ///     .uri(String::from("../mapping.wdl"))?
    ///     .try_build()?;
    ///
    /// assert_eq!(
    ///     import.r#as().unwrap(),
    ///     &Identifier::try_from("baz_quux").unwrap()
    /// );
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn r#as(&self) -> Option<&Identifier> {
        self.r#as.as_ref()
    }

    /// Gets the URI from the [`Import`] by reference (if it exists).
    ///
    /// # Examples
    ///
    /// ```
    /// use ast::v1::document::identifier::singular::Identifier;
    /// use ast::v1::document::import::Builder;
    /// use wdl_ast as ast;
    ///
    /// let import = Builder::default()
    ///     .insert_alias(
    ///         Identifier::try_from("hello_world")?,
    ///         Identifier::try_from("foo_bar").unwrap(),
    ///     )
    ///     .r#as(Identifier::try_from("baz_quux").unwrap())?
    ///     .uri(String::from("../mapping.wdl"))?
    ///     .try_build()?;
    ///
    /// assert_eq!(import.uri(), "../mapping.wdl");
    ///
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn uri(&self) -> &str {
        self.uri.as_str()
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Import {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self, Self::Error> {
        check_node!(node, import);
        let mut builder = builder::Builder::default();

        for node in node.into_inner() {
            match node.as_rule() {
                Rule::import_uri => {
                    let uri = dive_one!(node, string_literal_contents, import_uri);
                    builder = builder
                        .uri(uri.as_str().to_owned())
                        .map_err(Error::Builder)?;
                }
                Rule::import_as => {
                    let identifier_node = dive_one!(node, singular_identifier, import_as);
                    let identifier = Identifier::try_from(identifier_node.as_str().to_string())
                        .map_err(Error::Identifier)?;
                    builder = builder.r#as(identifier).map_err(Error::Builder)?;
                }
                Rule::import_alias => {
                    // TODO: a clone is required here because Pest's `FlatPairs`
                    // type does not support creating an iterator without taking
                    // ownership (at the time of writing). This can be made
                    // better with a PR to Pest.
                    let from_node = dive_one!(node.clone(), import_alias_from, import_alias);
                    let from = Identifier::try_from(from_node.as_str().to_string())
                        .map_err(Error::Identifier)?;

                    let to_node = dive_one!(node, import_alias_to, import_alias);
                    let to = Identifier::try_from(to_node.as_str().to_string())
                        .map_err(Error::Identifier)?;

                    builder = builder.insert_alias(from, to);
                }
                Rule::COMMENT => {}
                Rule::WHITESPACE => {}
                rule => unreachable!("import should not contain {:?}", rule),
            }
        }

        builder.try_build().map_err(Error::Builder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_parses_a_complicated_import_correctly()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let import = wdl_grammar::v1::parse_rule(
            wdl_grammar::v1::Rule::import,
            r#"import "hello.wdl" as hello alias foo as bar alias baz as quux"#,
        )
        .unwrap()
        .into_tree()
        .unwrap();

        let import = Import::try_from(import).unwrap();
        assert_eq!(import.uri(), "hello.wdl");
        assert_eq!(import.r#as().map(|x| x.as_str()), Some("hello"));

        let aliases = import.aliases().unwrap();
        assert_eq!(
            aliases.get("foo"),
            Some(&Identifier::try_from("bar").unwrap())
        );
        assert_eq!(
            aliases.get("baz"),
            Some(&Identifier::try_from("quux").unwrap())
        );

        Ok(())
    }
}
