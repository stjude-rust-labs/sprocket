//! WDL 1.x.
//!
//! ## Linting Rules
//!
//! The following linting rules are supported for WDL 1.x:
//!
//! | Name                | Code      | Group    | Module                        |
//! |:--------------------|:---------:|:--------:|:-----------------------------:|
//! | `whitespace`        | `v1::001` | Style    | [Link](lint::Whitespace)      |
//! | `no_curly_commands` | `v1::002` | Pedantic | [Link](lint::NoCurlyCommands) |

use pest::Parser as _;

use crate::core::lint::Linter;
use crate::core::validation::Validator;
use crate::core::Tree;
use crate::Error;
use crate::Result;

pub mod lint;
mod parse;
#[cfg(test)]
mod tests;
pub mod validation;

pub(crate) use parse::Parser;
pub use parse::Rule;

/// Parses a WDL 1.x input according to the specified [Rule].
///
/// # Examples
///
/// ```
/// use wdl_grammar as grammar;
///
/// use grammar::v1::Rule;
/// use grammar::Error;
///
/// // A valid grammar tree.
///
/// let tree = grammar::v1::parse(Rule::document, "version 1.1\n \n")?;
///
/// let warnings = tree.warnings().unwrap();
/// assert_eq!(warnings.len(), 1);
///
/// let warning = warnings.first().unwrap();
/// assert_eq!(
///     warning.to_string(),
///     String::from("[v1::001::Style/Low] line contains only whitespace at 2:*")
/// );
///
/// let pair = tree.into_inner().next().unwrap();
/// assert!(matches!(pair.as_rule(), Rule::document));
///
/// // An invalid grammar tree due to pest parsing.
///
/// let err = grammar::v1::parse(Rule::document, "Hello, world!").unwrap_err();
/// assert!(matches!(err, Error::Parse(_)));
///
/// // An invalid grammar tree due to our additional validation.
///
/// let err = grammar::v1::parse(
///     Rule::document,
///     "version 1.1
/// task test {
///     output {
///         String hello = \"\\.\"
///     }
/// }",
/// )
/// .unwrap_err();
///
/// assert!(matches!(err, Error::Validation(_)));
///
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn parse(rule: Rule, input: &str) -> Result<Tree<'_, Rule>, Rule> {
    let tree = Parser::parse(rule, input)
        .map_err(Box::new)
        .map_err(Error::Parse)?;

    let validations = validation::rules();
    Validator::validate(tree.clone(), validations.as_ref())
        .map_err(Box::new)
        .map_err(Error::Validation)?;

    let lints = lint::rules();
    let warnings = Linter::lint(tree.clone(), lints.as_ref()).map_err(Error::Lint)?;

    Ok(Tree::new(tree, warnings))
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
/// assert!(!matches!(rule, Some(_)));
/// ```
pub fn get_rule(rule: &str) -> Option<Rule> {
    for candidate in Rule::all_rules() {
        if format!("{:?}", candidate) == rule {
            return Some(*candidate);
        }
    }

    None
}
