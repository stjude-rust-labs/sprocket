//! A lint rule for encouraging the use of `struct`s over `Map`s.

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::Expr;
use wdl_ast::v1::PrimitiveTypeKind;
use wdl_ast::v1::Type;
use wdl_ast::v1::UnboundDecl;
use wdl_ast::version::V1;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the struct-over-map rule.
const ID: &str = "StructOverMap";

/// Creates a diagnostic for a `Map[String, ...]` type.
fn prefer_struct(value_type: Type, span: Span) -> Diagnostic {
    Diagnostic::note(format!(
        "usage of `Map[String, {}]`",
        value_type.inner().text()
    ))
    .with_rule(ID)
    .with_highlight(span)
    .with_help(format!(
        "consider using a `struct` instead of a `Map[String, {}]`",
        value_type.inner().text()
    ))
}

/// A lint rule for encouraging the use of `struct`s over `Map`s.
#[derive(Clone, Copy, Debug, Default)]
pub struct StructOverMap {
    /// The version of the current document.
    version: Option<SupportedVersion>,
}

impl Rule for StructOverMap {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Checks for usage of `Map[String, ...]` types."
    }

    fn explanation(&self) -> &'static str {
        "In cases where arbitrary key-value storage isn't necessary, `struct`s can be leveraged to \
         provide clearer semantics and better validation.\n\nOf course, there are many valid use \
         cases for `Map[String, ...]` (e.g. environment variables). Expect many false positives."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.3

task example {
    Map[String, Boolean] config = {
        "verbose": false,
        "dry_run": true,
    }

    command <<<
        cmd ~{if (config["verbose"])
            then "--verbose"
            else ""
        } ~{if (config["dry_run"])
            then "--dry-run"
            else ""
        }
    >>>
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: Some("If possible, consider switching to a `struct`"),
                snippet: r#"version 1.3

struct Config {
    Boolean verbose
    Boolean dry_run
}

task example {
    Config config = Config {
        verbose: false,
        dry_run: true,
    }

    command <<<
        cmd ~{if (config.verbose)
            then "--verbose"
            else ""
        } ~{if (config.dry_run)
            then "--dry-run"
            else ""
        }
    >>>
}
"#,
            }),
        }]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Clarity])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::StructDefinitionNode,
            SyntaxKind::TaskDefinitionNode,
            SyntaxKind::WorkflowDefinitionNode,
            SyntaxKind::BoundDeclNode,
            SyntaxKind::UnboundDeclNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &[]
    }
}

impl Visitor for StructOverMap {
    fn reset(&mut self) {
        *self = Self { version: None }
    }

    fn document(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        _: &Document,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.version = Some(version);
    }

    fn unbound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &UnboundDecl,
    ) {
        if reason != VisitReason::Enter
            || self
                .version
                .is_some_and(|v| v < SupportedVersion::V1(V1::One))
        {
            return;
        }

        lint_struct_type(decl.ty(), diagnostics, &self.exceptable_nodes());
    }

    fn bound_decl(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, decl: &BoundDecl) {
        if reason != VisitReason::Enter
            || self
                .version
                .is_some_and(|v| v < SupportedVersion::V1(V1::One))
        {
            return;
        }

        if check_expr_inherited(decl.expr()) {
            return;
        }

        lint_struct_type(decl.ty(), diagnostics, &self.exceptable_nodes());
    }
}

/// Whether the expression inherits its type from another declaration.
///
/// For example:
///
/// ```wdl
/// task foo {
///     Int age = 30
///
///     output {
///         Int final_age = age
///     }
/// }
/// ```
///
/// In this case, `final_age` is an `Int` *because* `age` is an `Int`.
fn check_expr_inherited(expr: Expr) -> bool {
    match expr {
        Expr::Literal(_) => false,
        Expr::Parenthesized(inner) => check_expr_inherited(inner.expr()),
        Expr::If(inner) => {
            let (_, then, else_) = inner.exprs();
            check_expr_inherited(then) && check_expr_inherited(else_)
        }

        // In any of these cases, the caller is either:
        //
        // 1. Referencing an external item that they don't control
        // 2. Referencing an item that either will be or already has produced a diagnostic itself
        //
        // In either case, we'll save them an unnecessary diagnostic.
        Expr::NameRef(_) | Expr::Index(_) | Expr::Access(_) | Expr::Call(_) => true,

        // Can never produce a `Map[String, ...]` anyway
        Expr::LogicalNot(_)
        | Expr::Negation(_)
        | Expr::LogicalOr(_)
        | Expr::LogicalAnd(_)
        | Expr::Equality(_)
        | Expr::Inequality(_)
        | Expr::Less(_)
        | Expr::LessEqual(_)
        | Expr::Greater(_)
        | Expr::GreaterEqual(_)
        | Expr::Addition(_)
        | Expr::Subtraction(_)
        | Expr::Multiplication(_)
        | Expr::Division(_)
        | Expr::Modulo(_)
        | Expr::Exponentiation(_) => true,
    }
}

/// Checks if a [`Type`] contains `Map[String, ...]`.
fn lint_struct_type(
    ty: Type,
    diagnostics: &mut Diagnostics,
    exceptable_nodes: &Option<&'static [SyntaxKind]>,
) {
    match ty {
        Type::Map(ty) => {
            let (key, value) = ty.types();
            if key.kind() == PrimitiveTypeKind::String {
                diagnostics.exceptable_add(
                    prefer_struct(value.clone(), ty.span()),
                    ty.inner(),
                    exceptable_nodes,
                );
            }

            lint_struct_type(value, diagnostics, exceptable_nodes);
        }
        Type::Array(ty) => lint_struct_type(ty.element_type(), diagnostics, exceptable_nodes),
        Type::Pair(ty) => {
            let (left, right) = ty.types();
            lint_struct_type(left, diagnostics, exceptable_nodes);
            lint_struct_type(right, diagnostics, exceptable_nodes);
        }
        _ => {}
    }
}
