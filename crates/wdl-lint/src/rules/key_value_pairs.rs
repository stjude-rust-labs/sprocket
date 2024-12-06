//! A lint rule for key-value pairs to ensure each element is on a newline.

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
use wdl_ast::v1::MetadataArray;
use wdl_ast::v1::MetadataObject;

use crate::Rule;
use crate::Tag;
use crate::TagSet;
use crate::rules::trailing_comma::find_next_comma;

/// Set indentation string
const INDENT: &str = "    ";

/// The identifier for the missing meta sections rule.
const ID: &str = "KeyValuePairs";

/// Diagnostic message for missing trailing newline.
fn missing_trailing_newline(span: Span) -> Diagnostic {
    Diagnostic::note("item should be followed by a newline")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a newline after this item")
}

/// Diagnostic message for all on one line.
fn all_on_one_line(span: Span) -> Diagnostic {
    Diagnostic::note("all items in an array or object should be on separate lines")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("put each item on a separate line")
}

/// Diagnostic for incorrect indentation.
fn incorrect_indentation(span: Span, expected: &str, actual: &str) -> Diagnostic {
    if expected.len() > actual.len() {
        Diagnostic::note("incorrect indentation")
            .with_rule(ID)
            .with_highlight(span)
            .with_fix(format!(
                "add {} spaces to indentation",
                (expected.len() - actual.len())
            ))
    } else {
        Diagnostic::note("incorrect indentation")
            .with_rule(ID)
            .with_highlight(span)
            .with_fix(format!(
                "remove {} spaces of indentation",
                (actual.len() - expected.len())
            ))
    }
}

/// A lint rule for missing meta and parameter_meta sections.
#[derive(Default, Debug, Clone, Copy)]
pub struct KeyValuePairsRule {
    /// The version of the WDL document being linted.
    version: Option<SupportedVersion>,
}

impl Rule for KeyValuePairsRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that arrays and objects in `meta` and `parameter_meta` sections have one element \
         per line and are indented correctly."
    }

    fn explanation(&self) -> &'static str {
        "All lists and objects in the `meta` and `parameter_meta` sections should have one element \
         per line (i.e. newline separate elements). A key/value pair are considered one element if \
         the value is atomic (i.e. not a list or an object). Otherwise have the key and opening \
         bracket on the same line; subsequently indent one level; put one value per line; and have \
         the closing bracket on its own line at the same indentation level of the key."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
            SyntaxKind::StructDefinitionNode,
            SyntaxKind::MetadataSectionNode,
            SyntaxKind::ParameterMetadataSectionNode,
        ])
    }
}

