//! Execution of runs for provenance tracking in v1.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use chrono::DateTime;
use chrono::Utc;
use serde_json::Value as JsonValue;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::info;
use uuid::Uuid;
use wdl::analysis::AnalysisResult;
use wdl::analysis::Analyzer;
use wdl::analysis::Config as AnalysisConfig;
use wdl::analysis::Document as AnalysisDocument;
use wdl::ast::Severity;
use wdl::engine::CancellationContext;
use wdl::engine::Config as WdlConfig;
use wdl::engine::Events;
use wdl::engine::Inputs;
use wdl::engine::Outputs;
use wdl::engine::v1::Evaluator;

use crate::system::v1::db::Database;
use crate::system::v1::exec::svc::TaskMonitorSvc;
use crate::system::v1::fs::OutputDirectory;
use crate::system::v1::fs::RunDirectory;

pub mod config;
pub mod names;
pub mod source;
pub mod svc;

pub use config::ConfigError;
pub use config::ConfigResult;
pub use names::generate_run_name;
pub use source::AllowedSource;

/// An identified target to run.
#[derive(Clone, Debug)]
pub enum Target {
    /// A task with the provided name.
    Task(String),
    /// A workflow with the provided name.
    Workflow(String),
}

impl Target {
    /// Gets the name for the target by reference.
    pub fn name(&self) -> &str {
        match self {
            Target::Task(name) => name,
            Target::Workflow(name) => name,
        }
    }

    /// Consumes `self` and returns the inner target name.
    pub fn into_name(self) -> String {
        match self {
            Target::Task(name) => name,
            Target::Workflow(name) => name,
        }
    }
}

/// Error type for target selection.
#[derive(Debug, Error)]
pub enum SelectTargetError {
    /// Target not found.
    #[error("target not found: `{0}`")]
    TargetNotFound(String),
    /// No tasks or workflows in document.
    #[error("a target cannot be inferred because the document contains no tasks and no workflow")]
    NoExecutableTarget,
    /// No workflows and multiple tasks in document.
    #[error("a target cannot be inferred because the document contains multiple tasks and no workflow")]
    TargetRequired,
}

/// Select the target workflow or task to execute from the document.
///
/// The priority is set as follows:
///
/// 1. If target provided, find workflow or task with that name
/// 2. If no target, use workflow if present
/// 3. If no target and no workflow, use single task if exactly one exists
/// 4. Otherwise error
pub fn select_target(
    document: &AnalysisDocument,
    target: Option<&str>,
) -> Result<Target, SelectTargetError> {
    if let Some(target) = target {
        // An explicit target name was provided, attempt to find the workflow or
        // task by name
        if let Some(workflow) = document.workflow()
            && workflow.name() == target
        {
            return Ok(Target::Workflow(target.to_owned()));
        }

        if document.task_by_name(target).is_some() {
            return Ok(Target::Task(target.to_owned()));
        }

        Err(SelectTargetError::TargetNotFound(target.to_owned()))
    } else {
        // No explicit target name was provided, infer using the rules outlined
        // above
        if let Some(workflow) = document.workflow() {
            // Document has a workflow, use that as the target
            Ok(Target::Workflow(workflow.name().to_owned()))
        } else {
            // No workflow was found, see if there is one task
            let tasks = document.tasks().collect::<Vec<_>>();
            match tasks.len() {
                0 => Err(SelectTargetError::NoExecutableTarget),
                1 => Ok(Target::Task(tasks[0].name().to_owned())),
                _ => Err(SelectTargetError::TargetRequired),
            }
        }
    }
}

/// Run execution context.
#[derive(Debug, Clone)]
pub struct RunContext {
    /// Run ID.
    pub run_id: Uuid,
    /// Generated run name.
    pub run_generated_name: String,
    /// Run start time.
    pub started_at: DateTime<Utc>,
}

/// The name for the "latest" symlink.
const LATEST: &str = "_latest";

