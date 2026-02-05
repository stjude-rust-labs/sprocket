//! A lint rule for ensuring doc comments appear before lint directives.

use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxToken;
use wdl_ast::SyntaxTokenExt;
use wdl_ast::v1::EnumDefinition;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the rule.
const ID: &str = "DocCommentFormatted";

/// Creates a diagnostic for doc comments appearing after lint directives.
fn doc_comment_after_directive(span: Span) -> Diagnostic {
    Diagnostic::note("doc comments should come before lint directives")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("move the doc comment before any lint directives")
}

/// A lint rule for ensuring doc comments appear before lint directives.
#[derive(Default, Debug, Clone, Copy)]
pub struct DocCommentFormattedRule;

impl Rule for DocCommentFormattedRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that doc comments appear before lint directives."
    }

    fn explanation(&self) -> &'static str {
        "Doc comments (`##`) should be placed before lint directives (`#@`). This ensures \
         consistent formatting and makes it clear that doc comments document the element they \
         precede, not the directives. This rule applies to constructs that currently support doc \
         comments."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Clarity, Tag::Style])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        None
    }

    fn related_rules(&self) -> &[&'static str] {
        &["PreambleCommentPlacement", "LintDirectiveFormatted"]
    }
}

impl DocCommentFormattedRule {
    /// Checks the preceding trivia of a keyword token for doc comments after
    /// directives.
    fn check_trivia(&self, diagnostics: &mut Diagnostics, keyword: &SyntaxToken) {
        let mut seen_directive = false;
        let mut doc_block_start: Option<Span> = None;
        let mut doc_block_element: Option<SyntaxElement> = None;

        for token in keyword.preceding_trivia() {
            match token.kind() {
                SyntaxKind::Comment => {
                    let text = token.text();
                    
                    // Check if it's a directive (any comment starting with #@)
                    if text.starts_with("#@") {
                        seen_directive = true;
                        // If we had a doc block in progress, it ended
                        doc_block_start = None;
                        doc_block_element = None;
                    }
                    // Check if it's a doc comment
                    else if (text == "##" || text.starts_with("## ")) && seen_directive {
                        // We found a doc comment after a directive
                        if doc_block_start.is_none() {
                            // Start of a new doc block
                            doc_block_start = Some(token.text_range().into());
                            doc_block_element = Some(SyntaxElement::Token(token.clone()));
                        } else {
                            // Extend the doc block span
                            if let Some(start_span) = doc_block_start {
                                let end = usize::from(token.text_range().end());
                                doc_block_start = Some(Span::new(
                                    start_span.start(),
                                    end - start_span.start(),
                                ));
                            }
                        }
                    }
                    // Non-doc, non-directive comment breaks the scan
                    else if !text.starts_with("##") {
                        break;
                    }
                }
                SyntaxKind::Whitespace => {
                    // Check if this is a blank line (more than one newline)
                    if token.text().chars().filter(|c| *c == '\n').count() > 1 {
                        // Blank line ends the doc block, but we continue scanning
                        // in case there are more directives and doc comments
                        if let (Some(span), Some(element)) = (doc_block_start, &doc_block_element) {
                            diagnostics.exceptable_add(
                                doc_comment_after_directive(span),
                                element.clone(),
                                &self.exceptable_nodes(),
                            );
                        }
                        doc_block_start = None;
                        doc_block_element = None;
                    }
                }
                _ => break,
            }
        }

        // Emit diagnostic for any remaining doc block
        if let (Some(span), Some(element)) = (doc_block_start, doc_block_element) {
            diagnostics.exceptable_add(
                doc_comment_after_directive(span),
                element,
                &self.exceptable_nodes(),
            );
        }
    }
}

// NOTE:
// This rule mirrors the current set of syntax elements that support doc comments.
// Some constructs (e.g. enums) may parse successfully but are not yet visited here.
// This is intentional and will be revisited as doc comment support is expanded.
// See #592 and #591.
impl Visitor for DocCommentFormattedRule {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn struct_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        def: &StructDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.check_trivia(diagnostics, def.keyword().inner());
    }

    fn enum_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        def: &EnumDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.check_trivia(diagnostics, def.keyword().inner());
    }

    fn task_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Tasks don't have a keyword() method, use first_token()
        if let Some(first_token) = task.inner().first_token() {
            self.check_trivia(diagnostics, &first_token);
        }
    }

    fn workflow_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        workflow: &WorkflowDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Workflows don't have a keyword() method, use first_token()
        if let Some(first_token) = workflow.inner().first_token() {
            self.check_trivia(diagnostics, &first_token);
        }
    }
}
