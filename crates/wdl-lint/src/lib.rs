//! Lint rules for Workflow Description Language (WDL) documents.
#![doc = include_str!("../RULES.md")]
//! # Definitions
#![doc = include_str!("../DEFINITIONS.md")]
//! # Examples
//!
//! An example of parsing a WDL document and linting it:
//!
//! ```rust
//! # let source = "version 1.1\nworkflow test {}";
//! use wdl_lint::Linter;
//! use wdl_lint::analysis::Validator;
//! use wdl_lint::analysis::document::Document;
//!
//! let mut validator = Validator::default();
//! validator.add_visitor(Linter::default());
//! ```

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::collections::HashSet;
use std::sync::LazyLock;

use wdl_analysis::Visitor;
use wdl_ast::SyntaxKind;

mod config;
pub(crate) mod fix;
mod linter;
pub mod rules;
mod tags;
pub(crate) mod util;

pub use config::Config;
pub use linter::*;
pub use tags::*;
pub use util::find_nearest_rule;
pub use wdl_analysis as analysis;
pub use wdl_ast as ast;

/// The definitions of WDL concepts and terminology used in the linting rules.
pub const DEFINITIONS_TEXT: &str = include_str!("../DEFINITIONS.md");

/// All rule IDs sorted alphabetically.
pub static ALL_RULE_IDS: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut ids: Vec<String> = rules(&Config::default())
        .iter()
        .map(|r| r.id().to_string())
        .collect();
    ids.sort();
    ids
});

/// All tag names sorted alphabetically.
pub static ALL_TAG_NAMES: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut tags: HashSet<Tag> = HashSet::new();
    for rule in rules(&Config::default()) {
        for tag in rule.tags().iter() {
            tags.insert(tag);
        }
    }
    let mut tag_names: Vec<String> = tags.into_iter().map(|t| t.to_string()).collect();
    tag_names.sort();
    tag_names
});

/// A trait implemented by lint rules.
pub trait Rule: Visitor {
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

/// Gets all of the lint rules.
pub fn rules(config: &Config) -> Vec<Box<dyn Rule>> {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::<rules::DoubleQuotesRule>::default(),
        Box::<rules::HereDocCommandsRule>::default(),
        Box::<rules::SnakeCaseRule>::default(),
        Box::<rules::RuntimeSectionRule>::default(),
        Box::<rules::ParameterMetaMatchedRule>::default(),
        Box::<rules::CommandSectionIndentationRule>::default(),
        Box::<rules::ImportPlacementRule>::default(),
        Box::<rules::PascalCaseRule>::default(),
        Box::<rules::MetaSectionsRule>::default(),
        Box::<rules::ImportSortedRule>::default(),
        Box::<rules::InputSortedRule>::default(),
        Box::<rules::ConsistentNewlinesRule>::default(),
        Box::<rules::CallInputKeywordRule>::default(),
        Box::<rules::SectionOrderingRule>::default(),
        Box::<rules::DeprecatedObjectRule>::default(),
        Box::<rules::MetaDescriptionRule>::default(),
        Box::<rules::DeprecatedPlaceholderRule>::default(),
        Box::new(rules::ExpectedRuntimeKeysRule::new(config)),
        Box::<rules::DocMetaStringsRule>::default(),
        Box::<rules::TodoCommentRule>::default(),
        Box::<rules::MatchingOutputMetaRule<'_>>::default(),
        Box::<rules::ElementSpacingRule>::default(),
        Box::<rules::ExpressionSpacingRule>::default(),
        Box::<rules::InputNameRule>::default(),
        Box::<rules::OutputNameRule>::default(),
        Box::<rules::DeclarationNameRule>::default(),
        Box::<rules::RedundantNone>::default(),
        Box::<rules::ContainerUriRule>::default(),
        Box::<rules::RequirementsSectionRule>::default(),
        Box::<rules::KnownRulesRule>::default(),
        Box::<rules::ExceptDirectiveValidRule>::default(),
        Box::<rules::ConciseInputRule>::default(),
        Box::<rules::ShellCheckRule>::default(),
        Box::<rules::DescriptionLengthRule>::default(),
        Box::<rules::DocCommentTabsRule>::default(),
    ];

    // Ensure all the rule IDs are unique and pascal case and that related rules are
    // valid, exist and not self-referential.
    #[cfg(debug_assertions)]
    {
        use std::collections::HashSet;

        use convert_case::Case;
        use convert_case::Casing;
        let mut lint_set = HashSet::new();
        let analysis_set: HashSet<&str> =
            HashSet::from_iter(analysis::rules().iter().map(|r| r.id()));
        for r in &rules {
            if r.id().to_case(Case::Pascal) != r.id() {
                panic!("lint rule id `{id}` is not pascal case", id = r.id());
            }

            if !lint_set.insert(r.id()) {
                panic!("duplicate rule id `{id}`", id = r.id());
            }

            if analysis_set.contains(r.id()) {
                panic!("rule id `{id}` is in use by wdl-analysis", id = r.id());
            }
            let self_id = &r.id();
            for related_id in r.related_rules() {
                if related_id == self_id {
                    panic!(
                        "Rule `{self_id}` refers to itself in its related rules. This is not \
                         allowed."
                    );
                }
            }
        }
    }

    rules
}