/// Context for executing the full run pipeline (analysis → execution).
///
/// This struct encapsulates all the state needed to execute a run
/// asynchronously, from initial WDL document analysis through to final
/// execution and cleanup. It is designed to be constructed via the builder and
/// then consumed via the [`execute`](Self::execute) method, which performs all
/// steps of the pipeline.
#[derive(bon::Builder)]
#[allow(missing_debug_implementations)]
pub struct RunnableExecutor {
    /// Database handle for persisting run state.
    db: Arc<dyn Database>,
    /// Output directory manager.
    output_dir: OutputDirectory,
    /// WDL engine configuration.
    engine_config: WdlConfig,
    /// Cancellation context for this run.
    cancellation: CancellationContext,
    /// Events broadcaster for progress reporting.
    events: Events,
    /// Shared mapping of active runs for cleanup on completion.
    runs: Arc<Mutex<HashMap<Uuid, CancellationContext>>>,
    /// Unique identifier for this run.
    run_id: Uuid,
    /// Human-readable generated name for this run.
    #[builder(into)]
    run_name: String,
    /// Validated source location of the WDL document.
    source: AllowedSource,
    /// User-provided target name (workflow or task), if any.
    target: Option<String>,
    /// Input values as JSON.
    inputs: JsonValue,
    /// Index key for result indexing, if requested.
    index_on: Option<String>,
}

impl RunnableExecutor {
    /// Executes a runnable.
    ///
    /// This method performs all steps of execution in order:
    ///
    /// 1. Analyze the WDL document
    /// 2. Validate analysis results
    /// 3. Select or resolve the target
    /// 4. Create the run directory
    /// 5. Execute the target
    ///
    /// On any error, the run is marked as failed in the database and the method
    /// returns early. On completion (success or failure), the run is removed
    /// from the active runs map.
    pub async fn execute(self) {
        let result = match analyze_wdl_document(&self.source).await {
            Ok(result) => result,
            Err(e) => {
                tracing::error!(
                    "run `{}` ({}) failed during analysis: {}",
                    &self.run_name,
                    self.run_id,
                    e
                );
                let _ = self
                    .db
                    .fail_run(self.run_id, &e.to_string(), Utc::now())
                    .await;
                self.runs.lock().await.remove(&self.run_id);
                return;
            }
        };

        let document = result.document();
        let resolved_target = match select_target(document, self.target.as_deref()) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(
                    "run `{}` ({}) failed during target selection: {}",
                    &self.run_name,
                    self.run_id,
                    e
                );
                let _ = self
                    .db
                    .fail_run(self.run_id, &e.to_string(), Utc::now())
                    .await;
                self.runs.lock().await.remove(&self.run_id);
                return;
            }
        };

        // Update the target in the database if the user didn't provide one.
        if self.target.is_none() {
            match self
                .db
                .update_run_target(self.run_id, resolved_target.name())
                .await
            {
                Ok(false) => {
                    let error = "run not found when updating target";
                    tracing::error!(
                        "run `{}` ({}) failed to update target: {}",
                        &self.run_name,
                        self.run_id,
                        error
                    );
                    let _ = self.db.fail_run(self.run_id, error, Utc::now()).await;
                    self.runs.lock().await.remove(&self.run_id);
                    return;
                }
                Err(e) => {
                    tracing::error!(
                        "run `{}` ({}) failed to update target: {}",
                        &self.run_name,
                        self.run_id,
                        e
                    );
                    let _ = self
                        .db
                        .fail_run(self.run_id, &e.to_string(), Utc::now())
                        .await;
                    self.runs.lock().await.remove(&self.run_id);
                    return;
                }
                Ok(true) => {}
            }
        }

        let run_dir_name = PathBuf::from(resolved_target.name())
            .join(format!("{}", Utc::now().format("%F_%H%M%S%f")));
        let run_dir = match self.output_dir.ensure_workflow_run(run_dir_name) {
            Ok(dir) => dir,
            Err(e) => {
                tracing::error!(
                    "run `{}` ({}) failed to create run directory: {}",
                    &self.run_name,
                    self.run_id,
                    e
                );
                let _ = self
                    .db
                    .fail_run(self.run_id, &e.to_string(), Utc::now())
                    .await;
                self.runs.lock().await.remove(&self.run_id);
                return;
            }
        };

        let run_dir_str = run_dir.relative_path().to_str().expect("path is not UTF-8");
        if let Err(e) = self.db.update_run_directory(self.run_id, run_dir_str).await {
            tracing::error!(
                "run `{}` ({}) failed to update directory: {}",
                &self.run_name,
                self.run_id,
                e
            );
            let _ = self
                .db
                .fail_run(self.run_id, &e.to_string(), Utc::now())
                .await;
            self.runs.lock().await.remove(&self.run_id);
            return;
        }

        // Create the `_latest` symlink.
        // SAFETY: we know that the `runs/` directory should be the parent here.
        let run_dir_parent = run_dir.root().parent().unwrap();
        let latest_symlink = run_dir_parent.join(LATEST);

        #[cfg(unix)]
        let _ = std::fs::remove_file(&latest_symlink);

        #[cfg(windows)]
        let _ = std::fs::remove_dir(&latest_symlink);

        if let Some(run_dir_basename) = run_dir.root().file_name() {
            #[cfg(unix)]
            let result = std::os::unix::fs::symlink(run_dir_basename, &latest_symlink);

            #[cfg(windows)]
            let result = std::os::windows::fs::symlink_dir(run_dir_basename, &latest_symlink);

            if let Err(e) = result {
                tracing::trace!(
                    "failed to create `_latest` symlink at `{}`: {}",
                    latest_symlink.display(),
                    e
                );
            }
        }

        let ctx = RunContext {
            run_id: self.run_id,
            run_generated_name: self.run_name.clone(),
            started_at: Utc::now(),
        };

        info!(
            "run `{}` ({}) execution started",
            &ctx.run_generated_name, self.run_id
        );

        // SAFETY: because we subscribe to all events above, the Crankshaft
        // subscriber should always be available to us here.
        let crankshaft_rx = self.events.subscribe_crankshaft().unwrap();
        let task_monitor_svc = TaskMonitorSvc::new(self.run_id, self.db.clone(), crankshaft_rx);
        tokio::spawn(task_monitor_svc.run());

        if let Err(e) = execute_target(
            self.db.clone(),
            &ctx,
            result.document().clone(),
            self.engine_config,
            self.cancellation,
            self.events,
            resolved_target,
            &self.inputs,
            &run_dir,
            self.index_on.as_deref(),
        )
        .await
        {
            tracing::error!(
                "run `{}` ({}) failed: {}",
                &ctx.run_generated_name,
                self.run_id,
                e
            );
        }

        self.runs.lock().await.remove(&self.run_id);
    }
}

