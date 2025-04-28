//! Validation of imports.

use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::v1;
use wdl_ast::v1::StringPart;

use crate::Diagnostics;
use crate::VisitReason;
use crate::Visitor;

/// Creates an "empty import" diagnostic
fn empty_import(span: Span) -> Diagnostic {
    Diagnostic::error("import URI cannot be empty").with_highlight(span)
}

/// Creates a "placeholder in import" diagnostic
fn placeholder_in_import(span: Span) -> Diagnostic {
    Diagnostic::error("import URI cannot contain placeholders")
        .with_highlight(span)
        .with_fix("remove the placeholder")
}

/// Creates an "invalid import namespace" diagnostic
fn invalid_import_namespace(span: Span) -> Diagnostic {
    Diagnostic::error("import namespace is not a valid WDL identifier")
        .with_label("a namespace cannot be derived from this import path", span)
        .with_fix("add an `as` clause to the import to specify a namespace")
}

/// An AST visitor that ensures that imports are valid.
#[derive(Debug, Default)]
pub struct ImportsVisitor;

impl Visitor for ImportsVisitor {
    fn reset(&mut self) {
        *self = Default::default();
    }

    fn import_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &v1::ImportStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let uri = stmt.uri();
        if uri.is_empty() {
            diagnostics.add(empty_import(uri.span()));
            return;
        }

        if uri.text().is_none() {
            let span = uri
                .parts()
                .find_map(|p| match p {
                    StringPart::Text(_) => None,
                    StringPart::Placeholder(p) => Some(p.span()),
                })
                .expect("should have a placeholder span");

            diagnostics.add(placeholder_in_import(span));
            return;
        }

        if stmt.namespace().is_none() {
            diagnostics.add(invalid_import_namespace(uri.span()));
        }
    }
}
