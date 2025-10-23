//! Configuration for this crate.

use std::sync::Arc;

use tracing::warn;
use wdl_ast::Severity;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxNode;

use crate::Rule;
use crate::SyntaxNodeExt as _;
use crate::UNNECESSARY_FUNCTION_CALL;
use crate::UNUSED_CALL_RULE_ID;
use crate::UNUSED_DECL_RULE_ID;
use crate::UNUSED_IMPORT_RULE_ID;
use crate::UNUSED_INPUT_RULE_ID;
use crate::USING_FALLBACK_VERSION;
use crate::rules;

/// Configuration for `wdl-analysis`.
///
/// This type is a wrapper around an `Arc`, and so can be cheaply cloned and
/// sent between threads.
#[derive(Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Config {
    /// The actual fields, `Arc`ed up for easy cloning.
    #[serde(flatten)]
    inner: Arc<ConfigInner>,
}

// Custom `Debug` impl for the `Config` wrapper type that simplifies away the
// arc and the private inner struct
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("diagnostics", &self.inner.diagnostics)
            .field("fallback_version", &self.inner.fallback_version)
            .finish()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            inner: Arc::new(ConfigInner {
                diagnostics: Default::default(),
                fallback_version: None,
                ignore_filename: None,
                all_rules: Default::default(),
                feature_flags: FeatureFlags::default(),
            }),
        }
    }
}

impl Config {
    /// Get this configuration's [`DiagnosticsConfig`].
    pub fn diagnostics_config(&self) -> &DiagnosticsConfig {
        &self.inner.diagnostics
    }

    /// Get this configuration's fallback version; see
    /// [`Config::with_fallback_version()`].
    pub fn fallback_version(&self) -> Option<SupportedVersion> {
        self.inner.fallback_version
    }

    /// Get this configuration's ignore filename.
    pub fn ignore_filename(&self) -> Option<&str> {
        self.inner.ignore_filename.as_deref()
    }

    /// Gets the list of all known rule identifiers.
    pub fn all_rules(&self) -> &[String] {
        &self.inner.all_rules
    }

    /// Gets the feature flags.
    pub fn feature_flags(&self) -> &FeatureFlags {
        &self.inner.feature_flags
    }

    /// Return a new configuration with the previous [`DiagnosticsConfig`]
    /// replaced by the argument.
    pub fn with_diagnostics_config(&self, diagnostics: DiagnosticsConfig) -> Self {
        let mut inner = (*self.inner).clone();
        inner.diagnostics = diagnostics;
        Self {
            inner: Arc::new(inner),
        }
    }

    /// Return a new configuration with the previous version fallback option
    /// replaced by the argument.
    ///
    /// This option controls what happens when analyzing a WDL document with a
    /// syntactically valid but unrecognized version in the version
    /// statement. The default value is `None`, with no fallback behavior.
    ///
    /// Configured with `Some(fallback_version)`, analysis will proceed as
    /// normal if the version statement contains a recognized version. If
    /// the version is unrecognized, analysis will continue as if the
    /// version statement contained `fallback_version`, though the concrete
    /// syntax of the version statement will remain unchanged.
    ///
    /// <div class="warning">
    ///
    /// # Warnings
    ///
    /// This option is intended only for situations where unexpected behavior
    /// due to unsupported syntax is acceptable, such as when providing
    /// best-effort editor hints via `wdl-lsp`. The semantics of executing a
    /// WDL workflow with an unrecognized version is undefined and not
    /// recommended.
    ///
    /// Once this option has been configured for an `Analyzer`, it should not be
    /// changed. A document that was initially parsed and analyzed with one
    /// fallback option may cause errors if subsequent operations are
    /// performed with a different fallback option.
    ///
    /// </div>
    pub fn with_fallback_version(&self, fallback_version: Option<SupportedVersion>) -> Self {
        let mut inner = (*self.inner).clone();
        inner.fallback_version = fallback_version;
        Self {
            inner: Arc::new(inner),
        }
    }

    /// Return a new configuration with the previous ignore filename replaced by
    /// the argument.
    ///
    /// Specifying `None` for `filename` disables ignore behavior. This is also
    /// the default.
    ///
    /// `Some(filename)` will use `filename` as the ignorefile basename to
    /// search for. Child directories _and_ parent directories are searched
    /// for a file with the same basename as `filename` and if a match is
    /// found it will attempt to be parsed as an ignorefile with a syntax
    /// similar to `.gitignore` files.
    pub fn with_ignore_filename(&self, filename: Option<String>) -> Self {
        let mut inner = (*self.inner).clone();
        inner.ignore_filename = filename;
        Self {
            inner: Arc::new(inner),
        }
    }

    /// Returns a new configuration with the list of all known rule identifiers
    /// replaced by the argument.
    ///
    /// This is used internally to populate the `#@ except:` snippet.
    pub fn with_all_rules(&self, rules: Vec<String>) -> Self {
        let mut inner = (*self.inner).clone();
        inner.all_rules = rules;
        Self {
            inner: Arc::new(inner),
        }
    }

    /// Return a new configuration with the previous [`FeatureFlags`]
    /// replaced by the argument.
    pub fn with_feature_flags(&self, feature_flags: FeatureFlags) -> Self {
        let mut inner = (*self.inner).clone();
        inner.feature_flags = feature_flags;
        Self {
            inner: Arc::new(inner),
        }
    }
}

