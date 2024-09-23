//! A lint rule that disallows redundant input names.

use wdl_ast::AstNode;
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
use wdl_ast::v1::InputSection;
use wdl_ast::v1::UnboundDecl;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the disallowed input name rule.
const ID: &str = "DisallowedInputName";

/// Declaration identifier too short
fn decl_identifier_too_short(span: Span) -> Diagnostic {
    Diagnostic::note("declaration identifier must be at least 3 characters")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("rename the declaration to a name with at least 3 characters")
}

/// Diagnostic for input names that start with [iI]n[A-Z_]
fn decl_identifier_starts_with_in(span: Span) -> Diagnostic {
    Diagnostic::note("declaration identifier starts with 'in'")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("rename the declaration to a name that does not start with 'in'")
}

/// Diagnostic for input names that start with "input"
fn decl_identifier_starts_with_input(span: Span) -> Diagnostic {
    Diagnostic::note("declaration identifier starts with 'input'")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("rename the declaration to a name that does not start with 'input'")
}

/// A lint rule for disallowed input names.
#[derive(Default, Debug, Clone, Copy)]
pub struct DisallowedInputNameRule {
    /// Track if we're in the input section.
    input_section: bool,
}

impl Rule for DisallowedInputNameRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures input names are meaningful."
    }

    fn explanation(&self) -> &'static str {
        "Any input name matching these regular expressions will be flagged: /^[iI]n[A-Z_]/, \
         /^input/i or /^..?$/. It is redundant and needlessly verbose to use an input's name to \
         specify that it is an input. Input names should be short yet descriptive. Prefixing a \
         name with in or input adds length to the name without adding clarity or context. \
         Additionally, names with only 2 characters can lead to confusion and obfuscates the \
         content of an input. Input names should be at least 3 characters long."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Naming])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::InputSectionNode,
            SyntaxKind::BoundDeclNode,
            SyntaxKind::UnboundDeclNode,
        ])
    }
}

impl Visitor for DisallowedInputNameRule {
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

    fn input_section(&mut self, _: &mut Self::State, reason: VisitReason, _: &InputSection) {
        self.input_section = reason == VisitReason::Enter;
    }

    fn bound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &BoundDecl) {
        if reason == VisitReason::Enter && self.input_section {
            check_decl_name(state, &Decl::Bound(decl.clone()), &self.exceptable_nodes());
        }
    }

    fn unbound_decl(&mut self, state: &mut Self::State, reason: VisitReason, decl: &UnboundDecl) {
        if reason == VisitReason::Enter && self.input_section {
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
    let name = name.as_str();

    let length = name.len();
    if length < 3 {
        // name is too short
        state.exceptable_add(
            decl_identifier_too_short(decl.name().span()),
            SyntaxElement::from(decl.syntax().clone()),
            exceptable_nodes,
        );
    }

    let mut name = name.chars().peekable();
    if let Some(c) = name.next() {
        if c == 'i' || c == 'I' {
            if let Some('n') = name.peek() {
                name.next();
                if let Some(c) = name.peek() {
                    if c.is_ascii_uppercase() || c == &'_' {
                        // name starts with "in"
                        state.exceptable_add(
                            decl_identifier_starts_with_in(decl.name().span()),
                            SyntaxElement::from(decl.syntax().clone()),
                            exceptable_nodes,
                        );
                    } else {
                        let s: String = name.take(3).collect();
                        if s == "put" {
                            // name starts with "input"
                            state.exceptable_add(
                                decl_identifier_starts_with_input(decl.name().span()),
                                SyntaxElement::from(decl.syntax().clone()),
                                exceptable_nodes,
                            );
                        }
                    }
                }
            }
        }
    }
}
