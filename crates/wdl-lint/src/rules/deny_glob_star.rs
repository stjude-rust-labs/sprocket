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
use wdl_ast::v1::OutputSection;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the disallowed glob star rule.
const ID: &str = "DenyGlobStar";

/// Declaration Identifier for glob star in output
fn glob_star_diagnostic(span: Span) -> Diagnostic {
    Diagnostic::warning("glob pattern \"*\" matches all files")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("use a more specific pattern to avoid capturing unintended files")
}

/// A lint rule for disallowing the use of glob patterns with only star.
#[derive(Default, Debug, Clone, Copy)]
pub struct DenyGlobStar {
    /// Track if we're in the output section.
    output_section: bool,
}

impl Rule for DenyGlobStar {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures glob(\"*\") is not used in output declarations."
    }

    fn explanation(&self) -> &'static str {
        "glob(\"*\") captures all files; use an explicit pattern instead as you may capture \
         unintended files and make the task harder to debug/reproduce."
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
        *self = Default::default();
    }

    fn output_section(&mut self, _: &mut Diagnostics, reason: VisitReason, _: &OutputSection) {
        self.output_section = reason == VisitReason::Enter;
    }

    fn bound_decl(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, decl: &BoundDecl) {
        if reason == VisitReason::Enter && self.output_section {
            // Checking if the expression contains glob("*")
            if let Expr::Call(call) = decl.expr()
                && call.target().text().eq("glob")
            {
                for argument in call.arguments() {
                    if argument.text().to_string().eq("\"*\"") {
                        diagnostics.exceptable_add(
                            glob_star_diagnostic(argument.span()),
                            SyntaxElement::from(decl.inner().clone()),
                            &self.exceptable_nodes(),
                        );
                    }
                }
            }
        }
    }
}
