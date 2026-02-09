//! A lint rule for import placements.

use wdl_analysis::Diagnostics;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::ImportStatement;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WorkflowDefinition;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the import placement rule.
const ID: &str = "ImportPlacement";

/// Creates a "misplaced import" diagnostic.
fn misplaced_import(span: Span) -> Diagnostic {
    Diagnostic::warning("misplaced import")
        .with_rule(ID)
        .with_highlight(span)
        .with_fix(
            "move this import so that it comes after the version statement but before any \
             document items",
        )
}

/// Detects incorrect import placements.
#[derive(Default, Debug, Clone, Copy)]
pub struct ImportPlacementRule {
    /// Whether or not an import statement is considered invalid.
    invalid: bool,
}

impl Rule for ImportPlacementRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn version(&self) -> &'static str {
        "0.3.0"
    }

    fn description(&self) -> &'static str {
        "Ensures that imports are placed between the version statement and any document items."
    }

    fn explanation(&self) -> &'static str {
        "All import statements should follow the WDL version declaration with one empty line \
         between the version and the first import statement."
    }

    fn examples(&self) -> &'static [&'static str] {
        &[
            r#"```wdl
version 1.2

workflow example {
    meta {}

    output {}
}

import "example2.wdl"
```"#,
            r#"Use instead:

```wdl
version 1.2

import "example2.wdl"

workflow example {
    meta {}

    output {}
}
```"#,
        ]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Clarity])
    }

    fn exceptable_nodes(&self) -> Option<&'static [wdl_ast::SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::ImportStatementNode,
        ])
    }

    fn related_rules(&self) -> &[&'static str] {
        &[]
    }
}

impl Visitor for ImportPlacementRule {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn import_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &ImportStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if self.invalid {
            diagnostics.exceptable_add(
                misplaced_import(stmt.span()),
                SyntaxElement::from(stmt.inner().clone()),
                &self.exceptable_nodes(),
            );
        }
    }

    fn struct_definition(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        _: &StructDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Saw an item other than an import, imports are no longer valid
        self.invalid = true;
    }

    fn task_definition(&mut self, _: &mut Diagnostics, reason: VisitReason, _: &TaskDefinition) {
        if reason == VisitReason::Exit {
            return;
        }

        // Saw an item other than an import, imports are no longer valid
        self.invalid = true;
    }

    fn workflow_definition(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        _: &WorkflowDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Saw an item other than an import, imports are no longer valid
        self.invalid = true;
    }
}
