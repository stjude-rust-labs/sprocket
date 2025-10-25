//! Workflow manager actor implementation.

use anyhow::Context;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tracing::debug;
use tracing::info;
use uuid::Uuid;

use crate::commands::run::setup_run_dir;
use crate::server::config::Config;
use crate::server::db::Database;
use crate::server::db::WorkflowStatus;
use crate::server::db::WdlSourceType;
use crate::server::manager::commands::*;
use crate::server::names::generate_workflow_name;

/// Channel buffer size for manager commands.
const CHANNEL_BUFFER_SIZE: usize = 200;

/// Workflow manager actor.
pub struct WorkflowManager {
    /// Configuration.
    config: Config,
    /// Database handle.
    db: Database,
    /// Command receiver.
    rx: mpsc::Receiver<ManagerCommand>,
    /// Running workflow tasks.
    workflows: HashMap<String, JoinHandle<()>>,
    /// Semaphore for limiting concurrent workflows.
    semaphore: Option<Arc<Semaphore>>,
}

impl WorkflowManager {
    /// Create a new workflow manager.
    pub fn new(config: Config, db: Database, rx: mpsc::Receiver<ManagerCommand>) -> Self {
        let semaphore = config
            .server
            .max_concurrent_workflows
            .map(|max| Arc::new(Semaphore::new(max)));

        Self {
            config,
            db,
            rx,
            workflows: HashMap::new(),
            semaphore,
        }
    }

