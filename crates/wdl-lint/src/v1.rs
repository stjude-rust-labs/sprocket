//! Module for V1 lint rules.

use convert_case::Case;
use convert_case::Casing;
use wdl_ast::experimental::v1::Visitor;
use wdl_ast::experimental::Diagnostics;

use crate::TagSet;

mod double_quotes;
mod ending_newline;
mod missing_runtime;
mod no_curly_commands;
mod preamble_comments;
mod preamble_whitespace;
mod snake_case;

pub use double_quotes::*;
pub use ending_newline::*;
pub use missing_runtime::*;
pub use no_curly_commands::*;
pub use preamble_comments::*;
pub use preamble_whitespace::*;
pub use snake_case::*;

/// A trait implemented by lint rules.
pub trait Rule {
    /// The unique identifier for the lint rule.
    ///
    /// The identifier is required to be pascal case.
    ///
    /// This is what will show up in style guides and is the identifier by which
    /// a lint rule is disabled.
    fn id(&self) -> &'static str;

    /// A short, single sentence description of the lint rule.
    fn description(&self) -> &'static str;

    /// Get the long-form explanation of the lint rule.
    fn explanation(&self) -> &'static str;

    /// Get the tags of the lint rule.
    fn tags(&self) -> TagSet;

    /// Gets the optional URL of the lint rule.
    fn url(&self) -> Option<&'static str> {
        None
    }

    /// Gets the visitor of the rule.
    fn visitor(&self) -> Box<dyn Visitor<State = Diagnostics>>;
}

/// Gets the default V1 rule set.
pub fn rules() -> Vec<Box<dyn Rule>> {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(DoubleQuotesRule),
        Box::new(NoCurlyCommandsRule),
        Box::new(SnakeCaseRule),
        Box::new(MissingRuntimeRule),
        Box::new(EndingNewlineRule),
        Box::new(PreambleWhitespaceRule),
        Box::new(PreambleCommentsRule),
    ];

    // Ensure all the rule ids are unique and pascal case
    #[cfg(debug_assertions)]
    {
        let mut set = std::collections::HashSet::new();
        for r in rules.iter() {
            if r.id().to_case(Case::Pascal) != r.id() {
                panic!("lint rule id `{id}` is not pascal case", id = r.id());
            }

            if !set.insert(r.id()) {
                panic!("duplicate rule id `{id}`", id = r.id());
            }
        }
    }

    rules
}
