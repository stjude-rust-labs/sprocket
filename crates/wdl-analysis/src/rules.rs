//! Implementation of analysis rules.

pub mod util;

use std::collections::HashMap;
use std::sync::LazyLock;

use wdl_ast::Severity;
use wdl_grammar::SyntaxKind;

use crate::RuleMap;

/// All rule IDs sorted alphabetically.
pub static ALL_RULE_IDS: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut ids: Vec<String> = rules().iter().map(|r| r.id().to_string()).collect();
    ids.sort();
    ids
});

/// All rules and their exceptable nodes.
pub(crate) static RULE_MAP: LazyLock<RuleMap> = LazyLock::new(|| {
    let rules = rules();
    let mut map = HashMap::with_capacity(rules.len());
    for rule in rules {
        map.insert(String::from(rule.id()), rule.exceptable_nodes());
    }
    map
});

/// A labeled WDL code snippet.
#[derive(Copy, Clone, Debug)]
pub struct LabeledSnippet {
    /// A label for the snippet.
    pub label: Option<&'static str>,
    /// A WDL code snippet.
    pub snippet: &'static str,
}

/// A lint rule example.
#[derive(Copy, Clone, Debug)]
pub struct Example {
    /// A snippet that will trigger the target lint rule.
    pub negative: LabeledSnippet,
    /// A revision of the negative snippet that will no longer trigger the rule.
    pub revised: Option<LabeledSnippet>,
}

/// A trait implemented by analysis rules.
pub trait Rule: Send + Sync {
    /// The unique identifier for the rule.
    ///
    /// The identifier is required to be pascal case and it is the identifier by
    /// which a rule is excepted or denied.
    fn id(&self) -> &'static str;

    /// A short, single sentence description of the rule.
    fn description(&self) -> &'static str;

    /// Get the long-form explanation of the rule.
    fn explanation(&self) -> &'static str;

    /// Get a list of examples that would trigger this rule.
    fn examples(&self) -> &'static [Example];

    /// Gets the nodes that are exceptable for this rule.
    ///
    /// If `None` is returned, all nodes are exceptable.
    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]>;

    /// Denies the rule.
    ///
    /// Denying the rule treats any diagnostics it emits as an error.
    fn deny(&mut self);

    /// Gets the severity of the rule.
    fn severity(&self) -> Severity;
}

/// Gets the list of all analysis rules.
pub fn rules() -> Vec<Box<dyn Rule>> {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::<UnusedImportRule>::default(),
        Box::<UnusedInputRule>::default(),
        Box::<UnusedDeclarationRule>::default(),
        Box::<UnusedCallRule>::default(),
        Box::<UnnecessaryFunctionCall>::default(),
        Box::<UsingFallbackVersion>::default(),
        Box::<MisleadingDeclarationOrderRule>::default(),
        Box::<MeaninglessLintDirective>::default(),
        Box::<KnownRulesRule>::default(),
        Box::<ExceptDirectiveValidRule>::default(),
        Box::<CommandSectionIndentationRule>::default(),
        Box::<DeprecatedObjectRule>::default(),
        Box::<DeprecatedPlaceholderRule>::default(),
        Box::<DeprecatedRuntimeSectionRule>::default(),
    ];

    // Ensure all the rule ids are unique and pascal case
    #[cfg(debug_assertions)]
    {
        use convert_case::Case;
        use convert_case::Casing;
        let mut set = std::collections::HashSet::new();
        for r in rules.iter() {
            if r.id().to_case(Case::Pascal) != r.id() {
                panic!("analysis rule id `{id}` is not pascal case", id = r.id());
            }

            if !set.insert(r.id()) {
                panic!("duplicate rule id `{id}`", id = r.id());
            }
        }
    }

    rules
}

/// Represents the unused import rule.
#[derive(Debug, Clone, Copy)]
pub struct UnusedImportRule(Severity);

impl UnusedImportRule {
    /// See [`Self::exceptable_nodes()`].
    pub const EXCEPTABLE_NODES: Option<&'static [SyntaxKind]> = Some(&[
        SyntaxKind::VersionStatementNode,
        SyntaxKind::ImportStatementNode,
    ]);
    /// The rule identifier for unused import warnings.
    pub const ID: &'static str = "UnusedImport";

    /// Creates a new unused import rule.
    pub fn new() -> Self {
        Self(Severity::Warning)
    }
}

