//! WDL 1.x abstract syntax tree.
//!
//! ## Validation Rules
//!
//! The following abstract syntax tree validation rules are supported for WDL
//! 1.x:
//!
//! _None at present._
//!
//! ## Lint Rules
//!
//! The following abstract syntax tree linting rules are supported for WDL 1.x:
//!
//! | Name                      | Code       | Tags         | Documentation                       |
//! |:--------------------------|:-----------|:-------------|:-----------------------------------:|
//! | `matching_parameter_meta` | `v1::W003` | Completeness | [Link](lint::MatchingParameterMeta) |

use pest::iterators::Pair;
use wdl_core::concern::concerns;
use wdl_core::concern::lint::Linter;
use wdl_core::concern::validation::Validator;
use wdl_core::Concern;
use wdl_grammar as grammar;

pub mod document;
pub mod lint;
pub mod validation;

pub use document::Document;

/// An unrecoverable error when parsing a WDL 1.x abstract syntax tree.
#[derive(Debug)]
pub enum Error {
    /// An unrecoverable error that occurred during document parsing.
    Document(document::Error),

    /// An unrecoverable error that occurred during linting.
    Lint(wdl_core::concern::lint::Error),

    /// An unrecoverable error that occurred during validation.
    Validation(wdl_core::concern::validation::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Document(err) => write!(f, "document error: {err}"),
            Error::Lint(err) => write!(f, "lint error: {err}"),
            Error::Validation(err) => write!(f, "validation error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// An abstract syntax tree [parse result](wdl_core::parse::Result).
pub type Result = wdl_core::parse::Result<Document>;

/// Parses an abstract syntax tree (in the form of a [`Document`]) from a
/// [`Pair<'_, grammar::v1::Rule>`].
///
/// # Examples
///
/// ```
/// use grammar::v1::Rule;
/// use wdl_ast as ast;
/// use wdl_grammar as grammar;
///
/// let pt = grammar::v1::parse("version 1.1")
///     .unwrap()
///     .into_tree()
///     .unwrap();
/// let ast = ast::v1::parse(pt).unwrap().into_tree().unwrap();
///
/// assert_eq!(ast.version(), &ast::v1::document::Version::OneDotOne);
/// ```
pub fn parse(tree: Pair<'_, grammar::v1::Rule>) -> std::result::Result<Result, Error> {
    let mut concerns = concerns::Builder::default();

    let document = Document::try_from(tree).map_err(Error::Document)?;

    if let Some(failures) =
        Validator::validate(&document, validation::rules()).map_err(Error::Validation)?
    {
        for failure in failures {
            concerns = concerns.push(Concern::ValidationFailure(failure));
        }
    };

    if let Some(warnings) = Linter::lint(&document, lint::rules()).map_err(Error::Lint)? {
        for warning in warnings {
            concerns = concerns.push(Concern::LintWarning(warning));
        }
    };

    // SAFETY: the abstract syntax tree is always [`Some`] at this point, even
    // if the concerns are empty, so this will always unwrap.
    Ok(Result::try_new(Some(document), concerns.build()).unwrap())
}
