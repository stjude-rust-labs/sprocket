//! Lint rules for Workflow Description Language (WDL) documents.
#![doc = include_str!("../RULES.md")]
//! # Examples
//!
//! An example of parsing a WDL document and linting it:
//!
//! ```rust
//! # let source = "version 1.1\nworkflow test {}";
//! use wdl_lint::ast::Document;
//! use wdl_lint::ast::Validator;
//! use wdl_lint::rules;
//!
//! match Document::parse(source).into_result() {
//!     Ok(document) => {
//!         let mut validator = Validator::default();
//!         validator.add_visitors(rules().into_iter().map(|r| r.visitor()));
//!         match validator.validate(&document) {
//!             Ok(_) => {
//!                 // The document was valid WDL and passed all lints
//!             }
//!             Err(diagnostics) => {
//!                 // Handle the failure to validate
//!             }
//!         }
//!     }
//!     Err(diagnostics) => {
//!         // Handle the failure to parse
//!     }
//! }
//! ```

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

pub mod rules;
mod tags;
pub(crate) mod util;

pub use tags::*;
pub use wdl_ast as ast;
use wdl_ast::Diagnostics;
use wdl_ast::Visitor;

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

/// Gets the default rule set.
pub fn rules() -> Vec<Box<dyn Rule>> {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(rules::DoubleQuotesRule),
        Box::new(rules::NoCurlyCommandsRule),
        Box::new(rules::SnakeCaseRule),
        Box::new(rules::MissingRuntimeRule),
        Box::new(rules::EndingNewlineRule),
        Box::new(rules::PreambleWhitespaceRule),
        Box::new(rules::PreambleCommentsRule),
        Box::new(rules::MatchingParameterMetaRule),
        Box::new(rules::WhitespaceRule),
        Box::new(rules::CommandSectionMixedIndentationRule),
        Box::new(rules::ImportPlacementRule),
        Box::new(rules::PascalCaseRule),
        Box::new(rules::ImportWhitespaceRule),
        Box::new(rules::MissingMetasRule),
        Box::new(rules::MissingOutputRule),
    ];

    // Ensure all the rule ids are unique and pascal case
    #[cfg(debug_assertions)]
    {
        use convert_case::Case;
        use convert_case::Casing;
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
