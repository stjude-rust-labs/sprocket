//! A lint rule for empty/missing `output` sections.

use wdl_analysis::Diagnostics;
use wdl_analysis::Example;
use wdl_analysis::LabeledSnippet;
use wdl_analysis::VisitReason;
use wdl_analysis::Visitor;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::SyntaxElement;
use wdl_ast::SyntaxKind;
use wdl_ast::v1::TaskDefinition;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the empty outputs rule.
const ID: &str = "EmptyOutputs";

/// Creates a diagnostic for missing `output` sections.
fn missing_outputs(task: Ident) -> Diagnostic {
    Diagnostic::note(format!("task '{}' defines no outputs", task.text()))
        .with_rule(ID)
        .with_highlight(task.span())
}

/// Detects missing/empty `output` sections.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyOutputs;

impl Rule for EmptyOutputs {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures `task`s specify an `output` section."
    }

    fn explanation(&self) -> &'static str {
        "A task without an `output` section may be a mistake. This lint may be overzealous, as \
         there are some legitimate use cases for tasks without outputs (e.g. uploading to an \
         external service). In that case, authors should suppress the diagnostic on that specific \
         task, and document its behavior in the `meta` section."
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            negative: LabeledSnippet {
                label: None,
                snippet: r#"version 1.2

task generate_files {
    command <<<
        touch foo.txt
    >>>
}
"#,
            },
            revised: Some(LabeledSnippet {
                label: Some("If the results are intended to be received by the caller"),
                snippet: r#"version 1.2

task generate_files {
    command <<<
        touch foo.txt
    >>>

    output {
        File files = "foo.txt"
    }
}
"#,
            }),
        }]
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Completeness])
    }

    fn exceptable_nodes(&self) -> Option<&'static [SyntaxKind]> {
        Some(&[
            SyntaxKind::VersionStatementNode,
            SyntaxKind::TaskDefinitionNode,
        ])
    }

    fn related_rules(&self) -> &'static [&'static str] {
        &[]
    }
}

impl Visitor for EmptyOutputs {
    fn reset(&mut self) {
        *self = Self;
    }

    fn task_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        if reason != VisitReason::Enter {
            return;
        }

        if let Some(output) = task.output()
            && output.declarations().count() > 0
        {
            return;
        }

        diagnostics.exceptable_add(
            missing_outputs(task.name()),
            SyntaxElement::from(task.inner().clone()),
            &self.exceptable_nodes(),
        );
    }
}
