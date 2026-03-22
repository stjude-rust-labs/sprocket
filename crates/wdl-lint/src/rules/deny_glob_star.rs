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
use wdl_ast::v1::Decl;
use wdl_ast::v1::OutputSection;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the disallowed glob star rule.
const ID: &str = "DenyGlobStar";

/// Declaration Identifier for glob star in output
fn glob_star_diagnostic(span: Span) -> Diagnostic {
    Diagnostic::warning("glob patterns with only * should not be used in output declarations")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("use an explicit glob pattern to avoid unintended consequences")
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
        "Ensures glob(*) is not used in output declarations."
    }

    fn explanation(&self) -> &'static str {
        "glob(*) captures all files; use an explicit pattern instead to avoid unintended \
         consequences."
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
            check_glob_star(
                diagnostics,
                &Decl::Bound(decl.clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}

/// Check declaration name
fn check_glob_star(
    diagnostics: &mut Diagnostics,
    decl: &Decl,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    // Grabbing the expression from the declaration
    let expr = decl.expr();

    // Ensuring expression is not None
    if let Some(value) = expr {
        // Checking if the expression is a call
        if let Some(call) = value.as_call() {
            // Checking if the target of the call is glob
            let func = call.target();
            if func.text().eq("glob") {
                // Checking if the arguments (should only be 1) contain a glob star
                for argument in call.arguments() {
                    if argument.text().to_string().eq("\"*\"") {
                        // Adding diagnostic
                        diagnostics.exceptable_add(
                            glob_star_diagnostic(argument.span()),
                            SyntaxElement::from(decl.inner().clone()),
                            exceptable_nodes,
                        );
                    }
                }
            }
        }
    }
}
