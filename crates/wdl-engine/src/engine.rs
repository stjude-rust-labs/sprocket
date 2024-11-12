//! Implementation of the WDL evaluation engine.

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use wdl_analysis::document::Document;
use wdl_analysis::types::Types;

use crate::Outputs;
use crate::TaskInputs;
use crate::WorkflowInputs;

/// Represents a WDL evaluation engine.
#[derive(Debug, Default)]
pub struct Engine {
    /// The engine's type collection.
    pub(crate) types: Types,
}

impl Engine {
    /// Constructs a new WDL evaluation engine.
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets the engine's type collection.
    pub fn types(&self) -> &Types {
        &self.types
    }

    /// Gets a mutable reference to the engine's type collection.
    pub fn types_mut(&mut self) -> &mut Types {
        &mut self.types
    }

    /// Evaluates a workflow.
    ///
    /// Returns the workflow outputs upon success.
    pub async fn evaluate_workflow(
        &mut self,
        document: &Document,
        inputs: &WorkflowInputs,
    ) -> Result<Outputs> {
        let workflow = document
            .workflow()
            .ok_or_else(|| anyhow!("document does not contain a workflow"))?;
        inputs
            .validate(&mut self.types, document, workflow)
            .with_context(|| {
                format!(
                    "failed to validate the inputs to workflow `{workflow}`",
                    workflow = workflow.name()
                )
            })?;

        todo!("not yet implemented")
    }

    /// Evaluates a task with the given name.
    ///
    /// Returns the task outputs upon success.
    pub async fn evaluate_task(
        &mut self,
        document: &Document,
        name: &str,
        inputs: &TaskInputs,
    ) -> Result<Outputs> {
        let task = document
            .task_by_name(name)
            .ok_or_else(|| anyhow!("document does not contain a task named `{name}`"))?;
        inputs
            .validate(&mut self.types, document, task)
            .with_context(|| {
                format!(
                    "failed to validate the inputs to task `{task}`",
                    task = task.name()
                )
            })?;

        todo!("not yet implemented")
    }
}