    /// Run the manager event loop.
    pub async fn run(mut self) {
        info!("workflow manager started");

        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                ManagerCommand::Submit { source, inputs, rx } => {
                    debug!(?source, ?inputs, "received `Submit` command");
                    let result = self.handle_submit(source, inputs).await;
                    let _ = rx.send(result);
                }
                ManagerCommand::GetStatus { id, rx } => {
                    debug!(?id, "received `GetStatus` command");
                    let result = self.handle_get_status(id).await;
                    let _ = rx.send(result);
                }
                ManagerCommand::List {
                    status,
                    limit,
                    offset,
                    rx,
                } => {
                    debug!(?status, ?limit, ?offset, "received `List` command");
                    let result = self.handle_list(status, limit, offset).await;
                    let _ = rx.send(result);
                }
                ManagerCommand::Cancel { id, rx } => {
                    debug!(?id, "received `Cancel` command");
                    let result = self.handle_cancel(id).await;
                    let _ = rx.send(result);
                }
                ManagerCommand::GetOutputs { id, rx } => {
                    debug!(?id, "received `GetOutputs` command");
                    let result = self.handle_get_outputs(id).await;
                    let _ = rx.send(result);
                }
                ManagerCommand::GetLogs {
                    id,
                    limit,
                    offset,
                    rx,
                } => {
                    debug!(?id, ?limit, ?offset, "received `GetLogs` command");
                    let result = self.handle_get_logs(id, limit, offset).await;
                    let _ = rx.send(result);
                }
                ManagerCommand::Shutdown { rx } => {
                    debug!("received `Shutdown` command");
                    info!("workflow manager shutting down");
                    let _ = rx.send(Ok(()));
                    break;
                }
            }
        }

        info!("workflow manager stopped");
    }

    /// Handle workflow submission.
    async fn handle_submit(&mut self, source: WdlSource, inputs: serde_json::Value) -> Result<SubmitResponse> {
        // Validate file sources if needed.
        if let WdlSource::File(ref path) = source {
            if !self.config.server.allow_file_sources {
                anyhow::bail!("file sources are not allowed");
            }

            if !path.exists() {
                anyhow::bail!("file does not exist: {}", path.display());
            }

            // Canonicalize the path to resolve symlinks and `..` components.
            let canonical_path = path
                .canonicalize()
                .context("failed to canonicalize path")?;

            // Check if the canonical path is within any of the allowed paths.
            // Allowed paths are already canonicalized in the config.
            let is_allowed = self
                .config
                .server
                .allowed_file_paths
                .iter()
                .any(|allowed| canonical_path.starts_with(allowed));

            if !is_allowed {
                anyhow::bail!("file path is not in allowed paths");
            }
        }

        // Generate workflow ID and name.
        let id = Uuid::new_v4().to_string();
        let name = generate_workflow_name();

        // Store workflow in database.
        let (source_type, source_value) = match &source {
            WdlSource::Content(content) => (WdlSourceType::Content, content.clone()),
            WdlSource::File(path) => (WdlSourceType::File, path.display().to_string()),
        };

        sqlx::query(
            "insert into workflows (id, name, status, wdl_source_type, wdl_source_value, inputs)
             values (?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(&name)
        .bind(WorkflowStatus::Queued)
        .bind(source_type)
        .bind(&source_value)
        .bind(&inputs)
        .execute(self.db.pool())
        .await
        .context("failed to insert workflow")?;

        // Spawn workflow execution task.
        let semaphore = self.semaphore.clone();
        let workflow_id = id.clone();
        let workflow_name = name.clone();
        let db = self.db.clone();
        let runs_root = self.config.server.runs_directory.clone();
        let task = tokio::spawn(async move {
            // Acquire semaphore permit if concurrency limit is set.
            let _permit = if let Some(ref sem) = semaphore {
                Some(sem.acquire().await.expect("semaphore closed"))
            } else {
                None
            };

            info!("workflow `{}` execution started", workflow_id);

            // Setup run directory.
            let run_dir = match setup_run_dir(&runs_root, &workflow_name) {
                Ok(dir) => dir,
                Err(e) => {
                    tracing::error!("workflow `{}` failed to setup run directory: {}", workflow_id, e);
                    return;
                }
            };

            // Store run directory in database.
            if let Err(e) = sqlx::query("update workflows set run_directory = ? where id = ?")
                .bind(run_dir.display().to_string())
                .bind(&workflow_id)
                .execute(db.pool())
                .await
            {
                tracing::error!("workflow `{}` failed to store run directory: {}", workflow_id, e);
            }

            // Execute the workflow.
            if let Err(e) = execute_workflow(&workflow_id, &source, &inputs, &run_dir, &db).await {
                tracing::error!("workflow `{}` failed: {}", workflow_id, e);
            }
        });

        self.workflows.insert(id.clone(), task);

        Ok(SubmitResponse { id, name })
    }

    /// Handle get status request.
    async fn handle_get_status(&self, id: String) -> Result<StatusResponse> {
        let workflow = sqlx::query_as(
            "select id, name, status, wdl_source_type, wdl_source_value, inputs, outputs, error,
                    created_at, started_at, completed_at
             from workflows
             where id = ?"
        )
        .bind(&id)
        .fetch_optional(self.db.pool())
        .await
        .context("failed to query workflow")?
        .ok_or_else(|| anyhow::anyhow!("workflow not found"))?;

        Ok(StatusResponse { workflow })
    }

    /// Handle list request.
    async fn handle_list(
        &self,
        status: Option<WorkflowStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<ListResponse> {
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);

        let workflows = if let Some(status) = status {
            sqlx::query_as(
                "select id, name, status, wdl_source_type, wdl_source_value, inputs, outputs, error,
                        created_at, started_at, completed_at
                 from workflows
                 where status = ?
                 order by created_at desc
                 limit ? offset ?"
            )
            .bind(status)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.pool())
            .await
            .context("failed to query workflows")?
        } else {
            sqlx::query_as(
                "select id, name, status, wdl_source_type, wdl_source_value, inputs, outputs, error,
                        created_at, started_at, completed_at
                 from workflows
                 order by created_at desc
                 limit ? offset ?"
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.pool())
            .await
            .context("failed to query workflows")?
        };

        let total: (i64,) = if let Some(status) = status {
            sqlx::query_as("select count(*) from workflows where status = ?")
                .bind(status)
                .fetch_one(self.db.pool())
                .await
                .context("failed to count workflows")?
        } else {
            sqlx::query_as("select count(*) from workflows")
                .fetch_one(self.db.pool())
                .await
                .context("failed to count workflows")?
        };

        Ok(ListResponse {
            workflows,
            total: total.0,
        })
    }

    /// Handle cancel request.
    async fn handle_cancel(&mut self, id: String) -> Result<CancelResponse> {
        // Check if workflow exists.
        let status: Option<(WorkflowStatus,)> = sqlx::query_as("select status from workflows where id = ?")
            .bind(&id)
            .fetch_optional(self.db.pool())
            .await
            .context("failed to query workflow")?;

        let current_status = status
            .ok_or_else(|| anyhow::anyhow!("workflow not found"))?
            .0;

        // Only running or queued workflows can be cancelled.
        if !matches!(current_status, WorkflowStatus::Running | WorkflowStatus::Queued) {
            anyhow::bail!("workflow cannot be cancelled (status: `{:?}`)", current_status);
        }

        // Cancel the task if it's running.
        if let Some(task) = self.workflows.remove(&id) {
            task.abort();
        }

        // Update status in database.
        sqlx::query("update workflows set status = ?, completed_at = current_timestamp where id = ?")
            .bind(WorkflowStatus::Cancelled)
            .bind(&id)
            .execute(self.db.pool())
            .await
            .context("failed to update workflow status")?;

        Ok(CancelResponse { id })
    }

    /// Handle get outputs request.
    async fn handle_get_outputs(&self, id: String) -> Result<OutputsResponse> {
        let outputs: Option<(Option<serde_json::Value>,)> =
            sqlx::query_as("select outputs from workflows where id = ?")
                .bind(&id)
                .fetch_optional(self.db.pool())
                .await
                .context("failed to query workflow outputs")?;

        let outputs = outputs
            .ok_or_else(|| anyhow::anyhow!("workflow not found"))?
            .0;

        Ok(OutputsResponse { outputs })
    }

    /// Handle get logs request.
    async fn handle_get_logs(
        &self,
        id: String,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<LogsResponse> {
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);

        // Check if workflow exists.
        let exists: Option<(i64,)> = sqlx::query_as("select 1 from workflows where id = ?")
            .bind(&id)
            .fetch_optional(self.db.pool())
            .await
            .context("failed to query workflow")?;

        if exists.is_none() {
            anyhow::bail!("workflow not found");
        }

        let logs: Vec<(String,)> = sqlx::query_as(
            "select message from logs where workflow_id = ? order by created_at asc limit ? offset ?"
        )
        .bind(&id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.db.pool())
        .await
        .context("failed to query logs")?;

        let total: (i64,) = sqlx::query_as("select count(*) from logs where workflow_id = ?")
            .bind(&id)
            .fetch_one(self.db.pool())
            .await
            .context("failed to count logs")?;

        Ok(LogsResponse {
            logs: logs.into_iter().map(|l| l.0).collect(),
            total: total.0,
        })
    }
}

