//! Implementation of analysis rules.

use std::sync::LazyLock;

use serde::Serialize;
use wdl_ast::Severity;
use wdl_grammar::SyntaxKind;

/// All rule IDs sorted alphabetically.
pub static ALL_RULE_IDS: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut ids: Vec<String> = rules().iter().map(|r| r.id().to_string()).collect();
    ids.sort();
    ids
});

/// A labeled WDL code snippet.
#[derive(Copy, Clone, Debug, Serialize)]
pub struct LabeledSnippet {
    /// A label for the snippet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<&'static str>,
    /// A WDL code snippet.
    pub snippet: &'static str,
}

/// A lint rule example.
#[derive(Copy, Clone, Debug, Serialize)]
pub struct Example {
    /// A snippet that will trigger the target lint rule.
    pub negative: LabeledSnippet,
    /// A revision of the negative snippet that will no longer trigger the rule.
    #[serde(skip_serializing_if = "Option::is_none")]
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
                snippet: r#"version 1.2

import "foo.wdl"

workflow example {
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: Some("Consider removing the import entirely"),
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

/// Represents the using fallback version rule.
#[derive(Debug, Clone, Copy)]
pub struct MeaninglessLintDirective(Severity);

impl MeaninglessLintDirective {
    /// See [`Self::exceptable_nodes()`].
    pub const EXCEPTABLE_NODES: Option<&'static [SyntaxKind]> =
        Some(&[SyntaxKind::VersionStatementNode]);
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
        Some(&[SyntaxKind::VersionStatementNode])
    }

    fn deny(&mut self) {
        self.0 = Severity::Error;
    }

    fn severity(&self) -> Severity {
        self.0
    }
}
