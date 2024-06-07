//! Definition of diagnostics displayed to users.

use std::fmt;

use rowan::TextRange;

/// Represents a span of source.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    /// The start of the span.
    start: usize,
    /// The end of the span.
    end: usize,
}

impl Span {
    /// Creates a new span from the given start and end.
    pub fn new(start: usize, len: usize) -> Self {
        Self {
            start,
            end: start + len,
        }
    }

    /// Gets the start of the span.
    pub fn start(&self) -> usize {
        self.start
    }

    /// Gets the end of the span.
    pub fn end(&self) -> usize {
        self.end
    }

    /// Gets the length of the span.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Determines if the span is empty.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{start}..{end}", start = self.start, end = self.end)
    }
}

impl From<logos::Span> for Span {
    fn from(value: logos::Span) -> Self {
        Self::new(value.start, value.len())
    }
}

/// Represents the severity of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// The diagnostic is displayed as an error.
    Error,
    /// The diagnostic is displayed as a warning.
    Warning,
    /// The diagnostic is displayed as a note.
    Note,
}

/// Represents a diagnostic to display to the user.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The optional rule associated with the diagnostic.
    rule: Option<String>,
    /// The default severity of the diagnostic.
    severity: Severity,
    /// The diagnostic message.
    message: String,
    /// The optional fix suggestion for the diagnostic.
    fix: Option<String>,
    /// The labels for the diagnostic.
    ///
    /// The first label in the collection is considered the primary label.
    labels: Vec<Label>,
}

impl Diagnostic {
    /// Creates a new diagnostic error with the given message.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            rule: None,
            severity: Severity::Error,
            message: message.into(),
            fix: None,
            labels: Default::default(),
        }
    }

    /// Creates a new diagnostic warning with the given message.
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            rule: None,
            severity: Severity::Warning,
            message: message.into(),
            fix: None,
            labels: Default::default(),
        }
    }

    /// Creates a new diagnostic node with the given message.
    pub fn note(message: impl Into<String>) -> Self {
        Self {
            rule: None,
            severity: Severity::Note,
            message: message.into(),
            fix: None,
            labels: Default::default(),
        }
    }

    /// Sets the rule for the diagnostic.
    pub fn with_rule(mut self, rule: impl Into<String>) -> Self {
        self.rule = Some(rule.into());
        self
    }

    /// Sets the fix message for the diagnostic.
    pub fn with_fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }

    /// Adds a highlight to the diagnostic.
    ///
    /// This is equivalent to adding a label with an empty message.
    pub fn with_highlight(mut self, span: impl ToSpan) -> Self {
        self.labels.push(Label::new(String::new(), span));
        self
    }

    /// Adds a label to the diagnostic.
    ///
    /// The first label added is considered the primary label.
    pub fn with_label(mut self, message: impl Into<String>, span: impl ToSpan) -> Self {
        self.labels.push(Label::new(message, span));
        self
    }

    /// Gets the optional rule associated with the diagnostic.
    pub fn rule(&self) -> Option<&str> {
        self.rule.as_deref()
    }

    /// Gets the default severity level of the diagnostic.
    ///
    /// The severity level may be upgraded to error depending on configuration.
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// Gets the message of the diagnostic.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Gets the optional fix of the diagnostic.
    pub fn fix(&self) -> Option<&str> {
        self.fix.as_deref()
    }

    /// Gets the labels of the diagnostic.
    pub fn labels(&self) -> impl Iterator<Item = &Label> {
        self.labels.iter()
    }

    /// Converts this diagnostic to a `codespan` [Diagnostic].
    ///
    /// [Diagnostic]: codespan_reporting::diagnostic::Diagnostic
    #[cfg(feature = "codespan")]
    pub fn to_codespan(&self) -> codespan_reporting::diagnostic::Diagnostic<()> {
        use codespan_reporting::diagnostic as codespan;

        let mut diagnostic = match self.severity {
            Severity::Error => codespan::Diagnostic::error(),
            Severity::Warning => codespan::Diagnostic::warning(),
            Severity::Note => codespan::Diagnostic::note(),
        };

        if let Some(rule) = &self.rule {
            diagnostic.message = format!("{msg} [rule: {rule}]", msg = self.message);
        } else {
            diagnostic.message.clone_from(&self.message);
        }

        if let Some(fix) = &self.fix {
            diagnostic.notes.push(format!("fix: {fix}"));
        }

        for (i, secondary) in self.labels.iter().enumerate() {
            diagnostic.labels.push(
                codespan::Label::new(
                    if i == 0 {
                        codespan::LabelStyle::Primary
                    } else {
                        codespan::LabelStyle::Secondary
                    },
                    (),
                    secondary.span.start..secondary.span.end,
                )
                .with_message(&secondary.message),
            );
        }

        diagnostic
    }
}

/// Represents a label that annotates the source code.
#[derive(Debug, Clone)]
pub struct Label {
    /// The optional message of the label (may be empty).
    message: String,
    /// The span of the label.
    span: Span,
}

impl Label {
    /// Creates a new label with the given message and span.
    pub fn new(message: impl Into<String>, span: impl ToSpan) -> Self {
        Self {
            message: message.into(),
            span: span.to_span(),
        }
    }

    /// Gets the message of the label.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Gets the span of the label.
    pub fn span(&self) -> Span {
        self.span
    }
}

/// A trait implemented on types that convert to spans.
pub trait ToSpan {
    /// Converts the type to a span.
    fn to_span(&self) -> Span;
}

impl ToSpan for TextRange {
    fn to_span(&self) -> Span {
        let start = usize::from(self.start());
        Span::new(start, usize::from(self.end()) - start)
    }
}

impl ToSpan for Span {
    fn to_span(&self) -> Span {
        *self
    }
}