/// Analyzes a WDL document from the given source.
///
/// Creates an analyzer, adds the document, runs analysis, and returns the
/// result for the entrypoint document. Checks all analysis results (including
/// transitive dependencies) for errors before returning.
pub async fn analyze_wdl_document(source: &AllowedSource) -> Result<AnalysisResult> {
    let analyzer = Analyzer::new(AnalysisConfig::default(), |(), _, _, _| async {});

    let uri = source.to_url();
    analyzer
        .add_document(uri.clone())
        .await
        .context("failed to add document to analyzer")?;

    let results = analyzer
        .analyze(())
        .await
        .context("failed to analyze document")?;

    for result in &results {
        if let Some(e) = result.error() {
            anyhow::bail!("parsing failed for `{}`: {:#}", result.document().uri(), e);
        }

        if let Some(diagnostic) = result
            .document()
            .diagnostics()
            .find(|d| d.severity() == Severity::Error)
        {
            anyhow::bail!(
                "analysis error in `{}`: {:?}",
                result.document().uri(),
                diagnostic
            );
        }
    }

    results
        .into_iter()
        .find(|result| **result.document().uri() == uri)
        .context("analyzer didn't return analysis results for document")
}

/// Parses and validates workflow inputs.
///
/// If inputs are provided, writes them to a file and parses them. Returns
/// workflow inputs or an error if the inputs are for a task instead of a
/// workflow.
async fn parse_workflow_inputs(
    db: &dyn Database,
    ctx: &RunContext,
    inputs: &JsonValue,
    document: &AnalysisDocument,
    run_dir: &RunDirectory,
) -> Result<wdl::engine::WorkflowInputs> {
    // Handle empty inputs
    if inputs.is_null() || inputs.as_object().is_some_and(|o| o.is_empty()) {
        return Ok(Default::default());
    }

    // Write inputs to file
    let inputs_file = run_dir.inputs_file();
    std::fs::write(&inputs_file, serde_json::to_string_pretty(inputs)?)
        .context("failed to write inputs file")?;

    // Parse and validate inputs
    match Inputs::parse(document, &inputs_file)? {
        Some((_, Inputs::Task(_))) => {
            let error = "inputs are for a task, not a workflow";
            db.fail_run(ctx.run_id, error, Utc::now()).await?;
            anyhow::bail!(error);
        }
        Some((_, Inputs::Workflow(inputs))) => Ok(inputs),
        None => Ok(Default::default()),
    }
}