impl Visitor for KeyValuePairsRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
        self.version = Some(version);
    }

    fn metadata_object(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        item: &MetadataObject,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let tmp = item
            .syntax()
            .parent()
            .expect("should have a parent")
            .prev_sibling_or_token()
            .expect("should have a prior token")
            .into_token()
            .expect("should have a token")
            .to_string();
        let parent_ws = tmp.split('\n').last().expect("should have indentation");

        if !item.syntax().to_string().contains('\n') {
            state.exceptable_add(
                all_on_one_line(item.syntax().text_range().to_span()),
                SyntaxElement::from(item.syntax().clone()),
                &self.exceptable_nodes(),
            );
            return;
        }

        // Check if the open delimiter has a newline after it
        let open_delim = item
            .syntax()
            .first_token()
            .expect("should have an opening delimiter");
        if let Some(open_ws) = open_delim.next_sibling_or_token() {
            if open_ws.kind() != SyntaxKind::Whitespace || !open_ws.to_string().contains('\n') {
                state.exceptable_add(
                    missing_trailing_newline(open_delim.text_range().to_span()),
                    SyntaxElement::from(item.syntax().clone()),
                    &self.exceptable_nodes(),
                );
            }
        }

        // Check if object is multi-line
        let close_delim = item
            .syntax()
            .last_token()
            .expect("should have a closing delimiter");
        for child in item.items() {
            let (next_newline, _newline_is_next) = find_next_newline(child.syntax());
            if next_newline.is_none() {
                // No newline found, report missing
                let s = child.syntax().text_range().to_span();
                let end = if let Some(next) = find_next_comma(child.syntax()).0 {
                    next.text_range().end()
                } else {
                    close_delim.text_range().start()
                };
                state.exceptable_add(
                    missing_trailing_newline(Span::new(s.start(), usize::from(end) - s.start())),
                    SyntaxElement::from(child.syntax().clone()),
                    &self.exceptable_nodes(),
                );
            }
            // Check indentation. If there is no prior whitespace, that will have been
            // reported already.
            if let Some(prior_ws) = child.syntax().prev_sibling_or_token() {
                if prior_ws.kind() == SyntaxKind::Whitespace && prior_ws.to_string().contains('\n')
                {
                    // If there was no newline, that is already reported
                    let ws = prior_ws.to_string();
                    let ws = ws.split('\n').last().expect("should have a last element");
                    let expected_ws = parent_ws.to_owned() + INDENT;

                    if ws != expected_ws {
                        state.exceptable_add(
                            incorrect_indentation(
                                prior_ws.text_range().to_span(),
                                &expected_ws,
                                ws,
                            ),
                            SyntaxElement::from(child.syntax().clone()),
                            &self.exceptable_nodes(),
                        );
                    }
                }
            }
        }

        // No need to check the closing delimiter as the last element must have
        // a newline. But we should check the indentation of the closing delimiter.
        if let Some(prior_ws) = close_delim.prev_sibling_or_token() {
            if prior_ws.kind() == SyntaxKind::Whitespace && prior_ws.to_string().contains('\n') {
                let ws = prior_ws.to_string();
                let ws = ws
                    .split('\n')
                    .last()
                    .expect("there should be a last element");
                let expected_ws = parent_ws.to_owned();

                if ws != expected_ws {
                    state.exceptable_add(
                        incorrect_indentation(
                            Span::new(
                                usize::from(close_delim.text_range().start()) - ws.len(),
                                ws.len(),
                            ),
                            &expected_ws,
                            ws,
                        ),
                        SyntaxElement::from(item.syntax().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            }
        }
    }

    fn metadata_array(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        item: &MetadataArray,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let tmp = item
            .syntax()
            .parent()
            .expect("should have a parent")
            .prev_sibling_or_token()
            .expect("should have a prior token")
            .into_token()
            .expect("should have a token")
            .to_string();
        let parent_ws = tmp.split('\n').last().expect("should have indentation");

        // If the array is all on one line, report that
        if !item.syntax().to_string().contains('\n') {
            state.exceptable_add(
                all_on_one_line(item.syntax().text_range().to_span()),
                SyntaxElement::from(item.syntax().clone()),
                &self.exceptable_nodes(),
            );
            return;
        }

        // Check if the open delimiter has a newline after it
        let open_delim = item
            .syntax()
            .first_token()
            .expect("should have an opening delimiter");
        if let Some(open_ws) = open_delim.next_sibling_or_token() {
            if open_ws.kind() != SyntaxKind::Whitespace || !open_ws.to_string().contains('\n') {
                state.exceptable_add(
                    missing_trailing_newline(open_delim.text_range().to_span()),
                    SyntaxElement::from(item.syntax().clone()),
                    &self.exceptable_nodes(),
                );
            }
        }

        // Metadata arrays should be one element per line
        let close_delim = item
            .syntax()
            .last_token()
            .expect("should have a closing delimiter");
        for child in item.elements() {
            let (next_newline, _newline_is_next) = find_next_newline(child.syntax());
            if next_newline.is_none() {
                // No newline found, report missing
                let s = child.syntax().text_range().to_span();
                let end = if let Some(next) = find_next_comma(child.syntax()).0 {
                    next.text_range().end()
                } else {
                    close_delim.text_range().start()
                };
                state.exceptable_add(
                    missing_trailing_newline(Span::new(s.start(), usize::from(end) - s.start())),
                    SyntaxElement::from(child.syntax().clone()),
                    &self.exceptable_nodes(),
                );
            }
            // Check indentation. If there is no prior whitespace, that will have been
            // reported already.
            if let Some(prior_ws) = child.syntax().prev_sibling_or_token() {
                if prior_ws.kind() == SyntaxKind::Whitespace && prior_ws.to_string().contains('\n')
                {
                    // If there was no newline, that is already reported
                    let ws = prior_ws.to_string();
                    let ws = ws
                        .split('\n')
                        .last()
                        .expect("there should be a last element");
                    let expected_ws = parent_ws.to_owned() + INDENT;

                    if ws != expected_ws {
                        state.exceptable_add(
                            incorrect_indentation(
                                prior_ws.text_range().to_span(),
                                &expected_ws,
                                ws,
                            ),
                            SyntaxElement::from(child.syntax().clone()),
                            &self.exceptable_nodes(),
                        );
                    }
                }
            }
        }

        // No need to check the closing delimiter as the last element must have
        // a newline. But we should check the indentation of the closing delimiter.
        if let Some(prior_ws) = close_delim.prev_sibling_or_token() {
            if prior_ws.kind() == SyntaxKind::Whitespace && prior_ws.to_string().contains('\n') {
                let ws = prior_ws.to_string();
                let ws = ws
                    .split('\n')
                    .last()
                    .expect("there should be a last element");
                let expected_ws = parent_ws.to_owned();

                if ws != expected_ws {
                    state.exceptable_add(
                        incorrect_indentation(
                            Span::new(
                                usize::from(close_delim.text_range().start()) - ws.len(),
                                ws.len(),
                            ),
                            &expected_ws,
                            ws,
                        ),
                        SyntaxElement::from(item.syntax().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            }
        }
    }
}

/// Find the next newline by consuming tokens until we hit a newline or a node.
/// Returns the next newline token and whether it immediately follows this node.
fn find_next_newline(node: &wdl_ast::SyntaxNode) -> (Option<wdl_ast::SyntaxToken>, bool) {
    let mut next = node.next_sibling_or_token();
    let mut is_next = true;
    while let Some(next_node) = next {
        // If we find a node before a newline, treat it as no newline.
        // If we find other tokens, then mark that they precede any potential newline.
        if next_node.as_node().is_some() {
            return (None, false);
        } else if next_node.kind() == SyntaxKind::Whitespace && next_node.to_string().contains('\n')
        {
            return (Some(next_node.into_token().unwrap()), is_next);
        } else {
            is_next = false;
        }
        next = next_node.next_sibling_or_token();
    }
    (None, false)
}
