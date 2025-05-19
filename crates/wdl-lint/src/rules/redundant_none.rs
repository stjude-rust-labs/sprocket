//! A lint rule for flagging redundant `= None` assignments for
//! optional inputs.

use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::Expr;
use wdl_ast::v1::LiteralExpr;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the redundant none rule.
const ID: &str = "RedundantNone";

/// Create a "redundant `= None` assignment" diagnostic
fn redundant_none(span: Span, name: &str) -> Diagnostic {
    Diagnostic::note(format!(
        "redundant assignment of `None` to optional input `{name}`"
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_fix(format!("remove `= None` for input `{name}`"))
}

/// A rule that identifies redundant `= None` assignments for
/// optional inputs.
#[derive(Debug, Default, Clone)]
pub struct RedundantNone;

impl Rule for RedundantNone {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Flags redundant assignment of `None` to optional inputs."
    }

    fn explanation(&self) -> &'static str {
        "The specification states that an optional input declaration (e.g., `String? foo`) is \
         implicitly initialized to `None` if no default is provided. Therefore explicitly writing \
         `String? foo = None` is equivalent to `String? foo` but adds unnecessary verbosity."
    }

    fn tags(&self) -> crate::TagSet {
        TagSet::new(&[Tag::Style, Tag::Clarity])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::InputSectionNode,
            SyntaxKind::BoundDeclNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for RedundantNone {
    fn reset(&mut self) {
        *self = Default::default();
    }

    fn bound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &wdl_ast::v1::BoundDecl,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if decl
            .inner()
            .parent()
            .is_none_or(|p| p.kind() != SyntaxKind::InputSectionNode)
        {
            return;
        }

        if !decl.ty().is_optional() {
            return;
        }

        let expr = decl.expr();
        if matches!(expr, Expr::Literal(LiteralExpr::None(_))) {
            let diagnostic = redundant_none(expr.span(), decl.name().text());
            diagnostics.exceptable_add(
                diagnostic,
                SyntaxElement::from(decl.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}
