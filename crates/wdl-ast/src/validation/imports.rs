//! Validation of imports.

use crate::v1;
use crate::v1::StringPart;
use crate::AstNode;
use crate::Diagnostic;
use crate::Diagnostics;
use crate::Document;
use crate::Span;
use crate::SupportedVersion;
use crate::ToSpan;
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

        *self = Default::default();
    }

    fn import_statement(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        stmt: &v1::ImportStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let uri = stmt.uri();
        if uri.is_empty() {
            state.add(empty_import(uri.syntax().text_range().to_span()));
            return;
        }

        if uri.text().is_none() {
            let span = uri
                .parts()
                .find_map(|p| match p {
                    StringPart::Text(_) => None,
                    StringPart::Placeholder(p) => Some(p.syntax().text_range().to_span()),
                })
                .expect("should have a placeholder span");

            state.add(placeholder_in_import(span));
            return;
        }

        if stmt.namespace().is_none() {
            state.add(invalid_import_namespace(
                uri.syntax().text_range().to_span(),
            ));
        }
    }
}
