//! Values for the `container` item within the `runtime` and `requirements`
//! blocks.

use std::ops::Deref;

use crate::AstNode;
use crate::TreeNode;
use crate::v1::Expr;
use crate::v1::LiteralExpr;

pub mod uri;

pub use uri::Uri;
use wdl_grammar::SyntaxNode;

/// An error when parsing a [`Uri`] from an expression.
#[derive(Debug)]
pub enum ParseUriError {
    /// Attempted to create a [`Uri`] from an invalid expression within an
    /// array.
    InvalidExpressionInArray(String),

    /// An error occurred when parsing the text value of the expression.
    Uri(uri::Error),
}

impl std::fmt::Display for ParseUriError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseUriError::Uri(err) => write!(f, "uri error: {err}"),
            ParseUriError::InvalidExpressionInArray(expr) => {
                write!(f, "uri cannot be created from the expression: {expr}")
            }
        }
    }
}

/// An error related to a [`Uri`].
#[derive(Debug)]
pub enum Error {
    /// An error that occurs when parsing a [`Uri`] within a [`Kind::String`].
    ParseString(ParseUriError),

    /// An error that occurs when parsing an array of [`Uri`]s within a
    /// [`Kind::Array`].
    ParseArray(Vec<ParseUriError>),

    /// Attempted to create a [`Uri`] from an invalid expression.
    InvalidExpression(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ParseString(error) => write!(f, "parse uri error: {error}"),
            Error::ParseArray(errors) => {
                write!(
                    f,
                    "multiple parse uri errors: {errors}",
                    errors = errors
                        .iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>()
                        .join("; ")
                )
            }
            Error::InvalidExpression(expr) => {
                write!(f, "uri cannot be created from the expression: {expr}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](Result) with an [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// The kind of the `container` item value.
#[derive(Debug, Eq, PartialEq)]
pub enum Kind<N: TreeNode = SyntaxNode> {
    /// A single container URI as a string literal.
    String(Uri<N>),

    /// An array of container URIs as an array.
    Array(Vec<Uri<N>>),
}

impl<N: TreeNode> Kind<N> {
    /// Gets the [`Uri`] present in this [`Value`] through an iterator.
    pub fn uris(&self) -> Box<dyn Iterator<Item = &Uri<N>> + '_> {
        match self {
            Kind::String(uri) => Box::new(std::iter::once(uri)),
            Kind::Array(uris) => Box::new(uris.iter()),
        }
    }

    /// Attempts to reference the inner [`Uri`](Uri).
    ///
    /// - If the value is a [`Kind::String`], a reference to the inner [`Uri`]
    ///   is returned.
    /// - Else, `None` is returned.
    pub fn as_single_uri(&self) -> Option<&Uri<N>> {
        match self {
            Kind::String(uri) => Some(uri),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`Uri`](Uri).
    ///
    /// - If the value is a [`Kind::String`], the inner [`Uri`] is returned.
    /// - Else, `None` is returned.
    pub fn into_single_uri(self) -> Option<Uri<N>> {
        match self {
            Kind::String(uri) => Some(uri),
            _ => None,
        }
    }

    /// Consumes `self` and returns the inner [`Uri`].
    ///
    /// # Panics
    ///
    /// Panics if the kind of the value is not [`Kind::String`].
    pub fn unwrap_single_uri(self) -> Uri<N> {
        self.into_single_uri()
            .expect("value is not a single, string literal URI")
    }

    /// Attempts to reference the inner [`Vec`] of [`Uri`]s.
    ///
    /// - If the value is a [`Kind::Array`], a reference to the inner [`Vec`] of
    ///   [`Uri`]s is returned.
    /// - Else, `None` is returned.
    pub fn as_multiple_uris(&self) -> Option<&Vec<Uri<N>>> {
        match self {
            Kind::Array(uri) => Some(uri),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`Vec`] of [`Uri`]s.
    ///
    /// - If the value is a [`Kind::Array`], the inner [`Vec`] of [`Uri`]s is
    ///   returned.
    /// - Else, `None` is returned.
    pub fn into_multiple_uris(self) -> Option<Vec<Uri<N>>> {
        match self {
            Kind::Array(uri) => Some(uri),
            _ => None,
        }
    }

    /// Consumes `self` and returns the inner [`Vec`] of [`Uri`]s.
    ///
    /// # Panics
    ///
    /// Panics if the value is not an array of uris.
    pub fn unwrap_multiple_uris(self) -> Vec<Uri<N>> {
        self.into_multiple_uris()
            .expect("value is not an array of uris")
    }
}

/// The value of the `container` item.
#[derive(Debug, Eq, PartialEq)]
pub struct Value<N: TreeNode = SyntaxNode> {
    /// The kind of the value.
    kind: Kind<N>,

    /// The expression backing the value.
    expr: Expr<N>,
}

impl<N: TreeNode> Value<N> {
    /// Gets the kind of the [`Value`].
    pub fn kind(&self) -> &Kind<N> {
        &self.kind
    }

    /// Consumes `self` and returns the kind of the [`Value`].
    pub fn into_kind(self) -> Kind<N> {
        self.kind
    }

    /// Gets the backing expression of the [`Value`].
    pub fn expr(&self) -> &Expr<N> {
        &self.expr
    }

    /// Consumes `self` and returns the node of the [`Value`].
    pub fn into_expr(self) -> Expr<N> {
        self.expr
    }

    /// Consumes `self` and returns the parts of the [`Value`].
    pub fn into_parts(self) -> (Kind<N>, Expr<N>) {
        (self.kind, self.expr)
    }
}

impl<N: TreeNode> Deref for Value<N> {
    type Target = Kind<N>;

    fn deref(&self) -> &Self::Target {
        &self.kind
    }
}

impl<N: TreeNode> TryFrom<Expr<N>> for Value<N> {
    type Error = Error;

    fn try_from(expr: Expr<N>) -> Result<Self> {
        if let Expr::Literal(literal) = &expr {
            match literal {
                // A single URI as a literal string.
                LiteralExpr::String(s) => {
                    return Uri::<N>::try_from(s.clone())
                        .map(|uri| Value {
                            kind: Kind::String(uri),
                            expr,
                        })
                        .map_err(|e| Error::ParseString(ParseUriError::Uri(e)));
                }
                // An array of literal strings.
                LiteralExpr::Array(a) => {
                    let mut errors: Vec<ParseUriError> = Vec::new();

                    let uris: Vec<_> = a
                        .elements()
                        .filter_map(|expr| {
                            match expr
                                .clone()
                                .into_literal()
                                .and_then(|literal| literal.into_string())
                            {
                                Some(s) => match Uri::try_from(s) {
                                    Ok(uri) => Some(uri),
                                    Err(e) => {
                                        errors.push(ParseUriError::Uri(e));
                                        None
                                    }
                                },
                                None => {
                                    errors.push(ParseUriError::InvalidExpressionInArray(
                                        expr.text().to_string(),
                                    ));
                                    None
                                }
                            }
                        })
                        .collect();

                    if !errors.is_empty() {
                        return Err(Error::ParseArray(errors));
                    }

                    return Ok(Value {
                        kind: Kind::Array(uris),
                        expr,
                    });
                }
                _ => {}
            }
        }

        Err(Error::InvalidExpression(expr.text().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use crate::Document;
    use crate::v1::task::requirements::item::container::Container;

    fn get_container(document: Document) -> Container {
        document
            .ast()
            .as_v1()
            .expect("v1 ast")
            .tasks()
            .next()
            .expect("the 'hello' task to exist")
            .requirements()
            .expect("the 'requirements' block to exist")
            .items()
            .find_map(|p| p.into_container())
            .expect("at least one 'container' item to exist")
    }

    #[test]
    fn it_parses_a_valid_single_uri_value() {
        let (document, diagnostics) = Document::parse(
            r#"version 1.2

task hello {
    requirements {
        container: 'ubuntu:latest'
    }
}"#,
        );

        assert!(diagnostics.is_empty());

        let container = get_container(document);

        let entry = container
            .value()
            .expect("value to be parsed")
            .into_kind()
            .unwrap_single_uri()
            .into_kind()
            .unwrap_entry();

        assert!(entry.protocol().is_none());
        assert_eq!(entry.image(), "ubuntu");
        assert_eq!(entry.tag().unwrap(), "latest");
        assert!(!entry.immutable());
    }

    #[test]
    fn it_parses_a_valid_any_value() {
        let (document, diagnostics) = Document::parse(
            r#"version 1.2

task hello {
    requirements {
        container: '*'
    }
}"#,
        );

        assert!(diagnostics.is_empty());

        let container = get_container(document);
        let kind = container
            .value()
            .expect("value to be parsed")
            .into_kind()
            .unwrap_single_uri()
            .into_kind();

        assert!(kind.is_any());
    }

    #[test]
    fn it_fails_to_parse_an_any_within_an_array() {
        let (document, diagnostics) = Document::parse(
            r#"version 1.2

task hello {
    requirements {
        container: ['ubuntu:latest', '*']
    }
}"#,
        );

        assert!(diagnostics.is_empty());

        let container = get_container(document);
        let mut uris = container
            .value()
            .expect("value to be parsed")
            .into_kind()
            .unwrap_multiple_uris()
            .into_iter();

        let entry = uris.next().unwrap().into_kind().unwrap_entry();
        assert!(entry.protocol().is_none());
        assert_eq!(entry.image(), "ubuntu");
        assert_eq!(entry.tag().unwrap(), "latest");
        assert!(!entry.immutable());

        let kind = uris.next().unwrap().into_kind();
        assert!(kind.is_any());

        assert!(uris.next().is_none());
    }
}
