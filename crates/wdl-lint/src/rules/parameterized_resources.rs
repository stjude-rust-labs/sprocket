//! A lint rule to flag statically allocated task resources.

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::Expr;
use wdl_ast::v1::LiteralExpr;
use wdl_ast::v1::RequirementsItem;
use wdl_ast::v1::RuntimeItem;
use wdl_ast::v1::StringPart;
use wdl_ast::version::V1;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the parameterized resources rule.
const ID: &str = "ParameterizedResources";

/// The keys to check for static resource allocations.
const KEYS_TO_LINT: &[&str] = &["cpu", "memory", "disks"];

/// Creates a fixed resource allocation diagnostic.
fn fixed_resources(span: Span, version: SupportedVersion) -> Diagnostic {
    let help = if version < SupportedVersion::V1(V1::Two) {
        "consider moving requirements to user-controlled inputs"
    } else {
        "consider using input parameters or `task.attempt` for retry-aware scaling"
    };

    Diagnostic::note("fixed resource allocation")
        .with_rule(ID)
        .with_highlight(span)
        .with_help(help)
}

/// Checks that task resources are not statically allocated.
#[derive(Default, Debug, Clone, Copy)]
pub struct ParameterizedResourcesRule {
    /// The version of the WDL document being linted.
    version: Option<SupportedVersion>,
}

impl Rule for ParameterizedResourcesRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Checks that task resources are not statically allocated."
    }

    fn explanation(&self) -> &'static str {
        "To avoid issues related to resource allocation, dynamic (user-controlled and/or retry \
         scalable) values are encouraged in `requirements`/`runtime` sections.\n\nOf course, there \
         are many valid use cases for fixed resource allocation. Expect many false positives."
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                negative: LabeledSnippet {
                    label: None,
                    snippet: r#"version 1.3

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>

    requirements {
        cpu: 4
        memory: "8 GiB"
    }
}
"#,
                },
                revised: Some(LabeledSnippet {
                    label: Some("Consider moving requirements to user-controlled inputs"),
                    snippet: r#"version 1.3

task say_hello {
    input {
        String name
        Int cpu = 4
        String memory = "8 GiB"
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>

    requirements {
        cpu: cpu
        memory: memory
    }
}
"#,
                }),
            },
            Example {
                negative: LabeledSnippet {
                    label: Some("Or consider introducing retry-aware scaling"),
                    snippet: r#"version 1.3

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>

    requirements {
        memory: "8 GiB"
    }
}
"#,
                },
                revised: Some(LabeledSnippet {
                    label: None,
                    snippet: r#"version 1.3

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>

    requirements {
        memory: if task.attempt == 0 then "8 GiB" else "~{8 * (task.attempt + 1)} GiB"
        max_retries: 2
    }
}
"#,
                }),
            },
        ]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Portability])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::RequirementsItemNode,
            SyntaxKind::RuntimeItemNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &[]
    }
}

impl Visitor for ParameterizedResourcesRule {
    fn reset(&mut self) {
        *self = Default::default();
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

    fn requirements_item(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        item: &RequirementsItem,
    ) {
        if reason != VisitReason::Enter || !KEYS_TO_LINT.contains(&item.name().text()) {
            return;
        }

        if is_fixed_allocation(&item.expr()) {
            diagnostics.exceptable_add(
                fixed_resources(item.span(), self.version.unwrap()),
                item.inner(),
                &self.exceptable_nodes(),
            );
        }
    }

    fn runtime_item(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        item: &RuntimeItem,
    ) {
        if reason != VisitReason::Enter || !KEYS_TO_LINT.contains(&item.name().text()) {
            return;
        }

        if is_fixed_allocation(&item.expr()) {
            diagnostics.exceptable_add(
                fixed_resources(item.span(), self.version.unwrap()),
                item.inner(),
                &self.exceptable_nodes(),
            );
        }
    }
}

/// Checks if the resource is statically allocated.
fn is_fixed_allocation(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(lit) => match lit {
            LiteralExpr::Integer(_) | LiteralExpr::Float(_) | LiteralExpr::Boolean(_) => true,
            LiteralExpr::String(s) => s.parts().all(|part| match part {
                StringPart::Text(_) => true,
                StringPart::Placeholder(p) => is_fixed_allocation(&p.expr()),
            }),
            LiteralExpr::Array(arr) => arr.elements().all(|e| is_fixed_allocation(&e)),
            _ => false,
        },
        Expr::Parenthesized(p) => is_fixed_allocation(&p.expr()),
        Expr::If(i) => {
            let (cond, true_expr, false_expr) = i.exprs();
            is_fixed_allocation(&cond)
                && is_fixed_allocation(&true_expr)
                && is_fixed_allocation(&false_expr)
        }

        Expr::Addition(e) => {
            let (lhs, rhs) = e.operands();
            is_fixed_allocation(&lhs) && is_fixed_allocation(&rhs)
        }
        Expr::Subtraction(e) => {
            let (lhs, rhs) = e.operands();
            is_fixed_allocation(&lhs) && is_fixed_allocation(&rhs)
        }
        Expr::Multiplication(e) => {
            let (lhs, rhs) = e.operands();
            is_fixed_allocation(&lhs) && is_fixed_allocation(&rhs)
        }
        Expr::Division(e) => {
            let (lhs, rhs) = e.operands();
            is_fixed_allocation(&lhs) && is_fixed_allocation(&rhs)
        }
        Expr::Modulo(e) => {
            let (lhs, rhs) = e.operands();
            is_fixed_allocation(&lhs) && is_fixed_allocation(&rhs)
        }
        Expr::Exponentiation(e) => {
            let (lhs, rhs) = e.operands();
            is_fixed_allocation(&lhs) && is_fixed_allocation(&rhs)
        }
        Expr::LogicalOr(e) => {
            let (lhs, rhs) = e.operands();
            is_fixed_allocation(&lhs) && is_fixed_allocation(&rhs)
        }
        Expr::LogicalAnd(e) => {
            let (lhs, rhs) = e.operands();
            is_fixed_allocation(&lhs) && is_fixed_allocation(&rhs)
        }
        Expr::Negation(e) => is_fixed_allocation(&e.operand()),

        Expr::LogicalNot(_)
        | Expr::Equality(_)
        | Expr::Inequality(_)
        | Expr::Less(_)
        | Expr::LessEqual(_)
        | Expr::Greater(_)
        | Expr::GreaterEqual(_)
        | Expr::NameRef(_)
        | Expr::Call(_)
        | Expr::Index(_)
        | Expr::Access(_) => false,
    }
}
