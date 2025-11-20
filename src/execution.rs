//! Run execution.

use std::ops::Deref;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use chrono::DateTime;
use chrono::Utc;
use serde_json::Value as JsonValue;
use tracing::info;
use url::Url;
use uuid::Uuid;
use wdl::analysis::AnalysisResult;
use wdl::analysis::Analyzer;
use wdl::analysis::Config as AnalysisConfig;
use wdl::analysis::Document as AnalysisDocument;
use wdl::ast::Severity;
use wdl::engine::Events;
use wdl::engine::Inputs;
use wdl::engine::Outputs;
use wdl::engine::v1::TaskEvaluator;
use wdl::engine::v1::WorkflowEvaluator;

use crate::Database;
use crate::OutputDirectory;
use crate::database::RunStatus;
use crate::provenance;

pub mod commands;
pub mod config;
pub mod manager;
pub mod names;

pub use commands::ManagerCommand;
pub use config::ConfigError;
pub use config::ConfigResult;
pub use config::ExecutionConfig;
pub use manager::ManagerError;
pub use manager::ManagerResult;
pub use manager::spawn_manager;
pub use names::generate_run_name;

/// Input file name.
const INPUTS_FILE: &str = "inputs.json";

/// Output file name.
const OUTPUTS_FILE: &str = "outputs.json";

/// Run execution directory.
///
/// The first item in the tuple is a the output directory this run is contained
/// within.
///
/// The second item in the tuple is the path to the run directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunDirectory(OutputDirectory, PathBuf);

impl RunDirectory {
    /// Creates a new run directory.
    pub fn new(output_dir: OutputDirectory, name: impl AsRef<Path>) -> Self {
        let path = PathBuf::from(output_dir.root())
            .join(crate::RUNS_DIR)
            .join(name.as_ref());
        Self(output_dir, path)
    }

    /// Gets a reference to the output directory.
    pub fn output_directory(&self) -> &OutputDirectory {
        &self.0
    }

    /// Gets the relative path to the run directory within the output directory
    /// (e.g., `runs/workflow-name`).
    pub fn relative_path(&self) -> &Path {
        // SAFETY: because of the way `RunDirectory`s are created, we know that
        // the inner path is prefixed by the output directory.
        self.1.strip_prefix(self.0.root()).unwrap()
    }

    /// Returns the path to the run execution directory.
    pub fn root(&self) -> &Path {
        &self.1
    }

    /// Returns the path to the inputs file.
    pub fn inputs_file(&self) -> PathBuf {
        self.root().join(INPUTS_FILE)
    }

    /// Returns the path to the outputs file.
    pub fn outputs_file(&self) -> PathBuf {
        self.root().join(OUTPUTS_FILE)
    }
}

impl Deref for RunDirectory {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.1
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

/// A validated workflow source.
///
/// This enum represents a workflow source that has been validated against the
/// execution configuration to prevent:
///
/// - **Path traversal attacks.** File paths are canonicalized and checked
///   against allowed directories using prefix matching.
/// - **Information leakage.** File existence is only revealed for paths within
///   allowed directories.
/// - **URL restriction.** URLs must match configured prefixes exactly,
///   including scheme.
///
/// # Security Invariants
///
/// Once constructed, an [`AllowedSource`] guarantees:
///
/// - File paths are absolute, canonical, and within allowed directories
/// - File paths contain valid UTF-8
/// - URLs match at least one configured prefix
#[derive(Debug, Clone)]
pub enum AllowedSource {
    /// A URL source that has been validated against allowed URL prefixes.
    Url(Url),
    /// A file path that has been shell-expanded, canonicalized, and validated
    /// against allowed file paths.
    File(PathBuf),
}

impl AllowedSource {
    /// Validates a source path against the execution configuration.
    ///
    /// # Preconditions
    ///
    /// The configuration must have been validated via
    /// `ExecutionConfig::validate()` which ensures all allowed paths are
    /// canonical.
    pub fn validate(source: &str, config: &ExecutionConfig) -> ConfigResult<Self> {
        if let Ok(url) = Url::parse(source) {
            let url_str = url.as_str();
            let is_allowed = config
                .allowed_urls
                .iter()
                .any(|prefix| url_str.starts_with(prefix));

            if !is_allowed {
                return Err(ConfigError::UrlForbidden(url));
            }

            Ok(AllowedSource::Url(url))
        } else {
            let expanded = shellexpand::tilde(source);
            let path = Path::new(expanded.as_ref());

            let Ok(canonical_path) = path.canonicalize() else {
                if let Some(parent) = path.parent()
                    && let Ok(parent_canonical) = parent.canonicalize()
                    && let Some(filename) = path.file_name()
                {
                    let would_be_path = parent_canonical.join(filename);
                    let is_allowed = config
                        .allowed_file_paths
                        .iter()
                        .any(|allowed| would_be_path.starts_with(allowed));

                    if is_allowed {
                        return Err(if path.exists() {
                            ConfigError::FailedToCanonicalize(path.to_path_buf())
                        } else {
                            ConfigError::FileNotFound(path.to_path_buf())
                        });
                    }
                }
                return Err(ConfigError::FilePathForbidden(path.to_path_buf()));
            };

            // Check to make sure the path is valid UTF-8.
            if canonical_path.to_str().is_none() {
                return Err(ConfigError::InvalidUtf8(canonical_path));
            }

            // Check to make sure the path is allowed.
            let is_allowed = config
                .allowed_file_paths
                .iter()
                .any(|allowed| canonical_path.starts_with(allowed));

            if !is_allowed {
                return Err(ConfigError::FilePathForbidden(canonical_path));
            }

            Ok(AllowedSource::File(canonical_path))
        }
    }

