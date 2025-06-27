//! Ensures that lines do not exceed a certain width.

use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::Whitespace;
use wdl_ast::v1;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the line width rule.
const ID: &str = "LineWidth";

/// Creates a diagnostic for when a line exceeds the maximum width.
fn line_too_long(span: Span, max_width: usize) -> Diagnostic {
    Diagnostic::note(format!("line exceeds maximum width of {max_width}"))
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("split the line into multiple lines")
}

/// Detects lines that exceed a certain width.
#[derive(Clone, Copy, Debug)]
pub struct LineWidthRule {
    /// The maximum width of a line.
    max_width: usize,
    /// The offset of the last newline character seen (if it exists).
    previous_newline_offset: Option<usize>,
    /// Whether we are in a section that should be ignored.
    ignored_section: bool,
}

impl LineWidthRule {
    /// Constructs a new line width rule with the given maximum line width.
    pub fn new(max_width: usize) -> Self {
        Self {
            max_width,
            ..Default::default()
        }
    }

    /// Detects lines that exceed a certain width.
    fn detect_line_too_long(
        &mut self,
        diagnostics: &mut Diagnostics,
        text: &str,
        start: usize,
        element: SyntaxElement,
        exceptable_nodes: &Option<&'static [wdl_ast::SyntaxKind]>,
    ) {
        for offset in text
            .char_indices()
            .filter(|(_, c)| *c == '\n')
            .map(|(offset, _)| offset)
        {
            let current_offset = start + offset;
            let previous_offset = self.previous_newline_offset.unwrap_or_default();
            let length = current_offset - previous_offset;

            if !self.ignored_section && length > self.max_width {
                let span = Span::new(previous_offset, length);

                diagnostics.exceptable_add(
                    line_too_long(span, self.max_width),
                    element.clone(),
                    exceptable_nodes,
                );
            }

            self.previous_newline_offset = Some(current_offset + 1);
        }
    }
}

/// Implements the default line width rule.
impl Default for LineWidthRule {
    fn default() -> Self {
        Self {
            max_width: 90,
            previous_newline_offset: None,
            ignored_section: false,
        }
    }
}

impl Rule for LineWidthRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that lines do not exceed a certain width."
    }

    fn explanation(&self) -> &'static str {
        "Lines should not exceed a certain width to make it easier to read and understand the \
         code. Code within the either the meta or parameter meta sections is not checked. Comments \
         are included in the line width check. The current maximum width is 90 characters."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style, Tag::Clarity, Tag::Spacing])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        None
    }

    fn related_rules(&self) -> &[&'static str] {
        &["ExpressionSpacing"]
    }
}

impl Visitor for LineWidthRule {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn whitespace(&mut self, diagnostics: &mut Diagnostics, whitespace: &Whitespace) {
        self.detect_line_too_long(
            diagnostics,
            whitespace.text(),
            whitespace.span().start(),
            whitespace
                .inner()
                .prev_sibling_or_token()
                .unwrap_or(SyntaxElement::from(whitespace.inner().clone())),
            &self.exceptable_nodes(),
        );
    }

    fn command_text(&mut self, diagnostics: &mut Diagnostics, text: &v1::CommandText) {
        self.detect_line_too_long(
            diagnostics,
            text.text(),
            text.span().start(),
            SyntaxElement::from(text.inner().clone()),
            &self.exceptable_nodes(),
        );
    }

    fn metadata_section(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        _: &v1::MetadataSection,
    ) {
        self.ignored_section = matches!(reason, VisitReason::Enter);
    }

    fn parameter_metadata_section(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        _: &v1::ParameterMetadataSection,
    ) {
        self.ignored_section = matches!(reason, VisitReason::Enter);
    }
}
