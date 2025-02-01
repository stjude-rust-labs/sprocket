//! Implementation of evaluation for V1 documents.

mod expr;
mod task;
mod workflow;

use anyhow::Result;
pub use expr::*;
use rowan::ast::AstPtr;
pub use task::*;
use wdl_analysis::document::Document;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::Decl;
use wdl_ast::v1::UnboundDecl;
pub use workflow::*;

use super::EvaluatedTask;
use super::EvaluationResult;
use crate::Outputs;

/// Represents a pointer to a declaration node.
///
/// This type is cheaply cloned.
#[derive(Debug, Clone)]
enum DeclPtr {
    /// The declaration is bound.
    Bound(AstPtr<BoundDecl>),
    /// The declaration is unbound.
    Unbound(AstPtr<UnboundDecl>),
}

impl DeclPtr {
    /// Constructs a new pointer to a declaration node given the declaration
    /// node.
    fn new(decl: &Decl) -> Self {
        match decl {
            Decl::Bound(decl) => Self::Bound(AstPtr::new(decl)),
            Decl::Unbound(decl) => Self::Unbound(AstPtr::new(decl)),
        }
    }

    /// Converts the pointer back to the declaration node.
    fn to_node(&self, document: &Document) -> Decl {
        match self {
            Self::Bound(decl) => Decl::Bound(decl.to_node(document.node().syntax())),
            Self::Unbound(decl) => Decl::Unbound(decl.to_node(document.node().syntax())),
        }
    }
}

/// Represents the kind of progress made during evaluation.
#[derive(Debug, Clone, Copy)]
pub enum ProgressKind<'a> {
    /// A task with the given id has started evaluation.
    TaskStarted {
        /// The identifier of the task.
        id: &'a str,
    },
    /// A task with the given id has started execution.
    ///
    /// Note that a task may have multiple executions as a result of retrying
    /// failed executions.
    TaskExecutionStarted {
        /// The identifier of the task.
        id: &'a str,
        /// The attempt number for the task's execution, starting at 0 to
        /// indicate the first attempt.
        ///
        /// This value is incremented upon each retry.
        attempt: u64,
    },
    /// A task with the given id has completed execution.
    TaskExecutionCompleted {
        /// The identifier of the task.
        id: &'a str,
        /// The status code from the task's execution.
        ///
        /// This may be `Err` if the task failed to spawn.
        status_code: &'a Result<i32>,
    },
    /// A task with the given id has completed evaluation.
    TaskCompleted {
        /// The identifier of the task.
        id: &'a str,
        /// The result of task evaluation.
        result: &'a EvaluationResult<EvaluatedTask>,
    },
    /// A workflow with the given id has started evaluation.
    WorkflowStarted {
        /// The identifier of the workflow.
        id: &'a str,
    },
    /// A workflow with the given id has completed evaluation.
    WorkflowCompleted {
        /// The identifier of the workflow.
        id: &'a str,
        /// The result of workflow evaluation.
        result: &'a EvaluationResult<Outputs>,
    },
}

impl ProgressKind<'_> {
    /// Gets the id of the task or workflow.
    pub fn id(&self) -> &str {
        match self {
            Self::TaskStarted { id, .. }
            | Self::TaskExecutionStarted { id, .. }
            | Self::TaskExecutionCompleted { id, .. }
            | Self::TaskCompleted { id, .. }
            | Self::WorkflowStarted { id, .. }
            | Self::WorkflowCompleted { id, .. } => id,
        }
    }
}