    /// Returns a reference to the URL if this is an [`AllowedSource::Url`].
    pub fn as_url(&self) -> Option<&Url> {
        match self {
            AllowedSource::Url(url) => Some(url),
            AllowedSource::File(_) => None,
        }
    }

    /// Consumes self and returns the URL if this is an [`AllowedSource::Url`].
    pub fn into_url(self) -> Option<Url> {
        match self {
            AllowedSource::Url(url) => Some(url),
            AllowedSource::File(_) => None,
        }
    }

    /// Returns a reference to the file path if this is an
    /// [`AllowedSource::File`].
    pub fn as_file_path(&self) -> Option<&Path> {
        match self {
            AllowedSource::Url(_) => None,
            AllowedSource::File(path) => Some(path),
        }
    }

    /// Consumes self and returns the file path if this is an
    /// [`AllowedSource::File`].
    pub fn into_file_path(self) -> Option<PathBuf> {
        match self {
            AllowedSource::Url(_) => None,
            AllowedSource::File(path) => Some(path),
        }
    }

    /// Returns the source as a string slice.
    ///
    /// For [`AllowedSource::Url`]s, this returns the URL string.  For file
    /// paths, this returns the path as a string.
    pub fn as_str(&self) -> &str {
        match self {
            AllowedSource::Url(url) => url.as_str(),
            AllowedSource::File(path) => {
                // SAFETY: path was checked to ensure valid UTF-8 at creation.
                path.to_str().expect("path should be valid UTF-8")
            }
        }
    }

    /// Converts the source to a URL.
    ///
    /// For [`AllowedSource::Url`]s, this clones the URL. For file paths, this
    /// converts the path to a `file://` URL.
    pub fn to_url(&self) -> Url {
        match self {
            AllowedSource::Url(url) => url.clone(),
            AllowedSource::File(path) => {
                // SAFETY: path is absolute (canonicalized at creation).
                Url::from_file_path(path).expect("file path should convert to URL")
            }
        }
    }
}

impl std::fmt::Display for AllowedSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AllowedSource::Url(url) => write!(f, "{}", url),
            AllowedSource::File(path) => write!(f, "{}", path.display()),
        }
    }
}

/// Marks a run as failed in the database.
///
/// Updates the run error message and status to `Failed`.
async fn set_run_failed(db: &dyn Database, ctx: &RunContext, error: &str) -> Result<()> {
    // Update error message
    db.update_run_error(ctx.run_id, error)
        .await
        .context("failed to update run error")?;

    // Mark run as failed
    db.update_run_status(
        ctx.run_id,
        RunStatus::Failed,
        Some(ctx.started_at),
        Some(Utc::now()),
    )
    .await
    .context("failed to update run status")
}

