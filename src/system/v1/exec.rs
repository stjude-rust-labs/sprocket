//! Execution of runs for provenance tracking in v1.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use chrono::DateTime;
use chrono::Utc;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::info;
use uuid::Uuid;
use wdl::analysis::AnalysisResult;
use wdl::analysis::Analyzer;
use wdl::analysis::Config as AnalysisConfig;
use wdl::analysis::Document as AnalysisDocument;
use wdl::ast::Severity;
use wdl::ast::SupportedVersion;
use wdl::engine::CancellationContext;
use wdl::engine::Config as WdlConfig;
use wdl::engine::EvaluationError;
use wdl::engine::EvaluationPath;
use wdl::engine::Events;
use wdl::engine::Inputs;
use wdl::engine::Outputs;
use wdl::engine::TaskInputs;
use wdl::engine::WorkflowInputs;
use wdl::engine::v1::Evaluator;

use crate::analysis::Source;
use crate::system::v1::db::Database;
use crate::system::v1::db::DatabaseError;
use crate::system::v1::db::Run;
use crate::system::v1::db::Session;
use crate::system::v1::db::SprocketCommand;
use crate::system::v1::db::SqliteDatabase;
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
pub use source::validate as validate_source;

/// Represents a JSON object.
type JsonObject = serde_json::Map<String, serde_json::Value>;

/// The name for the `_latest` symlink.
const LATEST: &str = "_latest";

/// Creates a `_latest` symlink pointing to the given run directory.
///
/// On Unix, this creates a symbolic link. On Windows, this creates a directory
/// junction. Any existing symlink at the target location is removed first.
/// Failures are logged at trace level but do not cause errors.
fn create_latest_symlink(run_dir: &RunDirectory) {
    let Some(run_dir_parent) = run_dir.root().parent() else {
        return;
    };
    let latest_symlink = run_dir_parent.join(LATEST);

    #[cfg(unix)]
    let _ = fs::remove_file(&latest_symlink);

    #[cfg(windows)]
    let _ = fs::remove_dir(&latest_symlink);

    let Some(run_dir_basename) = run_dir.root().file_name() else {
        return;
    };

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

/// Opens or creates a SQLite database at the given path.
///
/// If the database does not exist, it will be created along with any
/// necessary parent directories. Migrations are run automatically.
pub async fn open_database(path: impl AsRef<Path>) -> Result<Arc<dyn Database>> {
    let db = SqliteDatabase::new(path)
        .await
        .context("failed to open database")?;
    Ok(Arc::new(db))
}

/// Creates a new session record in the database.
///
/// The session tracks which Sprocket command initiated the execution and
/// associates all runs created during that session.
pub async fn create_session(
    db: &dyn Database,
    command: SprocketCommand,
) -> Result<Session, DatabaseError> {
    let id = Uuid::new_v4();
    let username = whoami::username()?;
    db.create_session(id, command, &username).await
}

/// Creates a timestamped run directory for the given target.
///
/// The directory is created at `<output_dir>/runs/<target>/<timestamp>/` where
/// timestamp has the format `YYYY-MM-DD_HHMMSSffffff`. A `_latest` symlink is
/// created pointing to the new directory.
///
/// Returns the [`RunDirectory`] handle for the created directory.
pub fn create_run_directory(output_dir: &OutputDirectory, target: &str) -> Result<RunDirectory> {
    let run_dir_name = PathBuf::from(target).join(format!("{}", Utc::now().format("%F_%H%M%S%f")));

    let run_dir = output_dir
        .ensure_workflow_run(run_dir_name)
        .context("failed to create run directory")?;

    create_latest_symlink(&run_dir);

    Ok(run_dir)
}

/// Creates a new run record in the database.
///
/// The provided inputs are expected to be in JSON.
///
/// Returns the generated run ID, run name, and the database run record.
pub async fn create_run_record(
    db: &dyn Database,
    session_id: Uuid,
    source: &Source,
    target: Option<&str>,
    inputs: &str,
) -> Result<(Uuid, String, Run), DatabaseError> {
    let run_id = Uuid::new_v4();
    let run_name = generate_run_name();
    let source_str = source.to_string();

    let run = db
        .create_run(run_id, session_id, &run_name, &source_str, target, inputs)
        .await?;

    Ok((run_id, run_name, run))
}

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
    #[error(
        "a target cannot be inferred because the document contains multiple tasks and no workflow"
    )]
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
    /// The fallback WDL version for documents with unrecognized versions.
    fallback_version: Option<SupportedVersion>,
    /// Validated source location of the WDL document.
    source: Source,
    /// User-provided target name (workflow or task), if any.
    target: Option<String>,
    /// The engine inputs.
    inputs: JsonObject,
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
        let result = match analyze_wdl_document(&self.source, self.fallback_version).await {
            Ok(result) => result,
            Err(e) => {
                tracing::error!(
                    "run `{}` ({}) failed during analysis: {e:#}",
                    &self.run_name,
                    self.run_id
                );
                let _ = self
                    .db
                    .fail_run(self.run_id, &format!("{e:#}"), Utc::now())
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

        create_latest_symlink(&run_dir);

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

        // Resolve relative paths in inputs from the current working directory.
        let cwd = std::env::current_dir().expect("failed to get current working directory");
        let base_dir = EvaluationPath::from(cwd.as_path());

        let inputs = match self
            .parse_inputs(result.document(), &resolved_target)
            .context("failed to parse inputs")
        {
            Ok(inputs) => inputs,
            Err(e) => {
                tracing::error!(
                    "run `{}` ({}) failed to parse inputs: {e:#}",
                    &self.run_name,
                    self.run_id
                );

                let _ = self
                    .db
                    .fail_run(self.run_id, &format!("{e:#}"), Utc::now())
                    .await;
                self.runs.lock().await.remove(&self.run_id);
                return;
            }
        };

        if let Err(e) = execute_target(
            self.db.clone(),
            &ctx,
            result.document().clone(),
            self.engine_config,
            self.cancellation,
            self.events,
            resolved_target,
            inputs,
            &run_dir,
            &base_dir,
            self.index_on.as_deref(),
        )
        .await
        {
            tracing::error!(
                "run `{}` ({}) failed: {}",
                &ctx.run_generated_name,
                self.run_id,
                e.to_string()
            );
        }

        self.runs.lock().await.remove(&self.run_id);
    }

    /// Parses the inputs into engine inputs.
    fn parse_inputs(
        &self,
        document: &AnalysisDocument,
        resolved_target: &Target,
    ) -> Result<Inputs> {
        let Some((target, inputs)) = Inputs::parse_json_object(document, self.inputs.clone())?
        else {
            return match resolved_target {
                Target::Task(_) => Ok(TaskInputs::default().into()),
                Target::Workflow(_) => Ok(WorkflowInputs::default().into()),
            };
        };

        if let Some(t) = &self.target
            && target != *t
        {
            bail!(format!(
                "supplied target `{t}` does not match the target `{target}` derived from the \
                 inputs"
            ))
        }

        Ok(inputs)
    }
}

