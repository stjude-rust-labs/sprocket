use pyo3::prelude::*;
use pyo3::types::PyList;

/// Represents a diagnostic to display to the user.
#[pyclass(module = "sprocket_bio.grammar", frozen, eq, ord)]
#[derive(PartialEq, PartialOrd)]
pub struct Diagnostic(pub(crate) wdl::grammar::Diagnostic);

#[pymethods]
impl Diagnostic {
    /// Creates a new diagnostic error with the given message.
    #[staticmethod]
    fn error(message: &str) -> Self {
        Self(wdl::grammar::Diagnostic::error(message))
    }

    /// Creates a new diagnostic warning with the given message.
    #[staticmethod]
    fn warning(message: &str) -> Self {
        Self(wdl::grammar::Diagnostic::warning(message))
    }

    /// Creates a new diagnostic node with the given message.
    #[staticmethod]
    fn note(message: &str) -> Self {
        Self(wdl::grammar::Diagnostic::note(message))
    }

    /// Sets the rule for the diagnostic.
    fn with_rule(&self, rule: &str) -> Self {
        Self(self.0.clone().with_rule(rule))
    }

    /// Sets the help message for the diagnostic.
    ///
    /// This is different from the `fix` message, as it only serves to provide
    /// more context to the issue, rather than a solution.
    fn with_help(&self, help: &str) -> Self {
        Self(self.0.clone().with_help(help))
    }

    /// Sets the fix message for the diagnostic.
    fn with_fix(&self, fix: &str) -> Self {
        Self(self.0.clone().with_fix(fix))
    }

    /// Adds a highlight to the diagnostic.
    ///
    /// This is equivalent to adding a label with an empty message.
    ///
    /// The span for the highlight is expected to be for the same file as the
    /// diagnostic.
    fn with_highlight(&self, span: Bound<'_, PyAny>) -> Self {
        Self(self.0.clone().with_highlight(span)) // TODO
    }

    /// Adds a label to the diagnostic.
    ///
    /// The first label added is considered the primary label.
    ///
    /// The span for the label is expected to be for the same file as the
    /// diagnostic.
    fn with_label(&self, message: &str, span: Bound<'_, PyAny>) -> Self {
        Self(self.0.clone().with_label(message, span)) // TODO
    }

    /// Sets the severity of the diagnostic.
    fn with_severity(&self, severity: Bound<'_, PyAny>) -> Self {
        Self(self.0.clone().with_severity(severity)) // TODO
    }

    /// Gets the optional rule associated with the diagnostic.
    fn rule(&self) -> Option<&str> {
        self.0.rule()
    }

    /// Gets the default severity level of the diagnostic.
    ///
    /// The severity level may be upgraded to error depending on configuration.
    fn severity(&self) -> Bound<'_, PyAny> {
        todo!()
    }

    /// Gets the message of the diagnostic.
    fn message(&self) -> &str {
        self.0.message()
    }

    /// Gets the optional fix of the diagnostic.
    fn fix(&self) -> Option<&str> {
        self.0.fix()
    }

    /// Gets the labels of the diagnostic.
    fn labels(&self) -> Bound<'_, PyList> {
        todo!()
    }
}
