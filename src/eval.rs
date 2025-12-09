//! Facilities for performing a typical WDL evaluation using the `wdl-*` crates.

use std::path::Path;

use anyhow::anyhow;
use wdl::analysis::Document;
use wdl::engine::CancellationContext;
use wdl::engine::EvaluatedTask;
use wdl::engine::EvaluationError;
use wdl::engine::EvaluationResult;
use wdl::engine::Events;
use wdl::engine::Inputs;
use wdl::engine::Outputs;
use wdl::engine::config::Config;
use wdl::engine::v1::TopLevelEvaluator;

use crate::inputs::OriginPaths;

/// An evaluator for a WDL task or workflow.
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

    /// The output directory.
    output_dir: &'a Path,
}

impl<'a> Evaluator<'a> {
    /// Creates a new task or workflow evaluator.
    pub fn new(
        document: &'a Document,
        name: &'a str,
        inputs: Inputs,
        origins: OriginPaths,
        config: Config,
        output_dir: &'a Path,
    ) -> Self {
        Self {
            document,
            name,
            inputs,
            origins,
            config,
            output_dir,
        }
    }

    /// Runs a WDL task or workflow evaluation.
    pub async fn run(
        mut self,
        cancellation: CancellationContext,
        events: &Events,
    ) -> EvaluationResult<Outputs> {
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
                inputs
                    .join_paths(task, |key| {
                        self.origins
                            .get(key)
                            .ok_or(anyhow!("unable to find origin path for key `{key}`"))
                    })
                    .await?;

                let evaluator =
                    TopLevelEvaluator::new(self.output_dir, self.config, cancellation, events)
                        .await?;
                evaluator
                    .evaluate_task(self.document, task, inputs, self.output_dir)
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
                inputs
                    .join_paths(workflow, |key| {
                        self.origins
                            .get(key)
                            .ok_or(anyhow!("unable to find origin path for key `{key}`"))
                    })
                    .await?;

                let evaluator =
                    TopLevelEvaluator::new(self.output_dir, self.config, cancellation, events)
                        .await?;
                evaluator
                    .evaluate_workflow(self.document, inputs, self.output_dir)
                    .await
            }
        }
    }
}
