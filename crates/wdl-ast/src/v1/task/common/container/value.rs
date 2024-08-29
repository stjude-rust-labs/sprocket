//! Values for the `container` item within the `runtime` and `requirements`
//! blocks.

use std::ops::Deref;

use rowan::ast::AstNode;

use crate::v1::Expr;
use crate::v1::LiteralExpr;

pub mod uri;

pub use uri::Uri;

/// An error when parsing a [`Uri`] from an expression.
#[derive(Debug)]
pub enum ParseUriError {
    /// Attempted to create a [`Uri`] from an invalid expression within an
    /// array.
    InvalidExpressionInArray {
        /// The expression from which the [`Uri`] was attempted to be created.
        expr: Expr,
    },

    /// An error occurred when parsing the text value of the expression.
    Uri(uri::Error),
}

impl std::fmt::Display for ParseUriError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseUriError::Uri(err) => write!(f, "uri error: {err}"),
            ParseUriError::InvalidExpressionInArray { expr } => write!(
                f,
                "uri cannot be created from the expression: {expr}",
                expr = expr.syntax()
            ),
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
    InvalidExpression {
        /// The expression from which the [`Uri`] was attempted to be created.
        expr: Expr,
    },
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
            Error::InvalidExpression { expr } => write!(
                f,
                "uri cannot be created from the expression: {expr}",
                expr = expr.syntax()
            ),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](Result) with an [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// The kind of the `container` item value.
#[derive(Debug, Eq, PartialEq)]
pub enum Kind {
    /// A single container URI as a string literal.
    String(Uri),

    /// An array of container URIs as an array.
    Array(Vec<Uri>),
}

impl Kind {
    /// Gets the [`Uri`] present in this [`Value`] through an iterator.
    pub fn uris(&self) -> Box<dyn Iterator<Item = &Uri> + '_> {
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
    pub fn as_single_uri(&self) -> Option<&Uri> {
        match self {
            Kind::String(uri) => Some(uri),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`Uri`](Uri).
    ///
    /// - If the value is a [`Kind::String`], the inner [`Uri`] is returned.
    /// - Else, `None` is returned.
    pub fn into_single_uri(self) -> Option<Uri> {
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
    pub fn unwrap_single_uri(self) -> Uri {
        self.into_single_uri()
            .expect("value is not a single, string literal URI")
    }

    /// Attempts to reference the inner [`Vec`] of [`Uri`]s.
    ///
    /// - If the value is a [`Kind::Array`], a reference to the inner [`Vec`] of
    ///   [`Uri`]s is returned.
    /// - Else, `None` is returned.
    pub fn as_multiple_uris(&self) -> Option<&Vec<Uri>> {
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
    pub fn into_multiple_uris(self) -> Option<Vec<Uri>> {
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
    pub fn unwrap_multiple_uris(self) -> Vec<Uri> {
        self.into_multiple_uris()
            .expect("value is not an array of uris")
    }
}

/// The value of the `container` item.
#[derive(Debug, Eq, PartialEq)]
pub struct Value {
    /// The kind of the value.
    kind: Kind,

    /// The expression backing the value.
    expr: Expr,
}

impl Value {
    /// Gets the kind of the [`Value`].
    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    /// Consumes `self` and returns the kind of the [`Value`].
    pub fn into_kind(self) -> Kind {
        self.kind
    }

    /// Gets the backing expression of the [`Value`].
    pub fn expr(&self) -> &Expr {
        &self.expr
    }

    /// Consumes `self` and returns the node of the [`Value`].
    pub fn into_expr(self) -> Expr {
        self.expr
    }

    /// Consumes `self` and returns the parts of the [`Value`].
    pub fn into_parts(self) -> (Kind, Expr) {
        (self.kind, self.expr)
    }
}

impl Deref for Value {
    type Target = Kind;

    fn deref(&self) -> &Self::Target {
        &self.kind
    }
}

impl TryFrom<Expr> for Value {
    type Error = Error;

    fn try_from(expr: Expr) -> Result<Self> {
        if let Expr::Literal(literal) = &expr {
            match literal {
                // A single URI as a literal string.
                LiteralExpr::String(s) => {
                    return Uri::try_from(s.clone())
                        .map(|uri| Value {
                            kind: Kind::String(uri),
                            expr,
                        })
                        .map_err(|e| Error::ParseString(ParseUriError::Uri(e)));
                }
                // An array of literal strings.
                LiteralExpr::Array(a) => {
                    let mut all_errors: Vec<ParseUriError> = Vec::new();

                    let (errors, literal_strings): (Vec<_>, Vec<_>) = a
                        .elements()
                        .map(|expr| {
                            expr.clone()
                                .into_literal()
                                .and_then(|literal| literal.into_string())
                                .ok_or(ParseUriError::InvalidExpressionInArray { expr })
                        })
                        .partition(std::result::Result::is_err);

                    // SAFETY: we ensured that only results that are [`Err()`] are
                    // partitioned into this vec, so each of these will always
                    // unwrap.
                    all_errors.extend(errors.into_iter().map(std::result::Result::unwrap_err));

                    let (errors, uris): (Vec<_>, Vec<_>) = literal_strings
                        .into_iter()
                        // SAFETY: we ensured that only results that are [`Ok()`] are
                        // partitioned into this vec, so each of these will always
                        // unwrap.
                        .map(std::result::Result::unwrap)
                        .map(Uri::try_from)
                        .partition(std::result::Result::is_err);

                    // SAFETY: we ensured that only results that are [`Err()`] are
                    // partitioned into this vec, so each of these will always
                    // unwrap.
                    all_errors.extend(
                        errors
                            .into_iter()
                            .map(std::result::Result::unwrap_err)
                            .map(ParseUriError::Uri),
                    );

                    if !all_errors.is_empty() {
                        return Err(Error::ParseArray(all_errors));
                    }

                    let uris = uris
                        .into_iter()
                        // SAFETY: we ensured that only results that are [`Ok()`] are
                        // partitioned into this vec, so each of these will always
                        // unwrap.
                        .map(std::result::Result::unwrap)
                        .collect();

                    return Ok(Value {
                        kind: Kind::Array(uris),
                        expr,
                    });
                }
                _ => {}
            }
        }

        Err(Error::InvalidExpression { expr })
    }
}

#[cfg(test)]
mod tests {
    use crate::v1::task::requirements::item::container::Container;
    use crate::Document;

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