/// Create a new workflow manager handle.
pub fn spawn_manager(config: Config, db: Database) -> mpsc::Sender<ManagerCommand> {
    let (tx, rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);
    let manager = WorkflowManager::new(config, db, rx);
    tokio::spawn(manager.run());
    tx
}

/// Execute a workflow.
async fn execute_workflow(
    workflow_id: &str,
    source: &WdlSource,
    inputs: &serde_json::Value,
    exec_dir: &std::path::Path,
    db: &Database,
) -> Result<()> {
    use super::helpers::*;
    use url::Url;
    use wdl::analysis::Analyzer;
    use wdl::analysis::Config as AnalysisConfig;
    use wdl::ast::Severity;
    use wdl::engine::Events;
    use wdl::engine::Inputs;
    use wdl::engine::v1::WorkflowEvaluator;

    // Update status to running.
    update_workflow_running(workflow_id, db).await?;

    // Analyze the WDL document.
    let analyzer = Analyzer::new(AnalysisConfig::default(), |(), _, _, _| async {});

    let uri = match source {
        WdlSource::Content(content) => {
            // Write content to a file in the execution directory.
            let wdl_file = exec_dir.join("workflow.wdl");
            std::fs::write(&wdl_file, content)
                .context("failed to write WDL content to file")?;
            Url::from_file_path(&wdl_file)
                .map_err(|()| anyhow::anyhow!("failed to convert path to URL"))?
        }
        WdlSource::File(path) => {
            Url::from_file_path(path)
                .map_err(|()| anyhow::anyhow!("failed to convert path to URL"))?
        }
    };

    analyzer
        .add_document(uri)
        .await
        .context("failed to add document")?;

    let results = analyzer
        .analyze(())
        .await
        .context("failed to analyze document")?;

    let result = results
        .first()
        .context("no analysis results")?;

    if let Some(e) = result.error() {
        let error_msg = format!("parsing failed: {:#}", e);
        update_workflow_failed(workflow_id, &error_msg, db).await?;
        anyhow::bail!(error_msg);
    }

    // Check for errors in the document.
    let diagnostics: Vec<_> = result.document().diagnostics().cloned().collect();
    if let Some(diagnostic) = diagnostics.iter().find(|d| d.severity() == Severity::Error) {
        let error_msg = format!("{:?}", diagnostic);
        update_workflow_failed(workflow_id, &error_msg, db).await?;
        anyhow::bail!(error_msg);
    }

    // Get the workflow.
    // TODO(clay): support task execution in addition to workflows.
    let workflow = result
        .document()
        .workflow()
        .context("document does not contain a workflow")?;

    // Parse inputs.
    // TODO(clay): support task inputs in addition to workflow inputs.
    let workflow_inputs = if inputs.is_null() || inputs.as_object().map_or(false, |o| o.is_empty()) {
        Default::default()
    } else {
        // Write inputs to a file in the execution directory.
        let inputs_file = exec_dir.join("inputs.json");
        let inputs_with_name = serde_json::json!({
            workflow.name(): inputs
        });
        std::fs::write(&inputs_file, serde_json::to_string_pretty(&inputs_with_name)?)
            .context("failed to write inputs file")?;

        match Inputs::parse(result.document(), &inputs_file)? {
            Some((_, Inputs::Task(_))) => {
                let error_msg = "inputs are for a task, not a workflow";
                update_workflow_failed(workflow_id, error_msg, db).await?;
                anyhow::bail!(error_msg);
            }
            Some((_, Inputs::Workflow(inputs))) => inputs,
            None => Default::default(),
        }
    };

    // Execute the workflow.
    // TODO(clay): support task execution using `TaskEvaluator`.
    let cancellation = wdl::engine::CancellationContext::new(wdl::engine::config::FailureMode::Slow);
    let evaluator = WorkflowEvaluator::new(
        Default::default(),
        cancellation,
        Events::none(),
    )
    .await
    .context("failed to create workflow evaluator")?;

    match evaluator
        .evaluate(result.document(), workflow_inputs, exec_dir)
        .await
    {
        Ok(outputs) => {
            // Serialize outputs.
            let outputs_with_name = outputs.with_name(workflow.name());
            let outputs_json = serde_json::to_value(&outputs_with_name)
                .context("failed to serialize workflow outputs")?;

            update_workflow_completed(workflow_id, &outputs_json, db).await?;

            info!("workflow `{}` completed successfully", workflow_id);
            Ok(())
        }
        Err(e) => {
            let error_msg = format!("{:#?}", e);
            update_workflow_failed(workflow_id, &error_msg, db).await?;
            anyhow::bail!(error_msg);
        }
    }
}