impl Default for UnusedImportRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for UnusedImportRule {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn description(&self) -> &'static str {
        "Ensures that import namespaces are used in the importing document."
    }

    fn explanation(&self) -> &'static str {
        "Imported WDL documents should be used in the document that imports them. Unused imports \
         impact parsing and evaluation performance."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

import "bar.wdl"
import "foo.wdl" as used

workflow example {
    call used.test
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: Some("Consider removing the import entirely"),
                snippet: r#"version 1.3

import "foo.wdl" as used

workflow example {
    call used.test
}
"#,
            }),
        }]
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Self::EXCEPTABLE_NODES
    }

    fn deny(&mut self) {
        self.0 = Severity::Error;
    }

    fn severity(&self) -> Severity {
        self.0
    }
}

/// Represents the unused input rule.
#[derive(Debug, Clone, Copy)]
pub struct UnusedInputRule(Severity);

impl UnusedInputRule {
    /// See [`Self::exceptable_nodes()`].
    pub const EXCEPTABLE_NODES: Option<&'static [SyntaxKind]> = Some(&[
        SyntaxKind::VersionStatementNode,
        SyntaxKind::WorkflowDefinitionNode,
        SyntaxKind::TaskDefinitionNode,
        SyntaxKind::BoundDeclNode,
        SyntaxKind::UnboundDeclNode,
    ]);
    /// The rule identifier for unused input warnings.
    pub const ID: &str = "UnusedInput";

    /// Creates a new unused input rule.
    pub fn new() -> Self {
        Self(Severity::Warning)
    }
}

impl Default for UnusedInputRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for UnusedInputRule {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn description(&self) -> &'static str {
        "Ensures that task or workspace inputs are used within the declaring task or workspace."
    }

    fn explanation(&self) -> &'static str {
        "Unused inputs degrade evaluation performance and reduce the clarity of the code. Unused \
         file inputs in tasks can also cause unnecessary file localizations."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

workflow example {
    input {
        String unused
    }
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: Some("Consider removing the input entirely"),
                snippet: r#"version 1.2

workflow example {
    input {
    }
}
"#,
            }),
        }]
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Self::EXCEPTABLE_NODES
    }

    fn deny(&mut self) {
        self.0 = Severity::Error;
    }

    fn severity(&self) -> Severity {
        self.0
    }
}

/// Represents the unused declaration rule.
#[derive(Debug, Clone, Copy)]
pub struct UnusedDeclarationRule(Severity);

impl UnusedDeclarationRule {
    /// See [`Self::exceptable_nodes()`].
    pub const EXCEPTABLE_NODES: Option<&'static [SyntaxKind]> = Some(&[
        SyntaxKind::VersionStatementNode,
        SyntaxKind::WorkflowDefinitionNode,
        SyntaxKind::TaskDefinitionNode,
        SyntaxKind::BoundDeclNode,
        SyntaxKind::UnboundDeclNode,
    ]);
    /// The rule identifier for unused declaration warnings.
    pub const ID: &str = "UnusedDeclaration";

    /// Creates a new unused declaration rule.
    pub fn new() -> Self {
        Self(Severity::Warning)
    }
}

impl Default for UnusedDeclarationRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for UnusedDeclarationRule {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn description(&self) -> &'static str {
        "Ensures that private declarations in tasks or workspaces are used within the declaring \
         task or workspace."
    }

    fn explanation(&self) -> &'static str {
        "Unused private declarations degrade evaluation performance and reduce the clarity of the \
         code."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

workflow example {
    String unused = "this will produce a warning"
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: Some("Consider removing the declaration entirely"),
                snippet: r#"version 1.2

workflow example {
}
"#,
            }),
        }]
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Self::EXCEPTABLE_NODES
    }

    fn deny(&mut self) {
        self.0 = Severity::Error;
    }

    fn severity(&self) -> Severity {
        self.0
    }
}

/// Represents the unused call rule.
#[derive(Debug, Clone, Copy)]
pub struct UnusedCallRule(Severity);

impl UnusedCallRule {
    /// See [`Self::exceptable_nodes()`].
    pub const EXCEPTABLE_NODES: Option<&'static [SyntaxKind]> = Some(&[
        SyntaxKind::VersionStatementNode,
        SyntaxKind::WorkflowDefinitionNode,
        SyntaxKind::CallStatementNode,
    ]);
    /// The rule identifier for unused call warnings.
    pub const ID: &str = "UnusedCall";

    /// Creates a new unused call rule.
    pub fn new() -> Self {
        Self(Severity::Warning)
    }
}

