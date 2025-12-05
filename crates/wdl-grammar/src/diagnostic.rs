//! Definition of diagnostics displayed to users.

use std::cmp::Ordering;
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
    /// Creates a new span from the given start and length.
    pub const fn new(start: usize, len: usize) -> Self {
        Self {
            start,
            end: start + len,
        }
    }

    /// Gets the start of the span.
    pub fn start(&self) -> usize {
        self.start
    }

    /// Gets the noninclusive end of the span.
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

    /// Determines if the span contains the given offset.
    pub fn contains(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }

    /// Calculates an intersection of two spans, if one exists.
    ///
    /// If spans are adjacent, a zero-length span is returned.
    ///
    /// Returns `None` if the two spans are disjoint.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use wdl_grammar::Span;
    /// assert_eq!(
    ///     Span::intersect(Span::new(0, 10), Span::new(5, 10)),
    ///     Some(Span::new(5, 5)),
    /// );
    /// ```
    #[inline]
    pub fn intersect(self, other: Self) -> Option<Self> {
        let start = self.start.max(other.start);
        let end = self.end.min(other.end);
        if end < start {
            return None;
        }

        Some(Self { start, end })
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

impl From<TextRange> for Span {
    fn from(value: TextRange) -> Self {
        let start = usize::from(value.start());
        Self::new(start, usize::from(value.end()) - start)
    }
}

/// A helper struct to associate a value with it's source span.
#[derive(Debug)]
pub struct Spanned<T> {
    /// The value
    pub value: T,
    /// The span from the source document.
    pub span: Span,
}

/// Represents the severity of a diagnostic.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum Severity {
    /// The diagnostic is displayed as an error.
    Error,
    /// The diagnostic is displayed as a warning.
    Warning,
    /// The diagnostic is displayed as a note.
    Note,
}

impl Severity {
    /// Returns `true` if the severity is [`Error`].
    ///
    /// [`Error`]: Severity::Error
    #[must_use]
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error)
    }

    /// Returns `true` if the severity is [`Warning`].
    ///
    /// [`Warning`]: Severity::Warning
    #[must_use]
    pub fn is_warning(&self) -> bool {
        matches!(self, Self::Warning)
    }

    /// Returns `true` if the severity is [`Note`].
    ///
    /// [`Note`]: Severity::Note
    #[must_use]
    pub fn is_note(&self) -> bool {
        matches!(self, Self::Note)
    }
}

/// Represents a diagnostic to display to the user.
#[derive(Debug, Clone, Eq, PartialEq)]
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

impl Ord for Diagnostic {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.labels.cmp(&other.labels) {
            Ordering::Equal => {}
            ord => return ord,
        }

        match self.rule.cmp(&other.rule) {
            Ordering::Equal => {}
            ord => return ord,
        }

        match self.severity.cmp(&other.severity) {
            Ordering::Equal => {}
            ord => return ord,
        }

        match self.message.cmp(&other.message) {
            Ordering::Equal => {}
            ord => return ord,
        }

        self.fix.cmp(&other.fix)
    }
}

impl PartialOrd for Diagnostic {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
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
    ///
    /// The span for the highlight is expected to be for the same file as the
    /// diagnostic.
    pub fn with_highlight(mut self, span: impl Into<Span>) -> Self {
        self.labels.push(Label::new(String::new(), span.into()));
        self
    }

    /// Adds a label to the diagnostic.
    ///
    /// The first label added is considered the primary label.
    ///
    /// The span for the label is expected to be for the same file as the
    /// diagnostic.
    pub fn with_label(mut self, message: impl Into<String>, span: impl Into<Span>) -> Self {
        self.labels.push(Label::new(message, span.into()));
        self
    }

    /// Sets the severity of the diagnostic.
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
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

    /// Gets the mutable labels of the diagnostic.
    pub fn labels_mut(&mut self) -> impl Iterator<Item = &mut Label> {
        self.labels.iter_mut()
    }

    /// Converts this diagnostic to a `codespan` [Diagnostic].
    ///
    /// The provided file identifier is used for the diagnostic.
    ///
    /// [Diagnostic]: codespan_reporting::diagnostic::Diagnostic
    pub fn to_codespan<FileId: Copy>(
        &self,
        file_id: FileId,
    ) -> codespan_reporting::diagnostic::Diagnostic<FileId> {
        use codespan_reporting::diagnostic as codespan;

        let mut diagnostic: codespan::Diagnostic<FileId> = match self.severity {
            Severity::Error => codespan::Diagnostic::error(),
            Severity::Warning => codespan::Diagnostic::warning(),
            Severity::Note => codespan::Diagnostic::note(),
        };

        if let Some(rule) = &self.rule {
            diagnostic.code = Some(rule.clone());
        }

        diagnostic.message.clone_from(&self.message);

        if let Some(fix) = &self.fix {
            diagnostic.notes.push(format!("fix: {fix}"));
        }

        if self.labels.is_empty() {
            // Codespan will treat this as a label at the end of the file
            // We add this so that every diagnostic has at least one label with the file
            // printed.
            diagnostic.labels.push(codespan::Label::new(
                codespan::LabelStyle::Primary,
                file_id,
                usize::MAX - 1..usize::MAX,
            ))
        } else {
            for (i, label) in self.labels.iter().enumerate() {
                diagnostic.labels.push(
                    codespan::Label::new(
                        if i == 0 {
                            codespan::LabelStyle::Primary
                        } else {
                            codespan::LabelStyle::Secondary
                        },
                        file_id,
                        label.span.start..label.span.end,
                    )
                    .with_message(&label.message),
                );
            }
        }

        diagnostic
    }
}

/// Represents a label that annotates the source code.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Label {
    /// The optional message of the label (may be empty).
    message: String,
    /// The span of the label.
    span: Span,
}

impl Ord for Label {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.span.cmp(&other.span) {
            Ordering::Equal => {}
            ord => return ord,
        }

        self.message.cmp(&other.message)
    }
}

impl PartialOrd for Label {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Label {
    /// Creates a new label with the given message and span.
    pub fn new(message: impl Into<String>, span: impl Into<Span>) -> Self {
        Self {
            message: message.into(),
            span: span.into(),
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

    /// Sets the span of the label.
    pub fn set_span(&mut self, span: impl Into<Span>) {
        self.span = span.into();
    }
}