/// The actual configuration fields inside the [`Config`] wrapper.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
struct ConfigInner {
    /// See [`DiagnosticsConfig`].
    #[serde(default)]
    diagnostics: DiagnosticsConfig,
    /// See [`Config::with_fallback_version()`]
    #[serde(default)]
    fallback_version: Option<SupportedVersion>,
    /// See [`Config::with_ignore_filename()`]
    ignore_filename: Option<String>,
    /// A list of all known rule identifiers.
    #[serde(default)]
    all_rules: Vec<String>,
    /// The set of feature flags that can be enabled or disabled.
    #[serde(default)]
    feature_flags: FeatureFlags,
}

/// A set of feature flags that can be enabled.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct FeatureFlags {
    /// When available, enables experimental WDL 1.3 features.
    #[serde(default)]
    wdl_1_3: bool,
}

impl FeatureFlags {
    /// Creates a new [`FeatureFlags`] with all features enabled.
    ///
    /// This is useful when running tests downstream.
    pub fn all() -> Self {
        Self { wdl_1_3: true }
    }

    /// Gets whether experimental WDL 1.3 features are enabled.
    pub fn wdl_1_3(&self) -> bool {
        self.wdl_1_3
    }

    /// Returns a new `FeatureFlags` with experimental WDL 1.3 features enabled.
    pub fn with_wdl_1_3(mut self) -> Self {
        self.wdl_1_3 = true;
        self
    }
}

/// Configuration for analysis diagnostics.
///
/// Only the analysis diagnostics that aren't inherently treated as errors are
/// represented here.
///
/// These diagnostics default to a warning severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct DiagnosticsConfig {
    /// The severity for the unused import diagnostic.
    ///
    /// A value of `None` disables the diagnostic.
    pub unused_import: Option<Severity>,
    /// The severity for the unused input diagnostic.
    ///
    /// A value of `None` disables the diagnostic.
    pub unused_input: Option<Severity>,
    /// The severity for the unused declaration diagnostic.
    ///
    /// A value of `None` disables the diagnostic.
    pub unused_declaration: Option<Severity>,
    /// The severity for the unused call diagnostic.
    ///
    /// A value of `None` disables the diagnostic.
    pub unused_call: Option<Severity>,
    /// The severity for the unnecessary function call diagnostic.
    ///
    /// A value of `None` disables the diagnostic.
    pub unnecessary_function_call: Option<Severity>,
    /// The severity for the using fallback version diagnostic.
    ///
    /// A value of `None` disables the diagnostic. If there is no version
    /// configured with [`Config::with_fallback_version()`], this diagnostic
    /// will not be emitted.
    pub using_fallback_version: Option<Severity>,
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        Self::new(rules())
    }
}

impl DiagnosticsConfig {
    /// Creates a new diagnostics configuration from a rule set.
    pub fn new<T: AsRef<dyn Rule>>(rules: impl IntoIterator<Item = T>) -> Self {
        let mut unused_import = None;
        let mut unused_input = None;
        let mut unused_declaration = None;
        let mut unused_call = None;
        let mut unnecessary_function_call = None;
        let mut using_fallback_version = None;

        for rule in rules {
            let rule = rule.as_ref();
            match rule.id() {
                UNUSED_IMPORT_RULE_ID => unused_import = Some(rule.severity()),
                UNUSED_INPUT_RULE_ID => unused_input = Some(rule.severity()),
                UNUSED_DECL_RULE_ID => unused_declaration = Some(rule.severity()),
                UNUSED_CALL_RULE_ID => unused_call = Some(rule.severity()),
                UNNECESSARY_FUNCTION_CALL => unnecessary_function_call = Some(rule.severity()),
                USING_FALLBACK_VERSION => using_fallback_version = Some(rule.severity()),
                unrecognized => {
                    warn!(unrecognized, "unrecognized rule");
                    if cfg!(test) {
                        panic!("unrecognized rule: {unrecognized}");
                    }
                }
            }
        }

        Self {
            unused_import,
            unused_input,
            unused_declaration,
            unused_call,
            unnecessary_function_call,
            using_fallback_version,
        }
    }

    /// Returns a modified set of diagnostics that accounts for any `#@ except`
    /// comments that precede the given syntax node.
    pub fn excepted_for_node(mut self, node: &SyntaxNode) -> Self {
        let exceptions = node.rule_exceptions();

        if exceptions.contains(UNUSED_IMPORT_RULE_ID) {
            self.unused_import = None;
        }

        if exceptions.contains(UNUSED_INPUT_RULE_ID) {
            self.unused_input = None;
        }

        if exceptions.contains(UNUSED_DECL_RULE_ID) {
            self.unused_declaration = None;
        }

        if exceptions.contains(UNUSED_CALL_RULE_ID) {
            self.unused_call = None;
        }

        if exceptions.contains(UNNECESSARY_FUNCTION_CALL) {
            self.unnecessary_function_call = None;
        }

        if exceptions.contains(USING_FALLBACK_VERSION) {
            self.using_fallback_version = None;
        }

        self
    }

    /// Excepts all of the diagnostics.
    pub fn except_all() -> Self {
        Self {
            unused_import: None,
            unused_input: None,
            unused_declaration: None,
            unused_call: None,
            unnecessary_function_call: None,
            using_fallback_version: None,
        }
    }
}
