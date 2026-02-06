//! A lint rule for flagging redundant input assignments

use std::fmt::Debug;

use wdl_analysis::Diagnostics;
use wdl_analysis::Document;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxElement;
use wdl_ast::v1::CallStatement;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the Redundant Input Assignment rule.
const ID: &str = "ConciseInput";

/// Create a "Redundant Input Assignment" diagnostic.
fn redundant_input_assignment(span: Span, name: &str) -> Diagnostic {
    Diagnostic::note("redundant input assignment")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix(format!("can be shortened to `{name}`"))
}

/// Detects a redundant input assignment.
#[derive(Default, Debug, Clone, Copy)]
pub struct ConciseInputRule(Option<SupportedVersion>);

impl Rule for ConciseInputRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures concise input assignments are used (implicit binding when available)."
    }

    fn explanation(&self) -> &'static str {
        "Redundant input assignments can be shortened in WDL versions >=v1.1 with an implicit \
         binding. For example, `{ input: a = a }` can be shortened to `{ input: a }`."
    }

    fn examples(&self) -> &'static [&'static str] {
        &[
            r#"```wdl
version 1.2

workflow hello {
    input {
        String name
    }

    # Since WDL v1.1, these explicit bindings can be shortened.
    call say_hello {
        name = name,
    }
}

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>
}
```"#,
            r#"Use instead:

```wdl
version 1.2

workflow hello {
    input {
        String name
    }

    # `name` can be passed in directly
    call say_hello {
        name,
    }
}

task say_hello {
    input {
        String name
    }

    command <<<
        echo "Hello, ~{name}!"
    >>>
}
```"#,
        ]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Style])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[
            wdl_ast::SyntaxKind::VersionStatementNode,
            wdl_ast::SyntaxKind::WorkflowDefinitionNode,
            wdl_ast::SyntaxKind::CallStatementNode,
            wdl_ast::SyntaxKind::CallInputItemNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for ConciseInputRule {
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

        self.0 = Some(version);
    }

    fn call_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &CallStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let SupportedVersion::V1(minor_version) = self.0.expect("version should exist here") {
            if minor_version < wdl_ast::version::V1::One {
                return;
            }
            stmt.inputs().for_each(|input| {
                if let Some(expr) = input.expr()
                    && let Some(expr_name) = expr.as_name_ref()
                    && expr_name.name().text() == input.name().text()
                {
                    diagnostics.exceptable_add(
                        redundant_input_assignment(input.span(), input.name().text()),
                        SyntaxElement::from(input.inner().clone()),
                        &self.exceptable_nodes(),
                    );
                }
            });
        }
    }
}
