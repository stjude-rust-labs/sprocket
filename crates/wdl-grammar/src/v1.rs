//! WDL 1.x parse tree.
//!
//! ## Validation Rules
//!
//! The following parse tree validation rules are supported for WDL 1.x:
//!
//! | Name                       | Code       | Documentation                              |
//! |:---------------------------|:-----------|:------------------------------------------:|
//! | `invalid_escape_character` | `v1::E001` | [Link](validation::InvalidEscapeCharacter) |
//! | `invalid_version`          | `v1::E002` | [Link](validation::InvalidVersion)         |
//! | `duplicate_runtime_keys`   | `v1::E003` | [Link](validation::DuplicateRuntimeKeys)   |
//! | `missing_literal_commas`   | `v1::E004` | [Link](validation::MissingLiteralCommas)   |
//!
//! ## Lint Rules
//!
//! The following parse tree linting rules are supported for WDL 1.x:
//!
//! | Name                    | Code       | Group       | Documentation                     |
//! |:------------------------|:-----------|:------------|:---------------------------------:|
//! | `whitespace`            | `v1::W001` | Style       | [Link](lint::Whitespace)          |
//! | `no_curly_commands`     | `v1::W002` | Pedantic    | [Link](lint::NoCurlyCommands)     |
//! | `mixed_indentation`     | `v1::W004` | Style       | [Link](lint::MixedIndentation)    |
//! | `missing_runtime_block` | `v1::W005` |Completeness | [Link](lint::MissingRuntimeBlock) |

use pest::iterators::Pair;
use pest::Parser as _;
use wdl_core::concern::concerns;
use wdl_core::concern::lint::Linter;
use wdl_core::concern::validation::Validator;
use wdl_core::Concern;

pub mod lint;
mod parse;
#[cfg(test)]
mod tests;
pub mod validation;

pub(crate) use parse::Parser;
pub use parse::Rule;

/// An unrecoverable error when parsing a WDL 1.x parse tree.
#[derive(Debug)]
pub enum Error {
    /// An unrecoverable error that occurred during linting.
    Lint(wdl_core::concern::lint::Error),

