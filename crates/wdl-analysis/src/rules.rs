//! Implementation of analysis rule configuration.

use wdl_ast::Severity;

/// The rule identifier for unused import warnings.
pub const UNUSED_IMPORT_RULE_ID: &str = "UnusedImport";

/// The rule identifier for unused input warnings.
pub const UNUSED_INPUT_RULE_ID: &str = "UnusedInput";

/// The rule identifier for unused declaration warnings.
pub const UNUSED_DECL_RULE_ID: &str = "UnusedDeclaration";

/// The rule identifier for unused call warnings.
pub const UNUSED_CALL_RULE_ID: &str = "UnusedCall";

/// The rule identifier for unnecessary function call warnings.
pub const UNNECESSARY_FUNCTION_CALL: &str = "UnnecessaryFunctionCall";

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
        UNUSED_IMPORT_RULE_ID
    }

    fn description(&self) -> &'static str {
        "Ensures that import namespaces are used in the importing document."
    }

    fn explanation(&self) -> &'static str {
        "Imported WDL documents should be used in the document that imports them. Unused imports \
         impact parsing and evaluation performance."
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
        UNUSED_INPUT_RULE_ID
    }

    fn description(&self) -> &'static str {
        "Ensures that task or workspace inputs are used within the declaring task or workspace."
    }

    fn explanation(&self) -> &'static str {
        "Unused inputs degrade evaluation performance and reduce the clarity of the code. Unused \
         file inputs in tasks can also cause unnecessary file localizations."
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
        UNUSED_DECL_RULE_ID
    }

    fn description(&self) -> &'static str {
        "Ensures that private declarations in tasks or workspaces are used within the declaring \
         task or workspace."
    }

    fn explanation(&self) -> &'static str {
        "Unused private declarations degrade evaluation performance and reduce the clarity of the \
         code."
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
        UNUSED_CALL_RULE_ID
    }

    fn description(&self) -> &'static str {
        "Ensures that outputs of a call statement are used in the declaring workflow."
    }

    fn explanation(&self) -> &'static str {
        "Unused calls may cause unnecessary consumption of compute resources."
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
        UNNECESSARY_FUNCTION_CALL
    }

    fn description(&self) -> &'static str {
        "Ensures that function calls are necessary."
    }

    fn explanation(&self) -> &'static str {
        "Unnecessary function calls may impact evaluation performance."
    }

    fn deny(&mut self) {
        self.0 = Severity::Error;
    }

    fn severity(&self) -> Severity {
        self.0
    }
}