/// Analyzes a WDL document from the given source.
///
/// Creates an analyzer, adds the document, runs analysis, and returns the
/// result for the entrypoint document. Checks all analysis results (including
/// transitive dependencies) for errors before returning.
pub async fn analyze_wdl_document(
    source: &Source,
    fallback_version: Option<SupportedVersion>,
) -> Result<AnalysisResult> {
    let config = AnalysisConfig::default().with_fallback_version(fallback_version);
    let analyzer = Analyzer::new(config, |(), _, _, _| async {});

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
            bail!("parsing failed for `{}`: {:#}", result.document().uri(), e);
        }

        if let Some(diagnostic) = result
            .document()
            .diagnostics()
            .find(|d| d.severity() == Severity::Error)
        {
            bail!(
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
    fs::write(&outputs_file, serde_json::to_string_pretty(&outputs_json)?)
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
            bail!("run not found when updating index directory");
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
/// Parses the workflow inputs from JSON, resolves relative paths from
/// `base_dir`, creates a workflow evaluator with the provided configuration,
/// cancellation context, and events handler, then evaluates the workflow and
/// returns the outputs. The workflow is executed in the provided run directory.
///
/// Returns `Ok(None)` if the workflow was canceled.
#[allow(clippy::too_many_arguments)]
async fn execute_workflow_target(
    db: &dyn Database,
    ctx: &RunContext,
    document: &AnalysisDocument,
    config: Arc<WdlConfig>,
    cancellation: CancellationContext,
    events: Events,
    inputs: Inputs,
    run_dir: &RunDirectory,
    base_dir: &EvaluationPath,
) -> Result<Option<Outputs>, EvaluationError> {
    // Write inputs to file
    let inputs_file = run_dir.inputs_file();
    fs::write(
        &inputs_file,
        serde_json::to_string_pretty(&inputs).context("failed to serialize inputs")?,
    )
    .with_context(|| {
        format!(
            "failed to write inputs file `{path}`",
            path = inputs_file.display()
        )
    })?;

    // Ensure the inputs are for a workflow
    if inputs.as_workflow_inputs().is_none() {
        let error = "inputs are for a task, not a workflow";
        db.fail_run(ctx.run_id, error, Utc::now())
            .await
            .context("failed to update database")?;
        return Err(anyhow!(error).into());
    }

    let mut inputs = inputs.unwrap_workflow_inputs();

    // Resolve relative paths in inputs from `base_dir`
    let workflow = document
        .workflow()
        .context("document does not contain a workflow")?;
    inputs
        .join_paths(workflow, |_| Ok(base_dir))
        .await
        .context("failed to resolve input paths")?;

    let evaluator = Evaluator::new(run_dir.root(), config, cancellation, events)
        .await
        .context("failed to create workflow evaluator")?;

    match evaluator
        .evaluate_workflow(document, inputs, run_dir.root())
        .await
    {
        Ok(outputs) => Ok(Some(outputs)),
        Err(EvaluationError::Canceled) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Execute a task target.
///
/// Retrieves the task from the document by name, parses the task inputs from
/// JSON, resolves relative paths from `base_dir`, creates a task evaluator with
/// the provided configuration, cancellation context, and events handler, then
/// evaluates the task and returns the outputs. The task is executed in the
/// provided run directory.
///
/// Returns `Ok(None)` if the task was canceled.
#[allow(clippy::too_many_arguments)]
async fn execute_task_target(
    db: &dyn Database,
    ctx: &RunContext,
    document: &AnalysisDocument,
    config: Arc<WdlConfig>,
    cancellation: CancellationContext,
    events: Events,
    target: &Target,
    inputs: Inputs,
    run_dir: &RunDirectory,
    base_dir: &EvaluationPath,
) -> Result<Option<Outputs>, EvaluationError> {
    let task = document.task_by_name(target.name()).with_context(|| {
        format!(
            "task `{name}` was not found in the document",
            name = target.name()
        )
    })?;

    // Ensure the inputs are for a tas
    if inputs.as_task_inputs().is_none() {
        let error = "inputs are for a workflow, not a task";
        db.fail_run(ctx.run_id, error, Utc::now())
            .await
            .context("failed to update database")?;
        return Err(anyhow!(error).into());
    }

    let mut inputs = inputs.unwrap_task_inputs();

    // Resolve relative paths in inputs from `base_dir`
    inputs
        .join_paths(task, |_| Ok(base_dir))
        .await
        .context("failed to resolve input paths")?;

    let evaluator = Evaluator::new(run_dir.root(), config, cancellation, events)
        .await
        .context("failed to create task evaluator")?;

    let evaluated_task = match evaluator
        .evaluate_task(document, task, inputs, run_dir.root())
        .await
    {
        Ok(task) => task,
        Err(EvaluationError::Canceled) => return Ok(None),
        Err(e) => return Err(e),
    };

    evaluated_task.into_outputs().map(Some)
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
/// - `base_dir` is the directory from which relative paths in inputs should be
///   resolved. For the server, this is typically the server's working
///   directory. For the CLI, paths should already be absolute.
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
    inputs: Inputs,
    run_dir: &RunDirectory,
    base_dir: &EvaluationPath,
    index_on: Option<&str>,
) -> Result<(), EvaluationError> {
    let config = Arc::new(config);
    db.start_run(ctx.run_id, ctx.started_at)
        .await
        .map_err(anyhow::Error::from)?;

    let result: Result<Option<Outputs>, EvaluationError> = async {
        match &target {
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
                    base_dir,
                )
                .await
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
                    base_dir,
                )
                .await
            }
        }
    }
    .await;

    match result {
        Ok(Some(outputs)) => {
            set_run_success(db.as_ref(), ctx, target, outputs, run_dir, index_on).await?;
            Ok(())
        }
        // NOTE: `Ok(None)` means the execution was canceled. The run manager
        // handles transitioning the run status from `Canceling` to `Canceled`.
        Ok(None) => Ok(()),
        Err(e) => {
            let error = e.to_string();
            if let Err(db_err) = db.fail_run(ctx.run_id, &error, Utc::now()).await {
                tracing::error!("failed to record run failure: {db_err:#}");
            }
            Err(e)
        }
    }
}