/// Analyzes a WDL document from the given source.
///
/// Creates an analyzer, adds the document, runs analysis, and returns the first
/// result.
pub async fn analyze_wdl_document(source: &AllowedSource) -> Result<AnalysisResult> {
    // Create analyzer
    let analyzer = Analyzer::new(AnalysisConfig::default(), |(), _, _, _| async {});

    // Add document
    let uri = source.to_url();
    analyzer
        .add_document(uri)
        .await
        .context("failed to add document")?;

    // Run analysis
    let results = analyzer
        .analyze(())
        .await
        .context("failed to analyze document")?;

    // Get first result
    results.into_iter().next().context("no analysis results")
}

/// Validates the analysis result for parsing and diagnostic errors.
///
/// Returns an error if the analysis failed or contains error-level diagnostics.
pub async fn validate_analysis_results(result: &AnalysisResult) -> Result<()> {
    // Check for parsing errors
    if let Some(e) = result.error() {
        anyhow::bail!("parsing failed: {:#}", e);
    }

    // Check for diagnostic errors
    let diagnostics: Vec<_> = result.document().diagnostics().cloned().collect();
    if let Some(diagnostic) = diagnostics.iter().find(|d| d.severity() == Severity::Error) {
        anyhow::bail!("{:?}", diagnostic);
    }

    Ok(())
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
            set_run_failed(db, ctx, error).await?;
            anyhow::bail!(error);
        }
        Some((_, Inputs::Workflow(inputs))) => Ok(inputs),
        None => Ok(Default::default()),
    }
}

