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

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The `HostPathLiterals` rule ID.
const ID: &str = "HostPathLiterals";

/// Returns `true` when `s` looks like an absolute host path on any platform we
/// care about: POSIX (`/foo`), Windows UNC (`\\server\share`), or Windows
/// drive-letter (`C:\foo` or `C:/foo`).
fn is_absolute_host_path(s: &str) -> bool {
    if s.starts_with('/') || s.starts_with(r"\\") {
        return true;
    }

    let bytes = s.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

/// Creates a diagnostic for a `File`/`Directory` declaration whose default is
/// an absolute host path.
fn absolute_host_path_default(span: Span, decl_name: &str) -> Diagnostic {
    Diagnostic::note(format!("`{decl_name}` has an absolute host path"))
        .with_rule(ID)
        .with_highlight(span)
        .with_help(
            "absolute paths outside of `output` sections will resolve on the host filesystem; \
             this prevents the task from being portable across execution environments",
        )
}

/// Flags `File`/`Directory` declaration defaults that use absolute host paths.
#[derive(Copy, Clone, Debug, Default)]
pub struct HostPathLiteralsRule {
    /// Whether the current declaration is inside an `output` section.
    output_section: bool,
}

impl Rule for HostPathLiteralsRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Flags `File`/`Directory` declaration defaults that use absolute host paths."
    }

    fn explanation(&self) -> &'static str {
        "`File` and `Directory` declarations with absolute path defaults are not portable across \
         environments. Use relative paths or supply values at runtime."
    }

    fn examples(&self) -> &'static [&'static str] {
        &[r#"```wdl
version 1.3

task run_tool {
    input {
        File data = "/etc/host/input.txt"
    }

    command <<<
        echo "run"
    >>>
}
```"#]
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

    fn related_rules(&self) -> &'static [&'static str] {
        &[]
    }
}

impl Visitor for HostPathLiteralsRule {
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

        if !matches!(
            decl.ty(),
            wdl_ast::v1::Type::Primitive(t)
                if matches!(
                    t.kind(),
                    PrimitiveTypeKind::File | PrimitiveTypeKind::Directory
                )
        ) {
            return;
        }

        // NOTE: `s.text()` returns `None` for interpolated strings (those
        // containing placeholders), so interpolated defaults are intentionally
        // skipped — we can only reason about absolute paths when the literal is
        // a single, static text piece.
        let expr = decl.expr();
        if let Expr::Literal(LiteralExpr::String(s)) = expr
            && let Some(text) = s.text()
            && is_absolute_host_path(text.text())
        {
            diagnostics.exceptable_add(
                absolute_host_path_default(s.span(), decl.name().text()),
                SyntaxElement::from(decl.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_posix_absolute() {
        assert!(is_absolute_host_path("/etc/host/input.txt"));
        assert!(is_absolute_host_path("/"));
    }

    #[test]
    fn detects_windows_drive_absolute() {
        assert!(is_absolute_host_path(r"C:\host\input.txt"));
        assert!(is_absolute_host_path("C:/host/input.txt"));
        assert!(is_absolute_host_path(r"z:\lower"));
    }

    #[test]
    fn detects_unc_absolute() {
        assert!(is_absolute_host_path(r"\\server\share\input.txt"));
    }

    #[test]
    fn rejects_relative() {
        assert!(!is_absolute_host_path("data/input.txt"));
        assert!(!is_absolute_host_path("./input.txt"));
        assert!(!is_absolute_host_path("input.txt"));
        assert!(!is_absolute_host_path(""));
        assert!(!is_absolute_host_path("C:"));
        assert!(!is_absolute_host_path("C:relative"));
        assert!(!is_absolute_host_path("1:\\digit-not-letter"));
    }
}
