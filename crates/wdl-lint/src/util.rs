//! A module for utility functions for the lint rules.

use std::process::Command;
use std::process::Stdio;

use wdl_analysis::DiagnosticsConfig;
use wdl_analysis::Document;
use wdl_analysis::Exceptable;
use wdl_analysis::diagnostics::unknown_type;
use wdl_analysis::document::ScopeRef;
use wdl_analysis::document::Task;
use wdl_analysis::types::Type;
use wdl_analysis::types::TypeNameRef;
use wdl_analysis::types::v1::EvaluationContext;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::TreeNode;

/// Check whether or not a program exists.
///
/// On unix-like OSes, uses `which`.
/// On Windows, uses `where.exe`.
pub fn program_exists(exec: &str) -> bool {
    let finder = if cfg!(windows) { "where.exe" } else { "which" };
    Command::new(finder)
        .arg(exec)
        .stdout(Stdio::null())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|r| r.success())
}

/// Serializes a list of items using the Oxford comma.
pub fn serialize_oxford_comma<T: std::fmt::Display>(items: &[T]) -> Option<String> {
    let len = items.len();

    match len {
        0 => None,
        // SAFETY: we just checked to ensure that exactly one element exists in
        // the `items` Vec, so this should always unwrap.
        1 => Some(items.iter().next().unwrap().to_string()),
        2 => {
            let mut items = items.iter();

            Some(format!(
                "{a} and {b}",
                // SAFETY: we just checked to ensure that exactly two elements
                // exist in the `items` Vec, so the first and second elements
                // will always be present.
                a = items.next().unwrap(),
                b = items.next().unwrap()
            ))
        }
        _ => {
            let mut result = String::new();

            for item in items.iter().take(len - 1) {
                if !result.is_empty() {
                    result.push_str(", ")
                }

                result.push_str(&item.to_string());
            }

            result.push_str(", and ");
            result.push_str(&items[len - 1].to_string());
            Some(result)
        }
    }
}

/// A context for evaluating expressions in a command section.
pub(crate) struct CommandContext<'a> {
    /// The document being linted.
    document: Document,
    /// The scope of the command section.
    scope: ScopeRef<'a>,
}

impl EvaluationContext for CommandContext<'_> {
    fn version(&self) -> SupportedVersion {
        self.document.version().expect("document has a version")
    }

    fn resolve_name(&mut self, name: &str, _span: Span) -> Option<Type> {
        // Check if there are any variables with this name and return if so.
        if let Some(var) = self.scope.lookup(name).map(|n| n.ty().clone()) {
            return Some(var);
        }

        if let Some(ty) = self.document.get_custom_type(name) {
            return Some(
                TypeNameRef::new(
                    name,
                    ty.as_custom()
                        .expect("type should be a custom type")
                        .clone(),
                )
                .into(),
            );
        }

        None
    }

    fn resolve_type_name(
        &mut self,
        name: &str,
        span: Span,
    ) -> std::result::Result<Type, Diagnostic> {
        self.scope
            .lookup(name)
            .map(|n| n.ty().clone())
            .ok_or_else(|| unknown_type(name, span))
    }

    fn task(&self) -> Option<&Task> {
        None
    }

    fn diagnostics_config(&self) -> DiagnosticsConfig {
        DiagnosticsConfig::except_all()
    }

    fn add_diagnostic(&mut self, _diagnostic: Diagnostic) {
        // do nothing
    }

    fn exceptable_add_diagnostic<N: TreeNode + Exceptable>(
        &mut self,
        _diagnostic: Diagnostic,
        _element: &N,
        _exceptable_nodes: &Option<&'static [SyntaxKind]>,
    ) {
        // do nothing
    }
}

impl<'a> CommandContext<'a> {
    /// Create a new `CommandContext`.
    pub(crate) fn new(document: Document, scope: ScopeRef<'a>) -> Self {
        Self { document, scope }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_program_exists() {
        if cfg!(windows) {
            assert!(program_exists("where.exe"));
        } else {
            assert!(program_exists("which"));
        }
    }

    #[test]
    fn test_itemize_oxford_comma() {
        assert_eq!(serialize_oxford_comma(&Vec::<String>::default()), None);
        assert_eq!(
            serialize_oxford_comma(&["hello"]),
            Some(String::from("hello"))
        );
        assert_eq!(
            serialize_oxford_comma(&["hello", "world"]),
            Some(String::from("hello and world"))
        );
        assert_eq!(
            serialize_oxford_comma(&["hello", "there", "world"]),
            Some(String::from("hello, there, and world"))
        );
    }
}
