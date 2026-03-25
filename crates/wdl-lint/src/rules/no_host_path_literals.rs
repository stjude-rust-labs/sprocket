//! A lint rule that flags absolute host-path literals in `File`/`Directory`
//! declaration defaults.

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
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::PrimitiveTypeKind;
use wdl_ast::v1::Type;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the no host path literals rule.
const ID: &str = "NoHostPathLiterals";

/// Creates an absolute host-path default diagnostic.
fn absolute_host_path_default(span: Span, decl_name: &str) -> Diagnostic {
    Diagnostic::note(format!(
        "declaration `{decl_name}` has an absolute host path default",
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix(
        "use a relative path for input/private declarations, or pass the path at runtime instead",
    )
}

/// Flags absolute host-path literals for File/Directory declaration defaults.
#[derive(Default, Debug, Clone, Copy)]
pub struct NoHostPathLiteralsRule {
    /// Whether the current declaration is inside an output section.
    output_section: bool,
}

impl Rule for NoHostPathLiteralsRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Flags File/Directory declaration defaults that use absolute host paths."
    }

    fn explanation(&self) -> &'static str {
        "File and Directory declarations with absolute path defaults are not portable across \
         environments. Use relative paths or supply values at runtime."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Portability])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::InputSectionNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
            SyntaxKind::BoundDeclNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for NoHostPathLiteralsRule {
    fn reset(&mut self) {
        *self = Default::default();
    }

    fn output_section(&mut self, _: &mut Diagnostics, reason: VisitReason, _: &OutputSection) {
        self.output_section = reason == VisitReason::Enter;
    }

    fn bound_decl(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, decl: &BoundDecl) {
        if reason == VisitReason::Exit || self.output_section {
            return;
        }

        if !is_file_or_dir(&decl.ty()) {
            return;
        }

        let expr = decl.expr();
        if let Expr::Literal(LiteralExpr::String(s)) = expr
            && let Some(text) = s.text()
            && text.text().starts_with('/')
        {
            diagnostics.exceptable_add(
                absolute_host_path_default(s.span(), decl.name().text()),
                SyntaxElement::from(decl.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}

/// Returns true for primitive `File` and `Directory` declarations.
fn is_file_or_dir(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Primitive(t)
            if matches!(
                t.kind(),
                PrimitiveTypeKind::File | PrimitiveTypeKind::Directory
            )
    )
}
