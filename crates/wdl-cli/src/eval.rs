//! Facilities for performing a typical WDL evaluation using the `wdl-*` crates.

use anyhow::anyhow;
use tokio_util::sync::CancellationToken;
use wdl_analysis::Document;
use wdl_engine::EvaluatedTask;
use wdl_engine::EvaluationError;
use wdl_engine::EvaluationResult;
use wdl_engine::Events;
use wdl_engine::Inputs;
use wdl_engine::Outputs;
use wdl_engine::config::Config;
use wdl_engine::v1::TaskEvaluator;
use wdl_engine::v1::WorkflowEvaluator;

use crate::inputs::OriginPaths;

/// An evaluator for a WDL task or workflow.
// TODO ACF 2025-10-10: this seems like a good start for an `Engine` state type
// within `wdl_engine` to hold things like the apptainer image cache.
pub struct Evaluator<'a> {
    /// The document that contains the task or workflow to run.
    document: &'a Document,

    /// The name of the task or workflow to run.
    name: &'a str,

    /// The inputs to the task or workflow.
    inputs: Inputs,

    /// The origin paths for the input keys.
    origins: OriginPaths,

    /// The configuration for the WDL engine.
    config: Config,
}

impl<'a> Evaluator<'a> {
    /// Creates a new task or workflow evaluator.
    pub fn new(
        document: &'a Document,
        name: &'a str,
        inputs: Inputs,
        origins: OriginPaths,
        config: Config,
    ) -> Self {
        Self {
            document,
            name,
            inputs,
            origins,
            config,
        }
    }

    /// Runs a WDL task or workflow evaluation.
    pub async fn run(
        mut self,
        token: CancellationToken,
        events: Events,
    ) -> EvaluationResult<Outputs> {
        self.config.validate().await?;
        match self.inputs {
            Inputs::Task(ref mut inputs) => {
                let task = self.document.task_by_name(self.name).ok_or_else(|| {
                    anyhow!(
                        "document does not contain a task named `{name}`",
                        name = self.name
                    )
                })?;

                // Ensure all the paths specified in the inputs are relative to
                // their respective origin paths.
                inputs.join_paths(task, |key| {
                    self.origins
                        .get(key)
                        .ok_or(anyhow!("unable to find origin path for key `{key}`"))
                })?;

                let initial_output_dir = self
                    .config
                    .output_dir
                    .clone()
                    .expect("valid Config must contain output_dir");
                let evaluator = TaskEvaluator::new(self.config, token, events).await?;
                evaluator
                    .evaluate(self.document, task, inputs, &initial_output_dir)
                    .await
                    .and_then(EvaluatedTask::into_result)
            }
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
                inputs.join_paths(workflow, |key| {
                    self.origins
                        .get(key)
                        .ok_or(anyhow!("unable to find origin path for key `{key}`"))
                })?;

                let initial_output_dir = self
                    .config
                    .output_dir
                    .clone()
                    .expect("valid Config must contain output_dir");
                let evaluator = WorkflowEvaluator::new(self.config, token, events).await?;
                evaluator
                    .evaluate(self.document, inputs, &initial_output_dir)
                    .await
            }
        }
    }
}
