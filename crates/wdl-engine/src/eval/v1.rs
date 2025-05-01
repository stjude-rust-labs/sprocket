//! Implementation of evaluation for V1 documents.

mod expr;
mod task;
mod workflow;

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
pub use expr::*;
use serde::Serialize;
pub use task::*;
pub use workflow::*;

use super::EvaluatedTask;
use super::EvaluationResult;
use crate::Outputs;
use crate::TaskExecutionResult;

/// The name of the inputs file to write for each task and workflow in the
/// outputs directory.
const INPUTS_FILE: &str = "inputs.json";

/// The name of the outputs file to write for each task and workflow in the
/// outputs directory.
const OUTPUTS_FILE: &str = "outputs.json";

/// Represents the kind of progress made during evaluation.
#[derive(Debug, Clone, Copy)]
pub enum ProgressKind<'a> {
    /// A task with the given id has started evaluation.
    TaskStarted {
        /// The identifier of the task.
        id: &'a str,
    },
    /// A task has been retried.
    TaskRetried {
        /// The identifier of the task.
        id: &'a str,
        /// The retry number for the task's execution, starting at 0 to indicate
        /// first retry.
        ///
        /// This value is incremented upon each retry.
        retry: u64,
    },
    /// A task with the given id has started execution.
    ///
    /// Note that a task may have multiple executions as a result of retrying
    /// failed executions.
    TaskExecutionStarted {
        /// The identifier of the task.
        id: &'a str,
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
            | Self::TaskRetried { id, .. }
            | Self::TaskExecutionStarted { id, .. }
            | Self::TaskExecutionCompleted { id, .. }
            | Self::TaskCompleted { id, .. }
            | Self::WorkflowStarted { id, .. }
            | Self::WorkflowCompleted { id, .. } => id,
        }
    }
}

/// Serializes a value into a JSON file.
fn write_json_file(path: impl AsRef<Path>, value: &impl Serialize) -> Result<()> {
    let path = path.as_ref();
    let file = File::create(path)
        .with_context(|| format!("failed to create file `{path}`", path = path.display()))?;
    serde_json::to_writer_pretty(BufWriter::new(file), value)
        .with_context(|| format!("failed to write file `{path}`", path = path.display()))
}
