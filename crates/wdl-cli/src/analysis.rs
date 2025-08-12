//! Facilities for performing a typical analysis using the `wdl-*` crates.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Error;
use futures::future::BoxFuture;
use nonempty::NonEmpty;
use tracing::warn;
use wdl_analysis::Analyzer;
use wdl_analysis::DiagnosticsConfig;
use wdl_analysis::ProgressKind;
use wdl_analysis::Validator;
use wdl_lint::Linter;

mod results;
mod source;

pub use results::AnalysisResults;
pub use source::Source;

/// The type of the initialization callback.
type InitCb = Box<dyn Fn() + 'static>;

/// The type of the progress callback.
type ProgressCb =
    Box<dyn Fn(ProgressKind, usize, usize) -> BoxFuture<'static, ()> + Send + Sync + 'static>;

/// An analysis.
pub struct Analysis {
    /// The set of root nodes to analyze.
    ///
    /// Can be files, directories, or URLs.
    sources: Vec<Source>,

    /// A list of rules to except.
    exceptions: HashSet<String>,

    /// Whether or not to enable linting.
    lint: bool,

    /// Basename for any ignorefiles which should be respected.
    ignore_filename: Option<String>,

    /// The initialization callback.
    init: InitCb,

    /// The progress callback.
    progress: ProgressCb,
}

impl Analysis {
    /// Adds a source to the analysis.
    pub fn add_source(mut self, source: Source) -> Self {
        self.sources.push(source);
        self
    }

    /// Adds multiple sources to the analysis.
    pub fn extend_sources(mut self, source: impl IntoIterator<Item = Source>) -> Self {
        self.sources.extend(source);
        self
    }

    /// Adds a rule to the excepted rules list.
    pub fn add_exception(mut self, rule: impl Into<String>) -> Self {
        self.exceptions.insert(rule.into());
        self
    }

    /// Adds multiple rules to the excepted rules list.
    pub fn extend_exceptions(mut self, rules: impl IntoIterator<Item = String>) -> Self {
        self.exceptions.extend(rules);
        self
    }

    /// Sets whether linting is enabled.
    pub fn lint(mut self, value: bool) -> Self {
        self.lint = value;
        self
    }

    /// Sets the ignorefile basename.
    pub fn ignore_filename(mut self, filename: Option<String>) -> Self {
        self.ignore_filename = filename;
        self
    }

    /// Sets the initialization callback.
    pub fn init<F>(mut self, init: F) -> Self
    where
        F: Fn() + 'static,
    {
        self.init = Box::new(init);
        self
    }

    /// Sets the progress callback.
    pub fn progress<F>(mut self, progress: F) -> Self
    where
        F: Fn(ProgressKind, usize, usize) -> BoxFuture<'static, ()> + Send + Sync + 'static,
    {
        self.progress = Box::new(progress);
        self
    }

    /// Runs the analysis and returns all results (if any exist).
    pub async fn run(self) -> std::result::Result<AnalysisResults, NonEmpty<Arc<Error>>> {
        warn_unknown_rules(&self.exceptions);
        let config = wdl_analysis::Config::default()
            .with_diagnostics_config(get_diagnostics_config(&self.exceptions))
            .with_ignore_filename(self.ignore_filename);

        (self.init)();

        let validator = Box::new(move || {
            let mut validator = Validator::default();

            if self.lint {
                let visitor = get_lint_visitor(&self.exceptions);
                validator.add_visitor(visitor);
            }

            validator
        });

        let mut analyzer = Analyzer::new_with_validator(
            config,
            move |_, kind, count, total| (self.progress)(kind, count, total),
            validator,
        );

        for source in self.sources {
            if let Err(error) = source.register(&mut analyzer).await {
                return Err(NonEmpty::new(Arc::new(error)));
            }
        }

        let results = analyzer
            .analyze(())
            .await
            .map_err(|error| NonEmpty::new(Arc::new(error)))?;

        AnalysisResults::try_new(results)
    }
}

impl Default for Analysis {
    fn default() -> Self {
        Self {
            sources: Default::default(),
            exceptions: Default::default(),
            lint: Default::default(),
            ignore_filename: None,
            init: Box::new(|| {}),
            progress: Box::new(|_, _, _| Box::pin(async {})),
        }
    }
}

/// Warns about any unknown rules.
fn warn_unknown_rules(exceptions: &HashSet<String>) {
    let mut names = wdl_analysis::rules()
        .iter()
        .map(|rule| rule.id().to_owned())
        .collect::<Vec<_>>();

    names.extend(wdl_lint::rules().iter().map(|rule| rule.id().to_owned()));

    let mut unknown = exceptions
        .iter()
        .filter(|rule| !names.iter().any(|name| name.eq_ignore_ascii_case(rule)))
        .map(|rule| format!("`{rule}`"))
        .collect::<Vec<_>>();

    if !unknown.is_empty() {
        unknown.sort();

        warn!(
            "ignoring unknown excepted rule{s}: {rules}",
            s = if unknown.len() == 1 { "" } else { "s" },
            rules = unknown.join(", ")
        );
    }
}

/// Gets the rules as a diagnositics configuration with the excepted rules
/// removed.
fn get_diagnostics_config(exceptions: &HashSet<String>) -> DiagnosticsConfig {
    DiagnosticsConfig::new(wdl_analysis::rules().into_iter().filter(|rule| {
        !exceptions
            .iter()
            .any(|exception| exception.eq_ignore_ascii_case(rule.id()))
    }))
}

/// Gets a lint visitor with the excepted rules removed.
fn get_lint_visitor(exceptions: &HashSet<String>) -> Linter {
    Linter::new(wdl_lint::rules().into_iter().filter(|rule| {
        !exceptions
            .iter()
            .any(|exception| exception.eq_ignore_ascii_case(rule.id()))
    }))
}