impl Default for UnusedCallRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for UnusedCallRule {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn description(&self) -> &'static str {
        "Ensures that outputs of a call statement are used in the declaring workflow."
    }

    fn explanation(&self) -> &'static str {
        "Unused calls may cause unnecessary consumption of compute resources."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

workflow example {
    # The output of `do_work` is never used
    call do_work
}

task do_work {
    command <<<
    >>>

    output {
        Int x = 0
    }
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: Some("Consider removing the call entirely"),
                snippet: r#"version 1.2

workflow example {
}

task do_work {
    command <<<
    >>>

    output {
        Int x = 0
    }
}
"#,
            }),
        }]
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Self::EXCEPTABLE_NODES
    }

    fn deny(&mut self) {
        self.0 = Severity::Error;
    }

    fn severity(&self) -> Severity {
        self.0
    }
}

/// Represents the unnecessary call rule.
#[derive(Debug, Clone, Copy)]
pub struct UnnecessaryFunctionCall(Severity);

impl UnnecessaryFunctionCall {
    /// See [`Self::exceptable_nodes()`].
    pub const EXCEPTABLE_NODES: Option<&'static [SyntaxKind]> = Some(&[
        SyntaxKind::VersionStatementNode,
        SyntaxKind::WorkflowDefinitionNode,
        SyntaxKind::TaskDefinitionNode,
        SyntaxKind::BoundDeclNode,
    ]);
    /// The rule identifier for unnecessary function call warnings.
    pub const ID: &str = "UnnecessaryFunctionCall";

    /// Creates a new unnecessary function call rule.
    pub fn new() -> Self {
        Self(Severity::Warning)
    }
}

impl Default for UnnecessaryFunctionCall {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for UnnecessaryFunctionCall {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn description(&self) -> &'static str {
        "Ensures that function calls are necessary."
    }

    fn explanation(&self) -> &'static str {
        "Unnecessary function calls may impact evaluation performance."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

workflow example {
    # Calls to `defined` on values that are statically
    # known to be non-None are unnecessary.
    Boolean exists = defined("hello")
}
"#,
            },
            revised: None,
        }]
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Self::EXCEPTABLE_NODES
    }

    fn deny(&mut self) {
        self.0 = Severity::Error;
    }

    fn severity(&self) -> Severity {
        self.0
    }
}

/// Represents the using fallback version rule.
#[derive(Debug, Clone, Copy)]
pub struct UsingFallbackVersion(Severity);

impl UsingFallbackVersion {
    /// See [`Self::exceptable_nodes()`].
    pub const EXCEPTABLE_NODES: Option<&'static [SyntaxKind]> =
        Some(&[SyntaxKind::VersionStatementNode]);
    /// The rule identifier for unsupported version fallback warnings.
    pub const ID: &str = "UsingFallbackVersion";

    /// Creates a new using fallback version rule.
    pub fn new() -> Self {
        Self(Severity::Warning)
    }
}

impl Default for UsingFallbackVersion {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for UsingFallbackVersion {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn description(&self) -> &'static str {
        "Warns if interpretation of a document with an unsupported version falls back to a default."
    }

    fn explanation(&self) -> &'static str {
        "A document with an unsupported version may have unpredictable behavior if interpreted as \
         a different version."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"# Not a valid version. If a fallback version is configured,
# the document will be interpreted as that version.
version development

workflow example {
}
"#,
            },
            revised: None,
        }]
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Self::EXCEPTABLE_NODES
    }

    fn deny(&mut self) {
        self.0 = Severity::Error;
    }

    fn severity(&self) -> Severity {
        self.0
    }
}

/// Represents the meaningless lint directive rule.
#[derive(Debug, Clone, Copy)]
pub struct MeaninglessLintDirective(Severity);

impl MeaninglessLintDirective {
    /// See [`Self::exceptable_nodes()`].
    pub const EXCEPTABLE_NODES: Option<&'static [SyntaxKind]> = None;
    /// The rule identifier for meaningless lint directive warnings.
    pub const ID: &str = "MeaninglessLintDirective";

    /// Creates a new meaningless lint directive rule.
    pub fn new() -> Self {
        Self(Severity::Note)
    }
}

impl Default for MeaninglessLintDirective {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for MeaninglessLintDirective {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn description(&self) -> &'static str {
        "Warns if an `#@ except:` comment doesn't actually suppress a lint."
    }

