//! A lint rule for newlines at the end of the document.

use wdl_ast::Ast;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::util::strip_newline;

/// The identifier for the ending newline rule.
const ID: &str = "EndingNewline";

/// Creates a "missing ending newline" diagnostic.
fn missing_ending_newline(span: Span) -> Diagnostic {
    Diagnostic::note("missing newline at the end of the file")
        .with_rule(ID)
        .with_label("expected a newline to follow this", span)
        .with_fix("add a newline at the end of the file")
}

/// Creates a "multiple ending newline" diagnostic.
fn multiple_ending_newline(span: Span, count: usize) -> Diagnostic {
    Diagnostic::note("multiple empty lines at the end of file")
        .with_rule(ID)
        .with_label(
            if count > 1 {
                "duplicate newlines here"
            } else {
                "duplicate newline here"
            },
            span,
        )
        .with_fix("remove all but one empty line at the end of the file")
}

/// Detects missing newline at the end of the document.
#[derive(Default, Debug, Clone, Copy)]
pub struct EndingNewlineRule;

impl Rule for EndingNewlineRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that documents end with a single newline character."
    }

    fn explanation(&self) -> &'static str {
        "The file should end with one and only one newline character to conform to POSIX standards. See https://pubs.opengroup.org/onlinepubs/9699919799/basedefs/V1_chap03.html#tag_03_206."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Spacing, Tag::Style])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[SyntaxKind::VersionStatementNode])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for EndingNewlineRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        doc: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Enter {
            // We only process on exit so that it's one of the last diagnostics emitted
            // Reset the visitor upon document entry
            *self = Default::default();
            return;
        }

        // Don't run on a document without a supported version
        if matches!(doc.ast(), Ast::Unsupported) {
            return;
        }

        // Get the last token in the document and see if it's whitespace
        match doc.inner().last_child_or_token() {
            Some(last) if last.kind() == SyntaxKind::Whitespace => {
                // It's whitespace, check if it ends with a newline
                let last = last.into_token().expect("whitespace should be a token");
                let start = usize::from(last.text_range().start());
                let text = last.text();
                let len = text.len();
                match strip_newline(last.text()) {
                    Some(mut text) => {
                        // Count the number of extra newlines
                        let mut extra = 0;
                        while let Some(stripped) = strip_newline(text) {
                            extra += 1;
                            text = stripped;
                        }

                        if extra > 0 {
                            // Since this rule can only be excepted in a document-wide fashion,
                            // if the rule is running we can directly add the diagnostic
                            // without checking for the exceptable nodes
                            state.add(multiple_ending_newline(
                                Span::new(start + text.len(), len - text.len() - 1),
                                extra,
                            ));
                        }
                    }
                    // Since this rule can only be excepted in a document-wide fashion,
                    // if the rule is running we can directly add the diagnostic
                    // without checking for the exceptable nodes
                    None => state.add(missing_ending_newline(Span::new(start + (len - 1), 1))),
                }
            }
            Some(last) => {
                // Since this rule can only be excepted in a document-wide fashion,
                // if the rule is running we can directly add the diagnostic
                // without checking for the exceptable nodes
                state.add(missing_ending_newline(Span::new(
                    usize::from(last.text_range().end()) - 1,
                    1,
                )));
            }
            None => {
                // Completely empty file is okay, at least with regard to this
                // lint rule
            }
        }
    }
}
