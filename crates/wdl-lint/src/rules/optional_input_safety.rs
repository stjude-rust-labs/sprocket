//! Lint rule for optional values used in command placeholders without explicit
//! guards.

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_analysis::diagnostics::unknown_type;
use wdl_analysis::document::ScopeRef;
use wdl_analysis::types::Optional;
use wdl_analysis::types::Type;
use wdl_analysis::types::v1::EvaluationContext;
use wdl_analysis::types::v1::ExprTypeEvaluator;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::CallExpr;
use wdl_ast::v1::CommandPart;
use wdl_ast::v1::CommandSection;
use wdl_ast::v1::Expr;
use wdl_ast::v1::Placeholder;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the optional-input-safety lint rule.
const ID: &str = "OptionalInputSafety";

/// Standard library call names that count as explicit optional guards.
const GUARD_CALL_TARGETS: &[&str] = &["defined", "select_first", "select_all"];

/// Context for typing placeholder expressions inside a task command section.
struct CommandScopeContext<'a> {
    /// The analysis document being visited.
    document: Document,
    /// The lexical scope at the start of the command section.
    scope: ScopeRef<'a>,
}

impl EvaluationContext for CommandScopeContext<'_> {
    fn version(&self) -> SupportedVersion {
        self.document.version().expect("document has a version")
    }

    fn resolve_name(&self, name: &str, _span: Span) -> Option<Type> {
        if let Some(var) = self.scope.lookup(name).map(|n| n.ty().clone()) {
            return Some(var);
        }

        if let Some(ty) = self.document.get_custom_type(name) {
            return Some(
                ty.type_name_ref()
                    .expect("type name ref to be created from custom type"),
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

    fn task(&self) -> Option<&wdl_analysis::document::Task> {
        None
    }

    fn diagnostics_config(&self) -> wdl_analysis::DiagnosticsConfig {
        wdl_analysis::DiagnosticsConfig::except_all()
    }

    fn add_diagnostic(&mut self, _diagnostic: Diagnostic) {
        // Swallow type errors here; other phases report them.
    }
}

impl<'a> CommandScopeContext<'a> {
    /// Creates a new context for evaluating types in a command section.
    fn new(document: Document, scope: ScopeRef<'a>) -> Self {
        Self { document, scope }
    }
}

/// Returns `true` if the expression uses an optional value in a way that should
/// trigger a warning (e.g. bare optional or optional under `+` even when the
/// overall expression type is non-optional).
fn optional_use_is_unsafe<C: EvaluationContext>(
    evaluator: &mut ExprTypeEvaluator<'_, C>,
    expr: &Expr,
) -> bool {
    match expr {
        Expr::If(_) => false,
        Expr::Call(call) => {
            if guard_call(call) {
                false
            } else {
                call.arguments()
                    .any(|arg| optional_use_is_unsafe(evaluator, &arg))
                    || matches!(
                        evaluator.evaluate_expr(expr),
                        Some(ty) if ty.is_optional()
                    )
            }
        }
        Expr::Addition(a) => {
            let (left, right) = a.operands();
            optional_use_is_unsafe(evaluator, &left) || optional_use_is_unsafe(evaluator, &right)
        }
        Expr::Parenthesized(p) => optional_use_is_unsafe(evaluator, &p.expr()),
        _ => {
            let Some(ty) = evaluator.evaluate_expr(expr) else {
                return false;
            };
            ty.is_optional()
        }
    }
}

/// Returns whether the call is to `defined`, `select_first`, or `select_all`.
fn guard_call(call: &CallExpr) -> bool {
    let target = call.target();
    let name = target.text();
    GUARD_CALL_TARGETS.contains(&name)
}

/// Best-effort name of the placeholder subject for diagnostics (e.g. `x` in
/// `~{x}`).
fn placeholder_subject_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::NameRef(r) => Some(r.name().text().to_owned()),
        Expr::Parenthesized(p) => placeholder_subject_name(&p.expr()),
        _ => None,
    }
}

/// Builds the warning diagnostic for an unguarded optional in a placeholder.
fn optional_placeholder_diagnostic(expr: &Expr, placeholder: &Placeholder) -> Diagnostic {
    let span = placeholder.span();
    let message = if let Some(name) = placeholder_subject_name(expr) {
        format!(
            "optional value `{name}` used in command placeholder without a guard; use `if \
             defined()`, `select_first()`, `select_all()`, or an `if`/`else` expression to handle \
             the `None` case explicitly"
        )
    } else {
        String::from(
            "optional value used in command placeholder without a guard; use `if defined()`, \
             `select_first()`, `select_all()`, or an `if`/`else` expression to handle the `None` \
             case explicitly",
        )
    };

    Diagnostic::warning(message)
        .with_rule(ID)
        .with_highlight(span)
        .with_fix(
            "wrap the optional in `if defined(...) then ... else ...`, or use `select_first()` / \
             `select_all()` so `None` is handled explicitly",
        )
}

/// Flags optional-typed command placeholders that do not explicitly handle
/// `None`.
#[derive(Debug, Default, Clone)]
pub struct OptionalInputSafetyRule {
    /// The analysis document being visited.
    document: Option<Document>,
}

impl Rule for OptionalInputSafetyRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures optional values in command placeholders explicitly handle `None`."
    }

    fn explanation(&self) -> &'static str {
        "In command sections, `None` interpolates as an empty string, which is valid WDL but easy \
         to misuse (for example, producing an extra shell word or an empty quoted argument). \
         Requiring `if defined()`, `select_first()`, `select_all()`, or an `if`/`else` expression \
         makes intent obvious and avoids subtle command-line bugs."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Correctness])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::CommandSectionNode,
            SyntaxKind::PlaceholderNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &["DeprecatedPlaceholder", "ShellCheck"]
    }
}

impl Visitor for OptionalInputSafetyRule {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn document(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        document: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }
        self.document = Some(document.clone());
    }

    fn command_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &CommandSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let doc = match self.document.as_ref() {
            Some(d) => d.clone(),
            None => return,
        };

        let Some(scope) = doc.find_scope_by_position(section.inner().text_range().start().into())
        else {
            return;
        };

        let mut ctx = CommandScopeContext::new(doc.clone(), scope);
        let mut evaluator = ExprTypeEvaluator::new(&mut ctx);

        for part in section.parts() {
            let CommandPart::Placeholder(placeholder) = part else {
                continue;
            };
            let expr = placeholder.expr();
            if !optional_use_is_unsafe(&mut evaluator, &expr) {
                continue;
            }

            let diagnostic = optional_placeholder_diagnostic(&expr, &placeholder);
            diagnostics.exceptable_add(
                diagnostic,
                SyntaxElement::from(placeholder.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }
}