/// Parse and validate task inputs from JSON.
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
                set_run_failed(db, ctx, &error).await?;
                anyhow::bail!(error);
            }
            Ok(task_inputs)
        }
        Some((_, Inputs::Workflow(_))) => {
            let error = "inputs are for a workflow, not a task";
            set_run_failed(db, ctx, error).await?;
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
    run_name: &str,
    outputs: Outputs,
    run_dir: &RunDirectory,
    index_on: Option<&str>,
) -> Result<()> {
    // Serialize outputs
    let outputs_with_name = outputs.with_name(run_name);
    let outputs_json =
        serde_json::to_value(&outputs_with_name).context("failed to serialize run outputs")?;

    // Write outputs to file
    let outputs_file = run_dir.outputs_file();
    std::fs::write(&outputs_file, serde_json::to_string_pretty(&outputs_json)?)
        .context("failed to write outputs file")?;

    // Update outputs in database
    let outputs_str = serde_json::to_string(&outputs_json)?;
    db.update_run_outputs(ctx.run_id, &outputs_str)
        .await
        .context("failed to update run outputs")?;

    let output_dir = run_dir.output_directory();

    // Create the index entries if index_on was provided
    if let Some(index_on) = index_on {
        provenance::index::create_index_entries(
            db,
            ctx.run_id,
            run_dir,
            index_on,
            &outputs_with_name,
        )
        .await
        .context("failed to create provenance index")?;

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

    // Mark run as completed
    db.update_run_status(
        ctx.run_id,
        RunStatus::Completed,
        Some(ctx.started_at),
        Some(Utc::now()),
    )
    .await
    .context("failed to update run status")?;

    info!(
        "run `{}` ({}) completed successfully",
        ctx.run_generated_name, ctx.run_id
    );
    Ok(())
}

/// Execute a workflow target.
#[allow(clippy::too_many_arguments)]
async fn execute_workflow_target(
    db: &dyn Database,
    ctx: &RunContext,
    document: &AnalysisDocument,
    config: wdl::engine::Config,
    cancellation: wdl::engine::CancellationContext,
    events: Events,
    inputs: &JsonValue,
    run_dir: &RunDirectory,
) -> Result<Outputs> {
    let workflow_inputs = parse_workflow_inputs(db, ctx, inputs, document, run_dir).await?;

    let evaluator = WorkflowEvaluator::new(config, cancellation, events)
        .await
        .context("failed to create workflow evaluator")?;

    evaluator
        .evaluate(document, workflow_inputs, run_dir.root())
        .await
        .map_err(|e| anyhow::anyhow!("workflow evaluation failed: {:#?}", e))
}

/// Execute a task target.
#[allow(clippy::too_many_arguments)]
async fn execute_task_target(
    db: &dyn Database,
    ctx: &RunContext,
    document: &AnalysisDocument,
    config: wdl::engine::Config,
    cancellation: wdl::engine::CancellationContext,
    events: Events,
    target_name: &str,
    inputs: &JsonValue,
    run_dir: &RunDirectory,
) -> Result<Outputs> {
    let task = document
        .task_by_name(target_name)
        .context("task not found in document")?;

    let task_inputs = parse_task_inputs(db, ctx, inputs, document, task, run_dir).await?;

    let evaluator = TaskEvaluator::new(config, cancellation, events)
        .await
        .context("failed to create task evaluator")?;

    let evaluated_task = evaluator
        .evaluate(document, task, &task_inputs, run_dir.root())
        .await
        .map_err(|e| anyhow::anyhow!("task evaluation failed: {:#?}", e))?;

    evaluated_task
        .into_result()
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
/// - `target_name` is the name of the target we are attempting to execute.
/// - `inputs` is the unparsed version of the inputs as JSON.
/// - `run_dir` is the run directory to output the results to.
/// - `index_on` is the key to index results on, if provided.
#[allow(clippy::too_many_arguments)]
pub async fn execute_task_or_workflow(
    db: Arc<dyn Database>,
    ctx: &RunContext,
    document: AnalysisDocument,
    config: wdl::engine::Config,
    cancellation: wdl::engine::CancellationContext,
    events: Events,
    target_name: &str,
    inputs: &JsonValue,
    run_dir: &RunDirectory,
    index_on: Option<&str>,
) -> Result<()> {
    // Mark run as `running`.
    db.update_run_status(ctx.run_id, RunStatus::Running, Some(ctx.started_at), None)
        .await
        .context("failed to update run status to `running`")?;

    let result: Result<()> = async {
        let outputs = if let Some(workflow) = document.workflow() {
            if workflow.name() == target_name {
                // Execute the named workflow.
                execute_workflow_target(
                    db.as_ref(),
                    ctx,
                    &document,
                    config,
                    cancellation.clone(),
                    events.clone(),
                    inputs,
                    run_dir,
                )
                .await?
            } else {
                // Execute the named task.
                execute_task_target(
                    db.as_ref(),
                    ctx,
                    &document,
                    config,
                    cancellation.clone(),
                    events.clone(),
                    target_name,
                    inputs,
                    run_dir,
                )
                .await?
            }
        } else {
            // No workflow, assume the target is a task.
            execute_task_target(
                db.as_ref(),
                ctx,
                &document,
                config,
                cancellation.clone(),
                events.clone(),
                target_name,
                inputs,
                run_dir,
            )
            .await?
        };

        // Mark the run as successful.
        set_run_success(db.as_ref(), ctx, target_name, outputs, run_dir, index_on).await
    }
    .await;

    if let Err(e) = result {
        let error = format!("{:#}", e);
        set_run_failed(db.as_ref(), ctx, &error).await?;
        anyhow::bail!(error);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_url_allowed() {
        let mut config = ExecutionConfig::builder()
            .output_directory(PathBuf::from("./out"))
            .allowed_file_paths(vec![])
            .allowed_urls(vec![
                String::from("https://example.com/"),
                String::from("http://localhost/"),
            ])
            .build();
        config.validate().unwrap();

        let result = AllowedSource::validate("https://example.com/workflow.wdl", &config);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), AllowedSource::Url(_)));
    }

    #[test]
    fn validate_url_forbidden() {
        let mut config = ExecutionConfig::builder()
            .output_directory(PathBuf::from("./out"))
            .allowed_file_paths(vec![])
            .allowed_urls(vec![String::from("https://example.com/")])
            .build();
        config.validate().unwrap();

        let result = AllowedSource::validate("https://forbidden.com/workflow.wdl", &config);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::UrlForbidden(_)));
    }

    #[test]
    fn validate_file_allowed() {
        use std::fs::File;

        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("workflow.wdl");
        File::create(&file_path).unwrap();

        let config = ExecutionConfig::builder()
            .output_directory(PathBuf::from("./out"))
            .allowed_file_paths(vec![temp_dir.path().canonicalize().unwrap()])
            .allowed_urls(vec![])
            .build();

        let result = AllowedSource::validate(file_path.to_str().unwrap(), &config);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), AllowedSource::File(_)));
    }

    #[test]
    fn validate_file_forbidden() {
        use std::fs::File;

        use tempfile::TempDir;

        let allowed_dir = TempDir::new().unwrap();
        let forbidden_dir = TempDir::new().unwrap();

        // Create a file in the forbidden directory
        let existing_file = forbidden_dir.path().join("workflow.wdl");
        File::create(&existing_file).unwrap();

        // Also test with non-existent file in forbidden directory
        let nonexistent_file = forbidden_dir.path().join("missing.wdl");

        let config = ExecutionConfig::builder()
            .output_directory(PathBuf::from("./out"))
            .allowed_file_paths(vec![allowed_dir.path().canonicalize().unwrap()])
            .allowed_urls(vec![])
            .build();

        // Both should return FilePathForbidden without leaking existence info
        let result1 = AllowedSource::validate(existing_file.to_str().unwrap(), &config);
        assert!(matches!(
            result1.unwrap_err(),
            ConfigError::FilePathForbidden(_)
        ));

        let result2 = AllowedSource::validate(nonexistent_file.to_str().unwrap(), &config);
        assert!(matches!(
            result2.unwrap_err(),
            ConfigError::FilePathForbidden(_)
        ));
    }

    #[test]
    fn validate_file_not_found() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("missing.wdl");

        let config = ExecutionConfig::builder()
            .output_directory(PathBuf::from("./out"))
            .allowed_file_paths(vec![temp_dir.path().canonicalize().unwrap()])
            .allowed_urls(vec![])
            .build();

        // Should reveal FileNotFound since it's in an allowed directory
        let result = AllowedSource::validate(nonexistent.to_str().unwrap(), &config);
        assert!(matches!(result.unwrap_err(), ConfigError::FileNotFound(_)));
    }

    #[test]
    fn validate_url_scheme_must_match() {
        let config = ExecutionConfig::builder()
            .output_directory(PathBuf::from("./out"))
            .allowed_file_paths(vec![])
            .allowed_urls(vec![String::from("https://example.com/")])
            .build();

        // http should not be allowed when only https is configured
        let result = AllowedSource::validate("http://example.com/workflow.wdl", &config);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::UrlForbidden(_)));
    }

    #[test]
    fn path_with_dotdot() {
        use std::fs::File;

        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let subdir = temp_dir.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();
        let file_path = subdir.join("workflow.wdl");
        File::create(&file_path).unwrap();

        let config = ExecutionConfig::builder()
            .output_directory(PathBuf::from("./out"))
            .allowed_file_paths(vec![temp_dir.path().canonicalize().unwrap()])
            .allowed_urls(vec![])
            .build();

        let path_with_dotdot = subdir.join("..").join("subdir").join("workflow.wdl");
        let result = AllowedSource::validate(path_with_dotdot.to_str().unwrap(), &config);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), AllowedSource::File(_)));
    }

    #[test]
    fn url_trailing_slash() {
        let config = ExecutionConfig::builder()
            .output_directory(PathBuf::from("./out"))
            .allowed_file_paths(vec![])
            .allowed_urls(vec![String::from("https://example.com/allowed/")])
            .build();

        let allowed = AllowedSource::validate("https://example.com/allowed/workflow.wdl", &config);
        assert!(allowed.is_ok());

        let forbidden =
            AllowedSource::validate("https://example.com/allowedother/workflow.wdl", &config);
        assert!(forbidden.is_err());
        assert!(matches!(
            forbidden.unwrap_err(),
            ConfigError::UrlForbidden(_)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape() {
        use std::fs::File;
        use std::os::unix::fs::symlink;

        use tempfile::TempDir;

        let allowed_dir = TempDir::new().unwrap();
        let forbidden_dir = TempDir::new().unwrap();

        let forbidden_file = forbidden_dir.path().join("secret.wdl");
        File::create(&forbidden_file).unwrap();

        let symlink_path = allowed_dir.path().join("escape.wdl");
        symlink(&forbidden_file, &symlink_path).unwrap();

        let config = ExecutionConfig::builder()
            .output_directory(PathBuf::from("./out"))
            .allowed_file_paths(vec![allowed_dir.path().canonicalize().unwrap()])
            .allowed_urls(vec![])
            .build();

        let result = AllowedSource::validate(symlink_path.to_str().unwrap(), &config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigError::FilePathForbidden(_)
        ));
    }
}