    fn explanation(&self) -> &'static str {
        "Unused `#@ except:` comments are likely leftovers of refactoring or debugging, and can \
         reduce the clarity of the code. It is best to remove them."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

task do_work {
    command <<<
        echo "Lots of hard work!"
    >>>

    output {
        String result = read_string(stdout())
    }
}

# We except `UnusedCall` unnecessarily.
workflow calculate {
    #@ except: UnusedCall
    call do_work

    output {
        # We're using the result here!
        String result = do_work.result
    }
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: Some("Consider removing the unused exception"),
                snippet: r#"version 1.3

task do_work {
    command <<<
        echo "Lots of hard work!"
    >>>

    output {
        String result = read_string(stdout())
    }
}

workflow calculate {
    call do_work

    output {
        String result = do_work.result
    }
}
"#,
            }),
        }]
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Self::EXCEPTABLE_NODES
    }

    fn deny(&mut self) {
        self.0 = Severity::Error;
    }

    fn severity(&self) -> Severity {
        self.0
    }
}

/// Represents the using misleading declaration order rule.
#[derive(Debug, Clone, Copy)]
pub struct MisleadingDeclarationOrderRule(Severity);

impl MisleadingDeclarationOrderRule {
    /// See [`Self::exceptable_nodes()`].
    pub const EXCEPTABLE_NODES: Option<&'static [SyntaxKind]> =
        Some(&[SyntaxKind::TaskDefinitionNode, SyntaxKind::BoundDeclNode]);
    /// The rule identifier for misleading declaration order warnings.
    pub const ID: &str = "MisleadingDeclarationOrder";

    /// Creates a new misleading declaration order rule.
    pub fn new() -> Self {
        Self(Severity::Warning)
    }
}

impl Default for MisleadingDeclarationOrderRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for MisleadingDeclarationOrderRule {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn description(&self) -> &'static str {
        "Warns when a variable declaration is placed after a `command` block."
    }

    fn explanation(&self) -> &'static str {
        "WDL tasks are evaluated based on their dependency graph, not top-to-bottom. Variable \
         declarations that appear after `command` sections are visually misleading, as they will \
         still be evaluated _before_ the command is executed."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

task greet {
    String greeting = "Hello"

    command <<<
        echo "~{greeting}, ~{name}!"
    >>>

    String name = "World"
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

task greet {
    String greeting = "Hello"
    String name = "World"

    command <<<
        echo "~{greeting}, ~{name}!"
    >>>
}
"#,
            }),
        }]
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Self::EXCEPTABLE_NODES
    }

    fn deny(&mut self) {
        self.0 = Severity::Error;
    }

    fn severity(&self) -> Severity {
        self.0
    }
}

/// Detects unknown rules within lint directives.
#[derive(Debug, Clone, Copy)]
pub struct KnownRulesRule(Severity);

impl KnownRulesRule {
    /// See [`Self::exceptable_nodes()`].
    pub const EXCEPTABLE_NODES: Option<&'static [SyntaxKind]> =
        Some(&[SyntaxKind::VersionStatementNode]);
    /// The rule identifier for known rules warnings.
    pub const ID: &str = "KnownRules";

    /// Creates a new "known rules" rule.
    pub fn new() -> Self {
        Self(Severity::Note)
    }
}

impl Default for KnownRulesRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for KnownRulesRule {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn description(&self) -> &'static str {
        "Ensures only known rules are used in `except` directives."
    }

    fn explanation(&self) -> &'static str {
        "When writing WDL, `except` directives are used to suppress certain rules. If a rule is \
         unknown, nothing will be suppressed. This rule flags unknown rules as they are often \
         mistakes."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"#@ except: LintThatDoesNotExist
version 1.2

workflow example {
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

workflow example {
}
"#,
            }),
        }]
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Self::EXCEPTABLE_NODES
    }

    fn deny(&mut self) {
        self.0 = Severity::Error;
    }

    fn severity(&self) -> Severity {
        self.0
    }
}

/// Detects improperly placed `except` directives.
#[derive(Debug, Clone, Copy)]
pub struct ExceptDirectiveValidRule(Severity);

impl ExceptDirectiveValidRule {
    /// See [`Self::exceptable_nodes()`].
    pub const EXCEPTABLE_NODES: Option<&'static [SyntaxKind]> =
        Some(&[SyntaxKind::VersionStatementNode]);
    /// The rule identifier for except directive warnings.
    pub const ID: &str = "ExceptDirectiveValid";

