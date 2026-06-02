use pyo3::prelude::*;
use pyo3::types::PyList;

/// Represents a span of source.
#[pyclass(module = "sprocket_bio.grammar", frozen, eq, ord, str = "{0}")]
#[derive(PartialEq, PartialOrd)]
pub struct Span(wdl::grammar::Span);

#[pymethods]
impl Span {
    /// Creates a new span from the given start and length.
    #[new]
    fn __new__(start: usize, len: usize) -> Self {
        Self(wdl::grammar::Span::new(start, len))
    }

    /// Gets the start of the span.
    #[getter]
    fn start(&self) -> usize {
        self.0.start()
    }

    /// Gets the noninclusive end of the span.
    #[getter]
    fn end(&self) -> usize {
        self.0.end()
    }

    /// Gets the length of the span.
    fn len(&self) -> usize {
        self.0.len()
    }

    /// Determines if the span is empty.
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Determines if the span contains the given offset.
    fn contains(&self, offset: usize) -> bool {
        self.0.contains(offset)
    }

    /// Calculates an intersection of two spans, if one exists.
    ///
    /// If spans are adjacent, a zero-length span is returned.
    ///
    /// Returns `None` if the two spans are disjoint.
    ///
    /// # Examples
    ///
    /// >>> Span.intersect(Span(0, 10), Span(5, 10))
    /// Span(5..10)
    fn intersect(&self, other: Bound<'_, Self>) -> Option<Self> {
        self.0.intersect(other.get().0).map(Self)
    }

    fn __repr__(&self) -> String {
        format!("Span({}..{})", self.0.start(), self.0.end())
    }
}

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
    fn with_highlight(&self, span: Bound<'_, Span>) -> Self {
        Self(self.0.clone().with_highlight(span.get().0))
    }

    /// Adds a label to the diagnostic.
    ///
    /// The first label added is considered the primary label.
    ///
    /// The span for the label is expected to be for the same file as the
    /// diagnostic.
    fn with_label(&self, message: &str, span: Bound<'_, Span>) -> Self {
        Self(self.0.clone().with_label(message, span.get().0))
    }

    /// Sets the severity of the diagnostic.
    fn with_severity(&self, severity: Bound<'_, PyAny>) -> Self {
        todo!("Self(self.0.clone().with_severity(severity))")
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
