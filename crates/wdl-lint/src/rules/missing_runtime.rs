//! A lint rule for missing runtime sections.

use wdl_ast::v1::TaskDefinition;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Document;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::VisitReason;
use wdl_ast::Visitor;

use crate::Rule;
use crate::Tag;
use crate::TagSet;

/// The identifier for the missing runtime rule.
const ID: &str = "MissingRuntime";

/// Creates a "missing runtime section" diagnostic.
fn missing_runtime_section(task: &str, span: Span) -> Diagnostic {
    Diagnostic::warning(format!("task `{task}` is missing a runtime section"))
        .with_rule(ID)
        .with_label("this task is missing a runtime section", span)
        .with_fix("add a runtime section to the task")
}

/// Detects missing `runtime` section for tasks.
#[derive(Default, Debug, Clone, Copy)]
pub struct MissingRuntimeRule;

impl Rule for MissingRuntimeRule {
    fn id(&self) -> &'static str {
        ID
    }

    fn description(&self) -> &'static str {
        "Ensures that tasks have a runtime section."
    }

    fn explanation(&self) -> &'static str {
        "Tasks that don't declare runtime sections are unlikely to be portable."
    }

    fn tags(&self) -> TagSet {
        TagSet::new(&[Tag::Completeness, Tag::Portability])
    }
}

impl Visitor for MissingRuntimeRule {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor upon document entry
        *self = Default::default();
    }

    fn task_definition(
        &mut self,
        state: &mut Self::State,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if task.runtimes().next().is_none() {
            let name = task.name();
            state.add(missing_runtime_section(name.as_str(), name.span()));
        }
    }
}
