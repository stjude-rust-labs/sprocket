//! Facilities for performing a typical WDL evaluation using the `wdl-*` crates.

use std::path::Path;

use anyhow::anyhow;
use wdl::analysis::Document;
use wdl::engine::CancellationContext;
use wdl::engine::Engine;
use wdl::engine::EvaluatedTask;
use wdl::engine::EvaluationError;
use wdl::engine::EvaluationPath;
use wdl::engine::EvaluationResult;
use wdl::engine::Events;
use wdl::engine::Inputs;
use wdl::engine::Outputs;

/// An evaluator for a WDL task or workflow.
pub struct Evaluator<'a> {
    /// The document that contains the task or workflow to run.
    document: &'a Document,
    /// The name of the task or workflow to run.
    name: &'a str,
    /// The inputs to the task or workflow.
    inputs: Inputs,
    /// The base directory to join for relative input paths.
    base_dir: &'a EvaluationPath,
    /// The WDL evaluation engine to use.
    engine: &'a Engine,
    /// The output directory.
    output_dir: &'a Path,
}

impl<'a> Evaluator<'a> {
    /// Creates a new task or workflow evaluator.
    pub fn new(
        document: &'a Document,
        name: &'a str,
        inputs: Inputs,
        base_dir: &'a EvaluationPath,
        engine: &'a Engine,
        output_dir: &'a Path,
    ) -> Self {
        Self {
            document,
            name,
            inputs,
            base_dir,
            engine,
            output_dir,
        }
    }

    /// Is this evaluator evaluating a workflow?
    pub fn is_workflow(&self) -> bool {
        matches!(self.inputs, Inputs::Workflow(_))
    }

    /// Evaluate a task, returning [`Result<EvaluatedTask, EvaluationError>`].
    ///
    /// # Panics
    ///
    /// Panics if the evaluator is for a workflow.
    pub async fn evaluate_task(
        self,
        events: Events,
        cancellation: CancellationContext,
    ) -> Result<EvaluatedTask, EvaluationError> {
        if self.is_workflow() {
            panic!(
                "cannot evaluate workflow `{name}` as a task",
                name = self.name
            );
        }
        let mut inputs = self.inputs.unwrap_task_inputs();
        let task = self.document.task_by_name(self.name).ok_or_else(|| {
            anyhow!(
                "document does not contain a task named `{name}`",
                name = self.name
            )
        })?;

        // Ensure all the paths specified in the inputs are relative to
        // their respective origin paths.
        inputs
            .join_paths(task, |_| Ok(std::slice::from_ref(self.base_dir)))
            .await?;

        self.engine
            .create_v1_evaluator(events, cancellation)
            .evaluate_task(self.document, task, inputs, self.output_dir)
            .await
    }

    /// Runs a WDL task or workflow evaluation.
    pub async fn run(
        self,
        events: Events,
        cancellation: CancellationContext,
    ) -> EvaluationResult<Outputs> {
        match self.inputs {
            Inputs::Task(_) => self
                .evaluate_task(events, cancellation)
                .await
                .and_then(EvaluatedTask::into_outputs),
            Inputs::Workflow(mut inputs) => {
                let workflow = self
                    .document
                    .workflow()
                    .ok_or_else(|| anyhow!("document does not contain a workflow"))?;

                if workflow.name() != self.name {
                    return Err(EvaluationError::Other(anyhow!(
                        "document does not contain a workflow named `{name}`",
                        name = self.name
                    )));
                }

                // Ensure all the paths specified in the inputs are relative to
                // their respective origin paths.
                inputs
                    .join_paths(workflow, |_| Ok(std::slice::from_ref(self.base_dir)))
                    .await?;

                self.engine
                    .create_v1_evaluator(events, cancellation)
                    .evaluate_workflow(self.document, inputs, self.output_dir)
                    .await
            }
        }
    }
}
