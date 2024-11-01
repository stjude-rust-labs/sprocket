//! Lint rules for Workflow Description Language (WDL) documents.
#![doc = include_str!("../RULES.md")]
//! # Examples
//!
//! An example of parsing a WDL document and linting it:
//!
//! ```rust
//! # let source = "version 1.1\nworkflow test {}";
//! use wdl_lint::LintVisitor;
//! use wdl_lint::ast::Document;
//! use wdl_lint::ast::Validator;
//!
//! let (document, diagnostics) = Document::parse(source);
//! if !diagnostics.is_empty() {
//!     // Handle the failure to parse
//! }
//!
//! let mut validator = Validator::default();
//! validator.add_visitor(LintVisitor::default());
//! if let Err(diagnostics) = validator.validate(&document) {
//!     // Handle the failure to validate
//! }
//! ```

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

use wdl_ast::Diagnostics;
use wdl_ast::SyntaxKind;
use wdl_ast::Visitor;

pub mod rules;
mod tags;
pub(crate) mod util;
mod visitor;

pub use tags::*;
pub use visitor::*;
pub use wdl_ast as ast;

/// The reserved rule identifiers that are used by analysis.
pub const RESERVED_RULE_IDS: &[&str] = &[
    "UnusedImport",
    "UnusedInput",
    "UnusedDeclaration",
    "UnusedCall",
];

/// A trait implemented by lint rules.
pub trait Rule: Visitor<State = Diagnostics> {
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

    /// Gets the nodes that are exceptable for this rule.
    ///
    /// If `None` is returned, all nodes are exceptable.
    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]>;
}

/// Gets the default rule set.
pub fn rules() -> Vec<Box<dyn Rule>> {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::<rules::DoubleQuotesRule>::default(),
        Box::<rules::NoCurlyCommandsRule>::default(),
        Box::<rules::SnakeCaseRule>::default(),
        Box::<rules::MissingRuntimeRule>::default(),
        Box::<rules::EndingNewlineRule>::default(),
        Box::<rules::PreambleFormattingRule>::default(),
        Box::<rules::MatchingParameterMetaRule>::default(),
        Box::<rules::WhitespaceRule>::default(),
        Box::<rules::CommandSectionMixedIndentationRule>::default(),
        Box::<rules::ImportPlacementRule>::default(),
        Box::<rules::PascalCaseRule>::default(),
        Box::<rules::ImportWhitespaceRule>::default(),
        Box::<rules::MissingMetasRule>::default(),
        Box::<rules::MissingOutputRule>::default(),
        Box::<rules::ImportSortRule>::default(),
        Box::<rules::InputNotSortedRule>::default(),
        Box::<rules::LineWidthRule>::default(),
        Box::<rules::InconsistentNewlinesRule>::default(),
        Box::<rules::CallInputSpacingRule>::default(),
        Box::<rules::SectionOrderingRule>::default(),
        Box::<rules::DeprecatedObjectRule>::default(),
        Box::<rules::DescriptionMissingRule>::default(),
        Box::<rules::DeprecatedPlaceholderOptionRule>::default(),
        Box::<rules::RuntimeSectionKeysRule>::default(),
        Box::<rules::TodoRule>::default(),
        Box::<rules::NonmatchingOutputRule<'_>>::default(),
        Box::<rules::CommentWhitespaceRule>::default(),
        Box::<rules::TrailingCommaRule>::default(),
        Box::<rules::BlankLinesBetweenElementsRule>::default(),
        Box::<rules::KeyValuePairsRule>::default(),
        Box::<rules::ExpressionSpacingRule>::default(),
        Box::<rules::DisallowedInputNameRule>::default(),
        Box::<rules::DisallowedOutputNameRule>::default(),
        Box::<rules::ContainerValue>::default(),
        Box::<rules::MissingRequirementsRule>::default(),
        Box::<rules::UnknownRule>::default(),
        Box::<rules::MisplacedLintDirectiveRule>::default(),
        Box::<rules::VersionFormattingRule>::default(),
        Box::<rules::PreambleCommentAfterVersionRule>::default(),
        Box::<rules::MalformedLintDirectiveRule>::default(),
        Box::<rules::RedundantInputAssignment>::default(),
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

            if RESERVED_RULE_IDS.contains(&r.id()) {
                panic!("rule id `{id}` is reserved", id = r.id());
            }
        }
    }

    rules
}