/// Parse and validate task inputs from JSON.
///
/// Handles empty or null inputs by returning default values. For non-empty
/// inputs, writes them to a file in the run directory, parses them, and
/// validates that they match the target task. If the inputs are for a workflow
/// or a different task, marks the run as failed in the database and returns an
/// error.
async fn parse_task_inputs(
    db: &dyn Database,
    ctx: &RunContext,
    inputs: &JsonValue,
    document: &AnalysisDocument,
    task: &wdl::analysis::document::Task,
    run_dir: &RunDirectory,
) -> Result<wdl::engine::TaskInputs> {
    // Handle empty inputs
    if inputs.is_null() || inputs.as_object().is_some_and(|o| o.is_empty()) {
        return Ok(Default::default());
    }

    // Write inputs to file
    let inputs_file = run_dir.inputs_file();
    std::fs::write(&inputs_file, serde_json::to_string_pretty(inputs)?)
        .context("failed to write inputs file")?;

    // Parse and validate inputs
    match Inputs::parse(document, &inputs_file)? {
        Some((name, Inputs::Task(task_inputs))) => {
            if name != task.name() {
                let error = format!(
                    "inputs are for task `{}`, but executing task `{}`",
                    name,
                    task.name()
                );
                db.fail_run(ctx.run_id, &error, Utc::now()).await?;
                anyhow::bail!(error);
            }
            Ok(task_inputs)
        }
        Some((_, Inputs::Workflow(_))) => {
            let error = "inputs are for a workflow, not a task";
            db.fail_run(ctx.run_id, error, Utc::now()).await?;
            anyhow::bail!(error);
        }
        None => Ok(Default::default()),
    }
}

/// Handles successful run execution.
///
/// Creates provenance index entries (if `index_on` is provided), serializes
/// outputs, and marks the run as completed.
async fn set_run_success(
    db: &dyn Database,
    ctx: &RunContext,
    target: Target,
    outputs: Outputs,
    run_dir: &RunDirectory,
    index_on: Option<&str>,
) -> Result<()> {
    // Serialize outputs
    let outputs_with_name = outputs.with_name(target.name());
    let outputs_json =
        serde_json::to_value(&outputs_with_name).context("failed to serialize run outputs")?;

    // Write outputs to file
    let outputs_file = run_dir.outputs_file();
    std::fs::write(&outputs_file, serde_json::to_string_pretty(&outputs_json)?)
        .context("failed to write outputs file")?;

    // Update outputs in database
    let outputs_str = serde_json::to_string(&outputs_json)?;
    db.update_run_outputs(ctx.run_id, &outputs_str).await?;

    let output_dir = run_dir.output_directory();

    // Create the index entries if index_on was provided
    if let Some(index_on) = index_on {
        crate::system::v1::fs::index::create_index_entries(
            db,
            ctx.run_id,
            run_dir,
            index_on,
            &outputs_with_name,
        )
        .await
        .context("failed to create index entry")?;

        // Update the index directory in the database after successful indexing
        let index_dir = output_dir
            .ensure_index_dir(index_on)
            .context("failed to ensure index directory")?;
        let relative_index_dir = output_dir
            .make_relative_to(&index_dir)
            .expect("index directory should be within output directory");
        let updated = db
            .update_run_index_directory(ctx.run_id, &relative_index_dir)
            .await
            .context("failed to update run index directory")?;
        if !updated {
            anyhow::bail!("run not found when updating index directory");
        }
    }

    db.complete_run(ctx.run_id, Utc::now()).await?;

    info!(
        "run `{}` ({}) completed successfully",
        ctx.run_generated_name, ctx.run_id
    );
    Ok(())
}

