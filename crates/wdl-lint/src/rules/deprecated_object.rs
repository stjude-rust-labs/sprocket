//! A lint rule for flagging `Object`s as deprecated.

use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;
use wdl_ast::v1::Type;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the deprecated object rule.
const ID: &str = "DeprecatedObject";

/// Creates a deprecated object use diagnostic.
fn deprecated_object_use(span: Span) -> Diagnostic {
    Diagnostic::note(String::from("use of a deprecated `Object` type"))
        .with_rule(ID)
        .with_highlight(span)
        .with_fix("replace the `Object` with a `Map` or a `Struct`")
}

/// Detects the use of the deprecated `Object` types.
#[derive(Default, Debug, Clone, Copy)]
pub struct DeprecatedObjectRule;

impl Rule for DeprecatedObjectRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that the deprecated `Object` types are not used."
    }

    fn explanation(&self) -> &'static str {
        "WDL `Object` types are officially deprecated and will be removed in the next major WDL release.

        `Object`s existed prior to better containers, such as `Map`s and `Struct`s, being \
         introduced into the language. Unfortunately, though these better alternatives did exist at \
         the time of the v1.0 release, the type was not removed. It was later decided \
         that `Object`s overlapped with `Map`s and `Struct`s in functionality, and the type was marked for removal.

         See this issue for more details: https://github.com/openwdl/wdl/pull/228."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Deprecated])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
            SyntaxKind::BoundDeclNode,
            SyntaxKind::UnboundDeclNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &["DeprecatedPlaceholderOption", "RuntimeSectionKeys"]
    }
}

impl Visitor for DeprecatedObjectRule {
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

    fn bound_decl(
        &mut self,
        state: &mut Self::State,
        reason: wdl_ast::VisitReason,
        decl: &wdl_ast::v1::BoundDecl,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Type::Object(ty) = decl.ty() {
            state.exceptable_add(
                deprecated_object_use(ty.span()),
                SyntaxElement::from(decl.inner().clone()),
                &self.exceptable_nodes(),
            )
        }
    }

    fn unbound_decl(
        &mut self,
        state: &mut Self::State,
        reason: wdl_ast::VisitReason,
        decl: &wdl_ast::v1::UnboundDecl,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Type::Object(ty) = decl.ty() {
            state.exceptable_add(
                deprecated_object_use(ty.span()),
                SyntaxElement::from(decl.inner().clone()),
                &self.exceptable_nodes(),
            )
        }
    }
}
