//! Language server protocol handlers.

use wdl_ast::Span;

use crate::DiagnosticsConfig;
use crate::Document;
use crate::diagnostics;
use crate::document::ScopeRef;
use crate::types::v1::EvaluationContext;

mod common;
mod completions;
mod document_symbol;
mod find_all_references;
mod goto_definition;
mod hover;
mod rename;
mod semantic_tokens;
pub(crate) mod snippets;

pub use completions::*;
pub use document_symbol::*;
pub use find_all_references::*;
pub use goto_definition::*;
pub use hover::*;
pub use rename::*;
pub use semantic_tokens::*;

/// Context for evaluating expression types during LSP operations.
///
/// This struct provides the necessary context for type evaluation when
/// processing LSP requests like completions, goto definition, and hover
/// information.
///
/// The context is specifically designed for LSP handlers where:
/// - We need to evaluate expression types at a specific cursor position
/// - We want to avoid collecting diagnostics (since they're handled separately)
/// - We only need read-only access to scope and document information
/// - We're typically evaluating single expressions rather than entire documents
#[derive(Debug)]
pub struct TypeEvalContext<'a> {
    /// The scope reference containing the variable and name bindings at the
    /// current position.
    scope: ScopeRef<'a>,
    /// The document being analyzed.
    document: &'a Document,
}

impl EvaluationContext for TypeEvalContext<'_> {
    fn version(&self) -> wdl_ast::SupportedVersion {
        self.document
            .version()
            .expect("document should have a version")
    }

    fn resolve_name(&self, name: &str, _span: Span) -> Option<crate::types::Type> {
        self.scope.lookup(name).map(|n| n.ty().clone())
    }

    fn resolve_type_name(
        &mut self,
        name: &str,
        span: Span,
    ) -> std::result::Result<crate::types::Type, wdl_ast::Diagnostic> {
        if let Some(s) = self.document.struct_by_name(name)
            && let Some(ty) = s.ty()
        {
            return Ok(ty.clone());
        }
        Err(diagnostics::unknown_type(name, span))
    }

    /// Returns `None` because LSP type evaluation doesn't occur within a
    /// specific task context.
    ///
    /// The task-specific
    /// information is provided through the scope reference instead.
    fn task(&self) -> Option<&crate::document::Task> {
        None
    }

    /// LSP handlers typically don't need custom diagnostics configuration since
    /// they focus on providing information rather than validation.
    fn diagnostics_config(&self) -> DiagnosticsConfig {
        DiagnosticsConfig::default()
    }

    /// LSP handlers are primarily concerned with extracting type information.
    /// Diagnostics are collected and reported through separate mechanisms,
    /// so we don't need to accumulate them during expression evaluation.
    fn add_diagnostic(&mut self, _: wdl_ast::Diagnostic) {}
}
