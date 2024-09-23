//! A lint rule for trailing commas in lists/objects.

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
use wdl_ast::v1::Expr;
use wdl_ast::v1::LiteralExpr;
use wdl_ast::v1::MetadataArray;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the trailing comma rule.
const ID: &str = "TrailingComma";

/// Diagnostic message for missing trailing comma.
fn missing_trailing_comma(span: Span) -> Diagnostic {
    Diagnostic::note("item missing trailing comma")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("add a comma after this element")
}

/// Diagnostic message for extraneous content before trailing comma.
fn extraneous_content(span: Span) -> Diagnostic {
    Diagnostic::note("extraneous whitespace and/or comments before trailing comma")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("remove this extraneous content")
}

/// Detects missing trailing commas.
#[derive(Default, Debug, Clone, Copy)]
pub struct TrailingCommaRule;

impl Rule for TrailingCommaRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that lists and objects have a trailing comma and that there's not extraneous \
         whitespace and/or comments before the trailing comma."
    }

    fn explanation(&self) -> &'static str {
        "All items in a comma-delimited object or list should be followed by a comma, including \
         the last item. An exception is made for lists for which all items are on the same line, \
         in which case there should not be a trailing comma following the last item. Note that \
         single-line lists are not allowed in the `meta` or `parameter_meta` sections. This method \
         checks `arrays` and `objects` in `meta` and `parameter_meta` sections. It also checks \
         `call` input blocks as well as `Array`, `Map`, `Object`, and `Struct` literals."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::MetadataSectionNode,
            SyntaxKind::ParameterMetadataSectionNode,
            SyntaxKind::MetadataArrayNode,
            SyntaxKind::MetadataObjectNode,
            SyntaxKind::CallStatementNode,
            SyntaxKind::LiteralStructNode,
            SyntaxKind::LiteralArrayNode,
            SyntaxKind::LiteralMapNode,
            SyntaxKind::LiteralObjectNode,
        ])
    }
}

impl Visitor for TrailingCommaRule {
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

    fn metadata_object(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        item: &wdl_ast::v1::MetadataObject,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Check if object is multi-line
        if item.syntax().to_string().contains('\n') && item.items().count() > 1 {
            let last_child = item.items().last();
            if let Some(last_child) = last_child {
                let (next_comma, comma_is_next) = find_next_comma(last_child.syntax());
                if let Some(comma) = next_comma {
                    if !comma_is_next {
                        // Comma found, but not next, extraneous trivia
                        state.exceptable_add(
                            extraneous_content(Span::new(
                                usize::from(last_child.syntax().text_range().end()),
                                usize::from(
                                    comma.text_range().start()
                                        - last_child.syntax().text_range().end(),
                                ),
                            )),
                            SyntaxElement::from(item.syntax().clone()),
                            &self.exceptable_nodes(),
                        );
                    }
                } else {
                    // No comma found, report missing
                    state.exceptable_add(
                        missing_trailing_comma(last_child.syntax().text_range().to_span()),
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

        // Check if array is multi-line
        if item.syntax().to_string().contains('\n') && item.elements().count() > 1 {
            let last_child = item.elements().last();
            if let Some(last_child) = last_child {
                let (next_comma, comma_is_next) = find_next_comma(last_child.syntax());
                if let Some(comma) = next_comma {
                    if !comma_is_next {
                        // Comma found, but not next, extraneous trivia
                        state.exceptable_add(
                            extraneous_content(Span::new(
                                usize::from(last_child.syntax().text_range().end()),
                                usize::from(
                                    comma.text_range().start()
                                        - last_child.syntax().text_range().end(),
                                ),
                            )),
                            SyntaxElement::from(item.syntax().clone()),
                            &self.exceptable_nodes(),
                        );
                    }
                } else {
                    // No comma found, report missing
                    state.exceptable_add(
                        missing_trailing_comma(last_child.syntax().text_range().to_span()),
                        SyntaxElement::from(item.syntax().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            }
        }
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

        if inputs < 2 {
            return;
        }

        call.inputs().for_each(|input| {
            // check each input for trailing comma
            let (next_comma, comma_is_next) = find_next_comma(input.syntax());
            if let Some(nc) = next_comma {
                if !comma_is_next {
                    state.exceptable_add(
                        extraneous_content(Span::new(
                            usize::from(input.syntax().text_range().end()),
                            usize::from(
                                nc.text_range().start() - input.syntax().text_range().end(),
                            ),
                        )),
                        SyntaxElement::from(call.syntax().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            } else {
                state.exceptable_add(
                    missing_trailing_comma(input.syntax().text_range().to_span()),
                    SyntaxElement::from(call.syntax().clone()),
                    &self.exceptable_nodes(),
                );
            }
        });
    }

    fn expr(&mut self, state: &mut Self::State, reason: VisitReason, expr: &Expr) {
        if reason == VisitReason::Exit {
            return;
        }
        if let Expr::Literal(l) = expr {
            match l {
                // items: map, object, struct
                // elements: array
                LiteralExpr::Array(_)
                | LiteralExpr::Map(_)
                | LiteralExpr::Object(_)
                | LiteralExpr::Struct(_) => {
                    // Check if array is multi-line
                    if l.syntax().to_string().contains('\n') && l.syntax().children().count() > 1 {
                        let last_child = l.syntax().children().last();
                        if let Some(last_child) = last_child {
                            let (next_comma, comma_is_next) = find_next_comma(&last_child);
                            if let Some(comma) = next_comma {
                                if !comma_is_next {
                                    // Comma found, but not next, extraneous trivia
                                    state.exceptable_add(
                                        extraneous_content(Span::new(
                                            usize::from(last_child.text_range().end()),
                                            usize::from(
                                                comma.text_range().start()
                                                    - last_child.text_range().end(),
                                            ),
                                        )),
                                        SyntaxElement::from(l.syntax().clone()),
                                        &self.exceptable_nodes(),
                                    );
                                }
                            } else {
                                // No comma found, report missing
                                state.exceptable_add(
                                    missing_trailing_comma(last_child.text_range().to_span()),
                                    SyntaxElement::from(l.syntax().clone()),
                                    &self.exceptable_nodes(),
                                );
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Find the next comma by consuming until we find a comma or a node.
pub(crate) fn find_next_comma(node: &wdl_ast::SyntaxNode) -> (Option<wdl_ast::SyntaxToken>, bool) {
    let mut next = node.next_sibling_or_token();
    let mut comma_is_next = true;
    while let Some(next_node) = next {
        // If we find a node before a comma, then treat as no comma
        // If we find other tokens, then mark that they precede any potential comma
        if next_node.as_node().is_some() {
            return (None, false);
        } else if next_node.kind() == wdl_ast::SyntaxKind::Comma {
            return (Some(next_node.into_token().unwrap()), comma_is_next);
        } else {
            comma_is_next = false;
        }
        next = next_node.next_sibling_or_token();
    }
    (None, false)
}
