//! A lint rule for missing runtime sections.

use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::Visitor;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Diagnostics;
use wdl_ast::Span;
use wdl_ast::VisitReason;

use super::Rule;
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
#[derive(Debug, Clone, Copy)]
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

    fn visitor(&self) -> Box<dyn Visitor<State = Diagnostics>> {
        Box::new(MissingRuntimeVisitor)
    }
}

/// Implements the visitor for the missing runtime section rule.
struct MissingRuntimeVisitor;

impl Visitor for MissingRuntimeVisitor {
    type State = Diagnostics;

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