/// Execute a workflow target.
///
/// Parses the workflow inputs from JSON, creates a workflow evaluator with the
/// provided configuration, cancellation context, and events handler, then
/// evaluates the workflow and returns the outputs. The workflow is executed in
/// the provided run directory.
#[allow(clippy::too_many_arguments)]
async fn execute_workflow_target(
    db: &dyn Database,
    ctx: &RunContext,
    document: &AnalysisDocument,
    config: Arc<WdlConfig>,
    cancellation: CancellationContext,
    events: Events,
    inputs: &JsonValue,
    run_dir: &RunDirectory,
) -> Result<Outputs> {
    let workflow_inputs = parse_workflow_inputs(db, ctx, inputs, document, run_dir).await?;

    let evaluator = Evaluator::new(run_dir.root(), config, cancellation, events)
        .await
        .context("failed to create workflow evaluator")?;

    evaluator
        .evaluate_workflow(document, workflow_inputs, run_dir.root())
        .await
        .map_err(|e| anyhow::anyhow!("workflow evaluation failed: {:#?}", e))
}

/// Execute a task target.
///
/// Retrieves the task from the document by name, parses the task inputs from
/// JSON, creates a task evaluator with the provided configuration, cancellation
/// context, and events handler, then evaluates the task and returns the
/// outputs. The task is executed in the provided run directory.
#[allow(clippy::too_many_arguments)]
async fn execute_task_target(
    db: &dyn Database,
    ctx: &RunContext,
    document: &AnalysisDocument,
    config: Arc<WdlConfig>,
    cancellation: CancellationContext,
    events: Events,
    target: &Target,
    inputs: &JsonValue,
    run_dir: &RunDirectory,
) -> Result<Outputs> {
    let task = document
        .task_by_name(target.name())
        // SAFETY: we should never get to this point with a task target without
        // the task being present in the document.
        .unwrap();

    let task_inputs = parse_task_inputs(db, ctx, inputs, document, task, run_dir).await?;

    let evaluator = Evaluator::new(run_dir.root(), config, cancellation, events)
        .await
        .context("failed to create task evaluator")?;

    let evaluated_task = evaluator
        .evaluate_task(document, task, task_inputs, run_dir.root())
        .await
        .map_err(|e| anyhow::anyhow!("task evaluation failed: {:#?}", e))?;

    evaluated_task
        .into_outputs()
        .map_err(|e| anyhow::anyhow!("task outputs evaluation failed: {:#?}", e))
}

/// Execute a workflow or task target.
///
/// This function orchestrates the execution of either a workflow or a
/// standalone task, based on the target name provided. It handles status
/// updates, input parsing, evaluation, output collection, and indexing.
///
/// # Arguments
///
/// - `db` is a reference to the database and is used to update various aspects
///   of the datbase as execution proceeds.
/// - `ctx` is the context of the run created for this execution (run UUID, run
///   name, start time, etc).
/// - `document` is the analysis document containing the task or workflow to
///   execute.
/// - `config` is the WDL engine configuration to use during evaluation.
/// - `cancellation` is the cancellation context for this run.
/// - `events` is the events system for progress reporting.
/// - `target` is the target we are attempting to execute.
/// - `inputs` is the unparsed version of the inputs as JSON.
/// - `run_dir` is the run directory to output the results to.
/// - `index_on` is the key to index results on, if provided.
#[allow(clippy::too_many_arguments)]
pub async fn execute_target(
    db: Arc<dyn Database>,
    ctx: &RunContext,
    document: AnalysisDocument,
    config: WdlConfig,
    cancellation: CancellationContext,
    events: Events,
    target: Target,
    inputs: &JsonValue,
    run_dir: &RunDirectory,
    index_on: Option<&str>,
) -> Result<()> {
    let config = Arc::new(config);
    db.start_run(ctx.run_id, ctx.started_at).await?;

    let result: Result<()> = async {
        let outputs = match &target {
            Target::Task(_) => {
                execute_task_target(
                    db.as_ref(),
                    ctx,
                    &document,
                    config,
                    cancellation,
                    events,
                    &target,
                    inputs,
                    run_dir,
                )
                .await?
            }
            Target::Workflow(_) => {
                execute_workflow_target(
                    db.as_ref(),
                    ctx,
                    &document,
                    config,
                    cancellation,
                    events,
                    inputs,
                    run_dir,
                )
                .await?
            }
        };

        set_run_success(db.as_ref(), ctx, target, outputs, run_dir, index_on).await
    }
    .await;

    if let Err(e) = result {
        let error = format!("{:#}", e);
        db.fail_run(ctx.run_id, &error, Utc::now()).await?;
        anyhow::bail!(error);
    }

    Ok(())
}