    /// Creates a new "except directive valid" rule.
    pub fn new() -> Self {
        Self(Severity::Note)
    }
}

impl Default for ExceptDirectiveValidRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for ExceptDirectiveValidRule {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn description(&self) -> &'static str {
        "Ensures `except` directives are placed correctly to have the intended effect."
    }

    fn explanation(&self) -> &'static str {
        "When writing WDL, `except` directives are used to suppress certain rules. If an `except` \
         directive is misplaced, it will have no effect. This rule flags misplaced `except` \
         directives to ensure they are in the correct location."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

# UsingFallbackVersion exceptions aren't valid
# in this context
#@ except: UsingFallbackVersion
workflow example {
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"#@ except: UsingFallbackVersion
version 1.3

workflow example {
}
"#,
            }),
        }]
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Self::EXCEPTABLE_NODES
    }

    fn deny(&mut self) {
        self.0 = Severity::Error;
    }

    fn severity(&self) -> Severity {
        self.0
    }
}

/// Detects mixed indentation within command sections.
#[derive(Debug, Clone, Copy)]
pub struct CommandSectionIndentationRule(Severity);

impl CommandSectionIndentationRule {
    /// See [`Self::exceptable_nodes()`].
    pub const EXCEPTABLE_NODES: Option<&'static [SyntaxKind]> = Some(&[
        SyntaxKind::VersionStatementNode,
        SyntaxKind::CommandSectionNode,
    ]);
    /// The rule identifier for mixed command section indentation warnings.
    pub const ID: &str = "CommandSectionIndentation";

    /// Creates a new "mixed command section indentation" rule.
    pub fn new() -> Self {
        Self(Severity::Warning)
    }
}

impl Default for CommandSectionIndentationRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for CommandSectionIndentationRule {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn description(&self) -> &'static str {
        "Ensures consistent indentation (no mixed spaces/tabs) within command sections."
    }

    fn explanation(&self) -> &'static str {
        "Mixing indentation (tab and space) characters within the command line causes leading \
         whitespace stripping to be skipped. Commands may be whitespace sensitive, and skipping \
         the whitespace stripping step may cause unexpected behavior."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

task say_greetings {
    input {
        String name
    }

    command <<<
        # this line is prefixed with tabs
		echo "Hello, ~{name}!"
        # this line is prefixed with spaces
        echo "Goodbye, ~{name}!"
    >>>
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

task say_greetings {
    input {
        String name
    }

    command <<<
        # this line is prefixed with spaces
        echo "Hello, ~{name}!"
        # this line is prefixed with spaces
        echo "Goodbye, ~{name}!"
    >>>
}
"#,
            }),
        }]
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Self::EXCEPTABLE_NODES
    }

    fn deny(&mut self) {
        self.0 = Severity::Error;
    }

    fn severity(&self) -> Severity {
        self.0
    }
}

/// Detects the use of the deprecated `Object` types.
#[derive(Debug, Clone, Copy)]
pub struct DeprecatedObjectRule(Severity);

impl DeprecatedObjectRule {
    /// See [`Self::exceptable_nodes()`].
    pub const EXCEPTABLE_NODES: Option<&'static [SyntaxKind]> = Some(&[
        SyntaxKind::VersionStatementNode,
        SyntaxKind::TaskDefinitionNode,
        SyntaxKind::WorkflowDefinitionNode,
        SyntaxKind::BoundDeclNode,
        SyntaxKind::UnboundDeclNode,
    ]);
    /// The rule identifier for deprecated object warnings.
    pub const ID: &str = "DeprecatedObject";

    /// Creates a new "deprecated object" rule.
    pub fn new() -> Self {
        Self(Severity::Warning)
    }
}

impl Default for DeprecatedObjectRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DeprecatedObjectRule {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn description(&self) -> &'static str {
        "Ensures that the deprecated `Object` types are not used."
    }

    fn explanation(&self) -> &'static str {
        "WDL `Object` types are officially deprecated and will be removed in the next major WDL release.

`Object`s existed prior to better containers, such as `Map`s and `Struct`s, being \
introduced into the language. Unfortunately, though these better alternatives did exist at \
the time of the v1.0 release, the type was not removed. It was later decided \
that `Object`s overlapped with `Map`s and `Struct`s in functionality, and the type was marked for removal.

