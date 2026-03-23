//! A lint rule for disallowing the use of glob patterns with only star.

use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::Expr;
use wdl_ast::v1::LiteralExpr;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the disallowed glob star rule.
const ID: &str = "DenyGlobStar";

/// Creates a diagnostic for a `glob("*")` pattern in an output declaration.
fn glob_star_diagnostic(span: Span) -> Diagnostic {
    Diagnostic::warning("glob pattern \"*\" matches all files")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("use a more specific pattern to avoid capturing unintended files")
}

/// A lint rule for disallowing the use of glob patterns with only star.
#[derive(Clone, Copy, Debug, Default)]
pub struct DenyGlobStar;

impl Rule for DenyGlobStar {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures glob(\"*\") is not used in output declarations."
    }

    fn explanation(&self) -> &'static str {
        "glob(\"*\") captures all files; As a task grows, you may include unintended files and \
         lead to unnecessary aggregation. Prefer explicit patterns to opt in only to the files you \
         need, keeping tasks easier to debug/reproduce."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Correctness, Tag::Clarity])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::OutputSectionNode,
            SyntaxKind::BoundDeclNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for DenyGlobStar {
    fn reset(&mut self) {
        *self = Self;
    }

    fn bound_decl(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, decl: &BoundDecl) {
        if decl
            .inner()
            .parent()
            .is_none_or(|p| p.kind() != SyntaxKind::OutputSectionNode)
            || reason != VisitReason::Enter
        {
            return;
        }

        if let Expr::Call(call) = decl.expr()
            && call.target().text() == ("glob")
        {
            for argument in call.arguments() {
                if let Expr::Literal(LiteralExpr::String(s)) = argument
                    && s.text().is_some_and(|t| t.text() == "*")
                {
                    diagnostics.exceptable_add(
                        glob_star_diagnostic(s.span()),
                        SyntaxElement::from(decl.inner().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            }
        }
    }
}
