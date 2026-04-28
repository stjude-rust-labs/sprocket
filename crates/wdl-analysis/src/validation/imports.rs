//! Validation of imports.

use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::v1;
use wdl_ast::v1::StringPart;
use wdl_ast::version::V1;

use crate::Config;
use crate::Diagnostics;
use crate::VisitReason;
use crate::Visitor;
use crate::document::Document as AnalysisDocument;

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

/// Creates a "symbolic module resolution not yet implemented" diagnostic.
fn symbolic_resolution_not_implemented(span: Span) -> Diagnostic {
    Diagnostic::error("symbolic module resolution is not yet implemented").with_label(
        "this symbolic module path cannot be resolved by the current Sprocket release",
        span,
    )
}

/// An AST visitor that ensures that imports are valid.
#[derive(Debug, Default)]
pub struct ImportsVisitor {
    /// Whether WDL 1.4 has been opted into via `feature_flags.wdl_1_4`.
    wdl_1_4_enabled: bool,
    /// The version of the document currently being visited.
    version: Option<SupportedVersion>,
}

impl Visitor for ImportsVisitor {
    fn register(&mut self, config: &Config) {
        self.wdl_1_4_enabled = config.feature_flags().wdl_1_4();
    }

    fn reset(&mut self) {
        let wdl_1_4_enabled = self.wdl_1_4_enabled;
        *self = Default::default();
        self.wdl_1_4_enabled = wdl_1_4_enabled;
    }

    fn document(
        &mut self,
        _diagnostics: &mut Diagnostics,
        reason: VisitReason,
        _doc: &AnalysisDocument,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Enter {
            self.version = Some(version);
        }
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

        let uri = match stmt.source() {
            v1::ImportSource::Uri(uri) => uri,
            v1::ImportSource::ModulePath(path) => {
                if matches!(self.version, Some(SupportedVersion::V1(V1::Four)))
                    && self.wdl_1_4_enabled
                {
                    diagnostics.add(symbolic_resolution_not_implemented(path.span()));
                }
                return;
            }
        };

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

        // Form 1 is the only form that introduces a namespace; the wildcard
        // and member-selection forms bring items directly into scope and
        // legitimately have no namespace, so the check is skipped for them.
        if stmt.form() == v1::ImportForm::Namespace && stmt.namespace().is_none() {
            diagnostics.add(invalid_import_namespace(uri.span()));
        }
    }
}
