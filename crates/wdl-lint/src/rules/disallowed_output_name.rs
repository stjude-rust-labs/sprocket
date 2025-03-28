//! A lint rule that disallows redundant output names.

use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::Decl;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::UnboundDecl;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the disallowed output name rule.
const ID: &str = "DisallowedOutputName";

/// Declaration identifier too short
fn decl_identifier_too_short(span: Span) -> Diagnostic {
    Diagnostic::note("declaration identifier must be at least 3 characters")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("rename the identifier to be at least 3 characters long")
}

/// Diagnostic for input names that start with [oO]ut[A-Z_]
fn decl_identifier_starts_with_out(span: Span) -> Diagnostic {
    Diagnostic::note("declaration identifier starts with 'out'")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("rename the identifier to not start with 'out'")
}

/// Diagnostic for input names that start with "output"
fn decl_identifier_starts_with_output(span: Span) -> Diagnostic {
    Diagnostic::note("declaration identifier starts with 'output'")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("rename the identifier to not start with 'output'")
}

/// A lint rule for disallowed output names.
#[derive(Default, Debug, Clone, Copy)]
pub struct DisallowedOutputNameRule {
    /// Track if we're in the output section.
    output_section: bool,
}

impl Rule for DisallowedOutputNameRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures output names are meaningful."
    }

    fn explanation(&self) -> &'static str {
        "Any output name matching these regular expressions will be flagged: /^[oO]ut[A-Z_]/, \
         /^output/i or /^..?$/. It is redundant and needlessly verbose to use an output's name to \
         specify that it is an output. Output names should be short yet descriptive. Prefixing a \
         name with out or output adds length to the name without adding clarity or context. \
         Additionally, names with only 2 characters can lead to confusion and obfuscates the \
         content of an output. Output names should be at least 3 characters long."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Naming])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::OutputSectionNode,
            SyntaxKind::BoundDeclNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &["DisallowedInputName", "DisallowedDeclarationName"]
    }
}

impl Visitor for DisallowedOutputNameRule {
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

    fn output_section(&mut self, _: &mut Self::State, reason: VisitReason, _: &OutputSection) {
        self.output_section = reason == VisitReason::Enter;
    }

    fn bound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &BoundDecl) {
        if reason == VisitReason::Enter && self.output_section {
            check_decl_name(state, &Decl::Bound(decl.clone()), &self.exceptable_nodes());
        }
    }

    fn unbound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &UnboundDecl) {
        if reason == VisitReason::Enter && self.output_section {
            check_decl_name(
                state,
                &Decl::Unbound(decl.clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}

/// Check declaration name
fn check_decl_name(
    state: &mut Diagnostics,
    decl: &Decl,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    let name = decl.name();
    let name = name.text();

    let length = name.len();
    if length < 3 {
        // name is too short
        state.exceptable_add(
            decl_identifier_too_short(decl.name().span()),
            SyntaxElement::from(decl.inner().clone()),
            exceptable_nodes,
        );
    }

    let mut name = name.chars().peekable();
    if let Some(c) = name.next() {
        if c == 'o' || c == 'O' {
            if let Some('u') = name.peek() {
                name.next();
                if let Some('t') = name.peek() {
                    name.next();
                    if let Some(c) = name.peek() {
                        if c.is_ascii_uppercase() || c == &'_' {
                            // name starts with "out"
                            state.exceptable_add(
                                decl_identifier_starts_with_out(decl.name().span()),
                                SyntaxElement::from(decl.inner().clone()),
                                exceptable_nodes,
                            );
                        } else {
                            let s: String = name.take(3).collect();
                            if s == "put" {
                                // name starts with "output"
                                state.exceptable_add(
                                    decl_identifier_starts_with_output(decl.name().span()),
                                    SyntaxElement::from(decl.inner().clone()),
                                    exceptable_nodes,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
