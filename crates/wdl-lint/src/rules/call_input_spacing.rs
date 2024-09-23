//! A lint rule for spacing of call inputs.

use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::ToSpan;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;
use wdl_ast::v1::CallStatement;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the input not sorted rule.
const ID: &str = "CallInputSpacing";

/// Creates a input spacing diagnostic.
fn call_input_keyword_spacing(span: Span) -> Diagnostic {
    Diagnostic::note("call input keyword not properly spaced")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a single space prior to the input keyword")
}

/// Creates an incorrect call input whitespace diagnostic.
fn call_input_incorrect_spacing(span: Span) -> Diagnostic {
    Diagnostic::note("call input not properly spaced")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("change this whitespace to a single space")
}

/// Creates an input call spacing diagnostic.
fn call_input_missing_newline(span: Span) -> Diagnostic {
    Diagnostic::note("call inputs must be separated by newline")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add newline before the input")
}

/// Creates call input assignment diagnostic.
fn call_input_assignment(span: Span) -> Diagnostic {
    Diagnostic::note("call inputs assignments must be surrounded with whitespace")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("surround '=' with whitespace on each side")
}

/// Detects unsorted input declarations.
#[derive(Default, Debug, Clone, Copy)]
pub struct CallInputSpacingRule;

impl Rule for CallInputSpacingRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that call inputs are spaced appropriately."
    }

    fn explanation(&self) -> &'static str {
        "When making calls from a workflow, it is more readable and easier to edit if the supplied \
         inputs are each on their own line. When there is more than one input to a call statement, \
         the `input:` keyword should follow the opening brace ({) and a single space, then each \
         input specification should occupy its own line. This does inflate the line count of a WDL \
         document, but it is worth it for the consistent readability. An exception can be made \
         (but does not have to be made), for calls with only a single parameter. In those cases, \
         it is permissable to keep the input on the same line as the call."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style, Tag::Clarity, Tag::Spacing])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::CallStatementNode,
            SyntaxKind::WorkflowDefinitionNode,
        ])
    }
}

impl Visitor for CallInputSpacingRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
    }

    fn call_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        call: &CallStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let inputs = call.inputs().count();

        if inputs == 0 {
            return;
        }

        // Check for "{ input:" spacing
        if let Some(input_keyword) = call
            .syntax()
            .children_with_tokens()
            .find(|c| c.kind() == SyntaxKind::InputKeyword)
        {
            if let Some(whitespace) = input_keyword.prev_sibling_or_token() {
                if whitespace.kind() != SyntaxKind::Whitespace {
                    // If there is no whitespace before the input keyword
                    state.exceptable_add(
                        call_input_keyword_spacing(input_keyword.text_range().to_span()),
                        SyntaxElement::from(call.syntax().clone()),
                        &self.exceptable_nodes(),
                    );
                } else if !whitespace.as_token().unwrap().text().eq(" ") {
                    // If there is anything other than one space before the input keyword
                    state.exceptable_add(
                        call_input_incorrect_spacing(whitespace.text_range().to_span()),
                        SyntaxElement::from(call.syntax().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            }
        }

        call.inputs().for_each(|input| {
            // Check for assignment spacing
            if let Some(assign) = input
                .syntax()
                .children_with_tokens()
                .find(|c| c.kind() == SyntaxKind::Assignment)
            {
                match (
                    assign.next_sibling_or_token().unwrap().kind(),
                    assign.prev_sibling_or_token().unwrap().kind(),
                ) {
                    (SyntaxKind::Whitespace, SyntaxKind::Whitespace) => {}
                    _ => {
                        state.exceptable_add(
                            call_input_assignment(assign.text_range().to_span()),
                            SyntaxElement::from(call.syntax().clone()),
                            &self.exceptable_nodes(),
                        );
                    }
                }
            }
        });

        // Check for one input per line
        let mut newline_seen = 0;
        call.syntax()
            .children_with_tokens()
            .for_each(|c| match c.kind() {
                SyntaxKind::Whitespace => {
                    if c.to_string().contains('\n') {
                        newline_seen += 1;
                    }
                }
                SyntaxKind::CallInputItemNode => {
                    if newline_seen == 0 && inputs > 1 {
                        state.exceptable_add(
                            call_input_missing_newline(c.text_range().to_span()),
                            SyntaxElement::from(call.syntax().clone()),
                            &self.exceptable_nodes(),
                        );
                    }
                    newline_seen = 0;
                }
                _ => {}
            });
    }
}