    /// An unrecoverable error that occurred during validation.
    Validation(wdl_core::concern::validation::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Lint(err) => write!(f, "lint error: {err}"),
            Error::Validation(err) => write!(f, "validation error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A parse tree [parse result](wdl_core::parse::Result).
pub type Result<'a> = wdl_core::parse::Result<Pair<'a, crate::v1::Rule>>;

/// Parses a WDL 1.x input according to the specified [Rule].
///
/// **Note:** prefer the [`parse()`] method when parsing a WDL document. This
/// rule should only be used to parse particular rules other than
/// [`Rule::document`].
///
/// # Examples
///
/// ```
/// use grammar::v1::Rule;
/// use grammar::Error;
/// use wdl_grammar as grammar;
///
/// // A valid grammar tree with lint warnings.
///
/// let result = grammar::v1::parse_rule(Rule::document, "version 1.1\n \n")?;
///
/// let concerns = result.concerns().unwrap();
/// assert_eq!(concerns.inner().len(), 1);
///
/// let warning = concerns.inner().first();
/// assert_eq!(
///     warning.to_string(),
///     String::from("[v1::W001::Style/Low] line contains only whitespace (2:1-2:1)")
/// );
///
/// let tree = result.into_tree().unwrap();
/// assert!(matches!(tree.as_rule(), Rule::document));
///
/// // An invalid grammar tree due to pest parsing.
///
/// let result = grammar::v1::parse_rule(Rule::document, "Hello, world!").unwrap();
///
/// let concerns = result.concerns().unwrap();
/// assert_eq!(concerns.inner().len(), 1);
///
/// let error = concerns.inner().first();
/// assert_eq!(
///     error.to_string(),
///     String::from("The following tokens are required: document. (1:1)")
/// );
///
/// let tree = result.into_tree();
/// assert!(tree.is_none());
///
/// // An invalid grammar tree due to our additional validation.
///
/// let result = grammar::v1::parse_rule(
///     Rule::document,
///     r#"version 1.1
/// task test {
///     output {
///         String hello = "\."
///     }
///     runtime {
///         cpu: 1
///         memory: "2GiB"
///     }
/// }"#,
/// )
/// .unwrap();
///
/// let concerns = result.concerns().unwrap();
/// assert_eq!(concerns.inner().len(), 1);
///
/// let error = concerns.inner().first();
/// assert_eq!(
///     error.to_string(),
///     String::from("[v1::E001] invalid escape character: '\\.' (4:25-4:27)")
/// );
///
/// let tree = result.into_tree();
/// assert!(tree.is_none());
///
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn parse_rule(rule: Rule, input: &str) -> std::result::Result<Result<'_>, Error> {
    let mut concerns = concerns::Builder::default();

    let mut pt = match Parser::parse(rule, input) {
        Ok(pt) => pt,
        Err(err) => {
            let concerns = concerns
                .push(Concern::ParseError(wdl_core::concern::parse::Error::from(
                    err,
                )))
                .build();
            // SAFETY: `concerns` will never be `None`, as we just pushed a
            // parse error into `concerns`. As such, this will always unwrap.
            return Ok(Result::try_new(None, concerns).unwrap());
        }
    };

    let pt = match pt.len() {
        // SAFETY: we just ensured there is exactly one element in the parse
        // tree. Thus, this will always unwrap.
        1 => pt.next().unwrap(),
        // SAFETY: there should always be one and only one root element.
        _ => unreachable!(),
    };

    if let Some(failures) =
        Validator::validate(&pt, validation::rules()).map_err(Error::Validation)?
    {
        for failure in failures {
            concerns = concerns.push(Concern::ValidationFailure(failure));
        }
    }

    if let Some(warnings) = Linter::lint(&pt, lint::rules()).map_err(Error::Lint)? {
        for warning in warnings {
            concerns = concerns.push(Concern::LintWarning(warning));
        }
    };

    // SAFETY: the parse tree is always [`Some`] at this point, even if the
    // concerns are empty, so this will always unwrap.
    let concerns = concerns.build();

    let pt = match concerns
        .as_ref()
        .and_then(|concerns| concerns.validation_failures())
    {
        Some(_) => None,
        None => Some(pt),
    };

    // SAFETY: the parse tree is only set to [`None`] when there are validation
    // errors (a parse tree with validation errors should not be returned). In
    // that case, it follows that the concerns cannot be [`None`]. In every
    // other case at this point, the parse tree is [`Some`]. As such, there is
    // no case where this will not unwrap.
    Ok(Result::try_new(pt, concerns).unwrap())
}

/// Parses a WDL 1.x document.
///
/// # Examples
///
/// ```
/// use grammar::v1::Rule;
/// use grammar::Error;
/// use wdl_grammar as grammar;
///
/// let pt = grammar::v1::parse("version 1.1\n \n")?;
///
/// let concerns = pt.concerns().unwrap();
/// assert_eq!(concerns.inner().len(), 1);
///
/// let warning = concerns.inner().first();
/// assert_eq!(
///     warning.to_string(),
///     String::from("[v1::W001::Style/Low] line contains only whitespace (2:1-2:1)")
/// );
///
/// let pair = pt.into_tree().unwrap();
/// assert!(matches!(pair.as_rule(), Rule::document));
///
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn parse(input: &str) -> std::result::Result<Result<'_>, Error> {
    parse_rule(Rule::document, input)
}

/// Gets a rule by name.
///
/// # Examples
///
/// ```
/// use wdl_grammar as wdl;
///
/// let rule = wdl::v1::get_rule("document");
/// assert!(matches!(rule, Some(_)));
///
/// let rule = wdl::v1::get_rule("foo-bar-baz-rule");
/// assert!(matches!(rule, None));
/// ```
pub fn get_rule(rule: &str) -> Option<Rule> {
    for candidate in Rule::all_rules() {
        if format!("{:?}", candidate) == rule {
            return Some(*candidate);
        }
    }

    None
}
