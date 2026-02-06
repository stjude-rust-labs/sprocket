//! A lint rule for flagging `Object`s as deprecated.

use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
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

    fn examples(&self) -> &'static [&'static str] {
        &[
            r#"```wdl
version 1.2

workflow example {
    meta {}

    Object person = object {
        name: "Jimmy",
        age: 55,
    }

    output {}
}
```"#,
            r#"Use instead:

```wdl
version 1.2

struct Person {
    String name
    Int age
}

workflow example {
    meta {}

    Person person = Person {
        name: "Jimmy",
        age: 55,
    }

    output {}
}
```"#,
        ]
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
        &["DeprecatedPlaceholder", "ExpectedRuntimeKeys"]
    }
}

impl Visitor for DeprecatedObjectRule {
    fn reset(&mut self) {
        *self = Self;
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

        if let Type::Object(ty) = decl.ty() {
            diagnostics.exceptable_add(
                deprecated_object_use(ty.span()),
                SyntaxElement::from(decl.inner().clone()),
                &self.exceptable_nodes(),
            )
        }
    }

    fn unbound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &wdl_ast::v1::UnboundDecl,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Type::Object(ty) = decl.ty() {
            diagnostics.exceptable_add(
                deprecated_object_use(ty.span()),
                SyntaxElement::from(decl.inner().clone()),
                &self.exceptable_nodes(),
            )
        }
    }
}
