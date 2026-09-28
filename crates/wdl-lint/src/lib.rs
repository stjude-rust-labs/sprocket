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

use std::collections::HashMap;
use std::sync::LazyLock;

use dyn_clone::DynClone;
use strum::VariantArray;
use wdl_analysis::Example;
use wdl_analysis::RuleMap;
use wdl_analysis::Visitor;
use wdl_ast::SyntaxKind;

pub mod baseline;
pub mod config;
pub(crate) mod fix;
mod linter;
pub mod rules;
mod tags;
pub(crate) mod util;

pub use baseline::Baseline;
pub use baseline::BaselineEntry;
pub use baseline::BaselineMatcher;
pub use config::Config;
#[doc(hidden)]
pub use config::ConfigField;
pub use config::RuleSeverity;
pub use linter::*;
pub use tags::*;
pub use wdl_analysis as analysis;
pub use wdl_ast as ast;

/// The definitions of WDL concepts and terminology used in the linting rules.
pub const DEFINITIONS_TEXT: &str = include_str!("../DEFINITIONS.md");

/// All rule IDs sorted alphabetically.
pub static ALL_RULE_IDS: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut ids: Vec<String> = all_rules(&Config::default())
        .iter()
        .map(|r| r.id().to_string())
        .collect();
    ids.sort();
    ids
});

/// All rule IDs and their exceptable nodes.
pub static RULE_MAP: LazyLock<RuleMap> = LazyLock::new(|| {
    let rules = all_rules(&Config::default());
    let mut map = HashMap::with_capacity(rules.len());
    for rule in rules {
        map.insert(String::from(rule.id()), rule.exceptable_nodes());
    }
    map
});

/// All tag names sorted alphabetically.
pub static ALL_TAG_NAMES: LazyLock<Vec<String>> =
    LazyLock::new(|| ALL_TAGS.iter().map(|t| t.to_string()).collect());

/// All tags sorted alphabetically.
pub static ALL_TAGS: LazyLock<Vec<Tag>> = LazyLock::new(|| {
    let mut tags: Vec<Tag> = Tag::VARIANTS.to_vec();
    tags.sort_by_cached_key(Tag::to_string);
    tags
});

/// A trait implemented by lint rules.
pub trait Rule: Visitor + DynClone {
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

    /// Get a list of examples that would trigger this lint rule.
    fn examples(&self) -> &'static [Example];

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
    fn related_rules(&self) -> &'static [&'static str];
}

dyn_clone::clone_trait_object!(Rule);

/// Gets the lint rules that are enabled by the given configuration.
///
/// Rules with a severity of `off` are not included. To keep `#@ except`
/// directives for those rules from being reported as unknown, register
/// [`RULE_MAP`] with the validator (see
/// [`Validator::extend_rules`](wdl_analysis::Validator::extend_rules)).
pub fn rules(config: &Config) -> Vec<Box<dyn Rule + Send + Sync>> {
    all_rules(config)
        .into_iter()
        .filter(|rule| config.severity(rule.id()) != Some(RuleSeverity::Off))
        .collect()
}

/// Gets all of the lint rules, including those that are `off`.
fn all_rules(config: &Config) -> Vec<Box<dyn Rule + Send + Sync>> {
    let rules: Vec<Box<dyn Rule + Send + Sync>> = vec![
        Box::new(rules::HereDocCommandsRule::new(config)),
        Box::new(rules::SnakeCaseRule::new(config)),
        Box::new(rules::RuntimeSectionRule::new(config)),
        Box::new(rules::MissingParameterMetaRule::new(config)),
        Box::new(rules::ExtraneousParameterMetaRule::new(config)),
        Box::new(rules::ParameterMetaOrderRule::new(config)),
        Box::new(rules::ImportPlacementRule::new(config)),
        Box::new(rules::PascalCaseRule::new(config)),
        Box::new(rules::MetaSectionsRule::new(config)),
        Box::new(rules::CallInputKeywordRule::new(config)),
        Box::new(rules::MetaDescriptionRule::new(config)),
        Box::new(rules::UnknownRuntimeKeysRule::new(config)),
        Box::new(rules::DeprecatedRuntimeKeyRule::new(config)),
        Box::new(rules::RecommendedRuntimeKeysRule::new(config)),
        Box::new(rules::EmptyDocCommentRule::new(config)),
        Box::new(rules::DocMetaStringsRule::new(config)),
        Box::new(rules::TodoCommentRule::new(config)),
        Box::new(rules::MatchingOutputMetaRule::new(config)),
        Box::new(rules::OutputMetaOrderRule::new(config)),
        Box::new(rules::InputNameRule::new(config)),
        Box::new(rules::OutputNameRule::new(config)),
        Box::new(rules::DeclarationNameRule::new(config)),
        Box::new(rules::RedundantNone::new(config)),
        Box::new(rules::HostPathLiteralsRule::new(config)),
        Box::new(rules::ContainerUriRule::new(config)),
        Box::new(rules::MutableContainerTagRule::new(config)),
        Box::new(rules::RedundantContainerArrayRule::new(config)),
        Box::new(rules::RequirementsSectionRule::new(config)),
        Box::new(rules::ParameterDescriptionRule::new(config)),
        Box::new(rules::ConciseInputRule::new(config)),
        Box::new(rules::ShellCheckRule::new(config)),
        Box::new(rules::DescriptionLengthRule::new(config)),
        Box::new(rules::DocCommentTabsRule::new(config)),
        Box::new(rules::UnusedDocCommentsRule::new(config)),
        Box::new(rules::DenyGlobStar::new(config)),
        Box::new(rules::EmptyOutputs::new(config)),
        Box::new(rules::BashSetSyntax::new(config)),
        Box::new(rules::InlineInstall::new(config)),
    ];

    // Ensure all the rule IDs are unique and pascal case and that related rules
    // are valid, exist and not self-referential.
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

            if config.severity(r.id()).is_none() {
                panic!("lint rule `{id}` has no configuration", id = r.id());
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
