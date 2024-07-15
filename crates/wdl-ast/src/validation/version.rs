//! Validation of supported syntax for WDL versions.

use rowan::ast::support::token;
use wdl_grammar::version::V1;
use wdl_grammar::ToSpan;

use crate::v1;
use crate::v1::Expr;
use crate::AstNode;
use crate::Diagnostic;
use crate::Diagnostics;
use crate::Document;
use crate::Span;
use crate::SupportedVersion;
use crate::SyntaxKind;
use crate::VisitReason;
use crate::Visitor;

/// Creates an "exponentiation requirement" diagnostic.
fn exponentiation_requirement(span: Span) -> Diagnostic {
    Diagnostic::error("use of the exponentiation operator requires WDL version 1.2")
        .with_highlight(span)
}

/// Creates a "requirements section requirement" diagnostic.
fn requirements_section(span: Span) -> Diagnostic {
    Diagnostic::error("use of the `requirements` section requires WDL version 1.2")
        .with_highlight(span)
}

/// Creates a "hints section requirement" diagnostic.
fn hints_section(span: Span) -> Diagnostic {
    Diagnostic::error("use of the `hints` section requires WDL version 1.2").with_highlight(span)
}

/// Creates a "multi-line string requirement" diagnostic.
fn multiline_string_requirement(span: Span) -> Diagnostic {
    Diagnostic::error("use of multi-line strings requires WDL version 1.2").with_highlight(span)
}

/// An AST visitor that ensures the syntax present in the document matches the
/// document's declared version.
#[derive(Debug, Default)]
pub struct VersionVisitor {
    /// Stores the supported version of the WDL document we're visiting.
    version: Option<SupportedVersion>,
}

impl Visitor for VersionVisitor {
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

        self.version = Some(version);
    }

    fn requirements_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::RequirementsSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(version) = self.version {
            if version < SupportedVersion::V1(V1::Two) {
                state.add(requirements_section(
                    token(section.syntax(), SyntaxKind::RequirementsKeyword)
                        .expect("should have keyword")
                        .text_range()
                        .to_span(),
                ));
            }
        }
    }

    fn hints_section(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        section: &v1::HintsSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(version) = self.version {
            if version < SupportedVersion::V1(V1::Two) {
                state.add(hints_section(
                    token(section.syntax(), SyntaxKind::HintsKeyword)
                        .expect("should have keyword")
                        .text_range()
                        .to_span(),
                ));
            }
        }
    }

    fn expr(&mut self, state: &mut Self::State, reason: VisitReason, expr: &v1::Expr) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(version) = self.version {
            match expr {
                Expr::Exponentiation(e) if version < SupportedVersion::V1(V1::Two) => {
                    state.add(exponentiation_requirement(
                        token(e.syntax(), SyntaxKind::Exponentiation)
                            .expect("should have operator")
                            .text_range()
                            .to_span(),
                    ));
                }
                v1::Expr::Literal(v1::LiteralExpr::String(s))
                    if version < SupportedVersion::V1(V1::Two)
                        && s.kind() == v1::LiteralStringKind::Multiline =>
                {
                    state.add(multiline_string_requirement(
                        s.syntax().text_range().to_span(),
                    ));
                }
                _ => {}
            }
        }
    }
}
