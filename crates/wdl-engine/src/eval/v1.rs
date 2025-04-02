//! Implementation of evaluation for V1 documents.

mod expr;
mod task;
mod workflow;

use anyhow::Result;
pub use expr::*;
pub use task::*;
pub use workflow::*;

use super::EvaluatedTask;
use super::EvaluationResult;
use crate::Outputs;
use crate::TaskExecutionResult;

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
        /// The result from the task's execution.
        ///
        /// This may be `Err` if the task failed to complete.
        result: &'a Result<TaskExecutionResult>,
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