See this issue for more details: <https://github.com/openwdl/wdl/pull/228>."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

workflow example {
    Object person = object {
        name: "Jimmy",
        age: 55,
    }
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: Some("Consider switching to a `Struct` or `Map`"),
                snippet: r#"version 1.2

struct Person {
    String name
    Int age
}

workflow example {
    Person person = Person {
        name: "Jimmy",
        age: 55,
    }
}
"#,
            }),
        }]
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Self::EXCEPTABLE_NODES
    }

    fn deny(&mut self) {
        self.0 = Severity::Error;
    }

    fn severity(&self) -> Severity {
        self.0
    }
}

/// Detects the use of a deprecated placeholder option.
#[derive(Debug, Clone, Copy)]
pub struct DeprecatedPlaceholderRule(Severity);

impl DeprecatedPlaceholderRule {
    /// See [`Self::exceptable_nodes()`].
    pub const EXCEPTABLE_NODES: Option<&'static [SyntaxKind]> = Some(&[
        SyntaxKind::VersionStatementNode,
        SyntaxKind::TaskDefinitionNode,
        SyntaxKind::WorkflowDefinitionNode,
        SyntaxKind::PlaceholderNode,
    ]);
    /// The rule identifier for deprecated placeholder option warnings.
    pub const ID: &str = "DeprecatedPlaceholder";

    /// Creates a new "deprecated placeholder option" rule.
    pub fn new() -> Self {
        Self(Severity::Warning)
    }
}

impl Default for DeprecatedPlaceholderRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DeprecatedPlaceholderRule {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn description(&self) -> &'static str {
        "Ensures that deprecated expression placeholder options are not used."
    }

    fn explanation(&self) -> &'static str {
        "Expression placeholder options were deprecated in WDL v1.1 and will be removed in the \
         next major WDL version.

         - `sep` placeholder options should be replaced by the `sep()` standard library function.
         - `true/false` placeholder options should be replaced with `if`/`else` statements.
         - `default` placeholder options should be replaced by the `select_first()` standard \
         library function.
         - `${}` interpolation placeholders should be replaced by `~{}` interpolation placeholders.


This rule only evaluates for WDL V1 documents with a version of v1.1 or later, as this was the \
         version where the deprecation was introduced."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

workflow example {
    Array[String] names = [
        "James",
        "Jimmy",
        "John",
    ]
    String names_separated = "~{sep="," names}"
    String names_interpolated = "${names_separated}"
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

workflow example {
    Array[String] names = [
        "James",
        "Jimmy",
        "John",
    ]
    String names_separated = "~{sep(",", names)}"
    String names_interpolated = "~{names_separated}"
}
"#,
            }),
        }]
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Self::EXCEPTABLE_NODES
    }

    fn deny(&mut self) {
        self.0 = Severity::Error;
    }

    fn severity(&self) -> Severity {
        self.0
    }
}

/// Detects deprecated `runtime` sections.
#[derive(Debug, Clone, Copy)]
pub struct DeprecatedRuntimeSectionRule(Severity);

impl DeprecatedRuntimeSectionRule {
    /// See [`Self::exceptable_nodes()`].
    pub const EXCEPTABLE_NODES: Option<&'static [SyntaxKind]> = Some(&[
        SyntaxKind::VersionStatementNode,
        SyntaxKind::TaskDefinitionNode,
        SyntaxKind::RuntimeSectionNode,
    ]);
    /// The rule identifier for deprecated runtime section warnings.
    pub const ID: &str = "DeprecatedRuntimeSection";

    /// Creates a new "deprecated runtime section" rule.
    pub fn new() -> Self {
        Self(Severity::Warning)
    }
}

impl Default for DeprecatedRuntimeSectionRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DeprecatedRuntimeSectionRule {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn description(&self) -> &'static str {
        "Detects deprecated `runtime` sections."
    }

    fn explanation(&self) -> &'static str {
        "The `runtime` section is deprecated in WDL v1.2 and later. Replace it with a \
         `requirements` section."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>

    runtime {
        container: "ubuntu:latest"
    }
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>

    requirements {
        container: "ubuntu:latest"
    }
}
"#,
            }),
        }]
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Self::EXCEPTABLE_NODES
    }

    fn deny(&mut self) {
        self.0 = Severity::Error;
    }

    fn severity(&self) -> Severity {
        self.0
    }
}
