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

pub(crate) mod fix;
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
    "UnnecessaryFunctionCall",
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

    /// Gets the ID of rules that are related to this rule.
    ///
    /// This can be used by tools (like `sprocket explain`) to suggest other
    /// relevant rules to the user based on potential logical connections or
    /// common co-occurrences of issues.
    fn related_rules(&self) -> &[&'static str];
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
        Box::<rules::DisallowedDeclarationNameRule>::default(),
        Box::<rules::ContainerValue>::default(),
        Box::<rules::MissingRequirementsRule>::default(),
        Box::<rules::UnknownRule>::default(),
        Box::<rules::MisplacedLintDirectiveRule>::default(),
        Box::<rules::VersionFormattingRule>::default(),
        Box::<rules::PreambleCommentAfterVersionRule>::default(),
        Box::<rules::MalformedLintDirectiveRule>::default(),
        Box::<rules::RedundantInputAssignment>::default(),
    ];

    // Ensure all the rule IDs are unique and pascal case and that related rules are
    // valid, exist and not self-referential.
    #[cfg(debug_assertions)]
    {
        use std::collections::HashSet;

        use convert_case::Case;
        use convert_case::Casing;

        let mut rule_ids = HashSet::new();
        for r in &rules {
            if r.id().to_case(Case::Pascal) != r.id() {
                panic!("lint rule id `{id}` is not pascal case", id = r.id());
            }

            if !rule_ids.insert(r.id()) {
                panic!("duplicate rule id `{id}`", id = r.id());
            }

            if RESERVED_RULE_IDS.contains(&r.id()) {
                panic!("rule id `{id}` is reserved", id = r.id());
            }
        }

        for r in &rules {
            let self_id = &r.id();
            for related_id in r.related_rules() {
                if !rule_ids.contains(related_id) {
                    // If a related rule is a reserved rule, then it's fine.
                    if RESERVED_RULE_IDS.contains(related_id) {
                        continue;
                    }
                    panic!(
                        "Rule `{id}` refers to a related rule `{related_id}` which does not exist \
                         in the default rule set.",
                        id = r.id(),
                        related_id = related_id
                    );
                }
                if related_id == self_id {
                    panic!(
                        "Rule `{id}` refers to itself in its related rules. This is not allowed.",
                        id = self_id
                    );
                }
            }
        }
    }

    rules
}

/// Gets the optional rule set.
pub fn optional_rules() -> Vec<Box<dyn Rule>> {
    let opt_rules: Vec<Box<dyn Rule>> = vec![Box::<rules::ShellCheckRule>::default()];

    // Ensure all the rule IDs are unique and pascal case and that related rules are
    // valid, exist and not self-referential.
    #[cfg(debug_assertions)]
    {
        use std::collections::HashSet;
        use std::iter::FromIterator;

        use convert_case::Case;
        use convert_case::Casing;

        let default_rule_ids: HashSet<&str> = HashSet::from_iter(rules().iter().map(|r| r.id()));
        let mut opt_rule_ids = HashSet::new();

        for r in &opt_rules {
            let id = r.id();
            if id.to_case(Case::Pascal) != id {
                panic!("lint rule id `{id}` is not pascal case", id = id);
            }

            if default_rule_ids.contains(id) {
                panic!(
                    "optional rule id `{id}` conflicts with a default rule id",
                    id = id
                );
            }

            if !opt_rule_ids.insert(id) {
                panic!("duplicate rule id `{id}`", id = id);
            }

            if RESERVED_RULE_IDS.contains(&id) {
                panic!("rule id `{id}` is reserved", id = id);
            }
        }

        let all_rule_ids: HashSet<&str> = default_rule_ids.union(&opt_rule_ids).copied().collect();
        for r in &opt_rules {
            let self_id = &r.id();
            for related_id in r.related_rules() {
                if !all_rule_ids.contains(related_id) {
                    panic!(
                        "optional rule `{id}` refers to a related rule `{related_id}` which does \
                         not exist.",
                        id = r.id(),
                        related_id = related_id
                    );
                }

                if related_id == self_id {
                    panic!(
                        "optional rule `{id}` refers to itself in its related rules. This is not \
                         allowed.",
                        id = self_id
                    );
                }
            }
        }
    }

    opt_rules
}
