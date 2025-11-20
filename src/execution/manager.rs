//! Run manager actor implementation.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::debug;
use tracing::info;
use uuid::Uuid;
use wdl::analysis::Document;
use wdl::engine::CancellationContextState;

use crate::Database;
use crate::OutputDirectory;
use crate::database::DatabaseError;
use crate::database::InvocationMethod;
use crate::database::RunStatus;
use crate::execution::AllowedSource;
use crate::execution::ConfigError;
use crate::execution::ExecutionConfig;
use crate::execution::RunContext;
use crate::execution::commands::*;
use crate::execution::names::generate_run_name;

/// Manager errors.
#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    /// Run not found in database.
    #[error("run not found")]
    RunNotFound,

    /// Target workflow or task not found in document.
    #[error("target `{0}` not found in document")]
    TargetNotFound(String),

    /// Target required but not provided (ambiguous document).
    #[error("target required: document contains no workflow and multiple tasks")]
    TargetRequired,

    /// Document does not contain any executable target.
    #[error("document contains no workflows or tasks")]
    NoExecutableTarget,

    /// WDL analysis or validation failed.
    #[error("{0}")]
    Analysis(anyhow::Error),

    /// Configuration validation error.
    #[error("{0}")]
    Config(#[from] ConfigError),

    /// Run cannot be cancelled in current state.
    #[error("run cannot be cancelled in `{0}` state")]
    CannotCancel(RunStatus),

    /// Database operation failed.
    #[error(transparent)]
    Database(#[from] DatabaseError),

    /// An I/O error occurred.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Result type for manager operations.
pub type ManagerResult<T> = std::result::Result<T, ManagerError>;

/// Channel buffer size for manager commands.
const CHANNEL_BUFFER_SIZE: usize = 200;

/// The name for the "latest" symlink.
const LATEST: &str = "_latest";

/// Information about a running run task.
#[derive(Debug)]
struct RunHandle {
    /// The tokio task handle.
    #[allow(dead_code)]
    handle: JoinHandle<()>,
    /// The run execution context.
    context: crate::execution::RunContext,
    /// The cancellation context for this run.
    cancellation: wdl::engine::CancellationContext,
}

/// Run manager actor.
#[derive(Debug)]
pub struct RunManager {
    /// Execution configuration.
    config: ExecutionConfig,
    /// Output directory.
    output_dir: OutputDirectory,
    /// Database handle.
    db: Arc<dyn Database>,
    /// Invocation ID for this server instance.
    ///
    /// This field is `None` until the first run is submitted, at which
    /// point it is lazily created and persisted to the database.
    invocation_id: Option<Uuid>,
    /// Command receiver.
    rx: mpsc::Receiver<ManagerCommand>,
    /// Running run tasks.
    runs: HashMap<Uuid, RunHandle>,
    /// Semaphore for limiting concurrent runs.
    semaphore: Option<Arc<Semaphore>>,
    /// Events for progress reporting.
    events: wdl::engine::Events,
}

impl RunManager {
    /// Create a new run manager.
    pub fn new(
        config: ExecutionConfig,
        db: Arc<dyn Database>,
        rx: mpsc::Receiver<ManagerCommand>,
        events: wdl::engine::Events,
    ) -> Self {
        let semaphore = config
            .max_concurrent_runs
            .map(|max| Arc::new(Semaphore::new(max)));

        let output_dir = OutputDirectory::new(&config.output_directory);

        Self {
            config,
            output_dir,
            db,
            invocation_id: None,
            rx,
            runs: HashMap::new(),
            semaphore,
            events,
        }
    }

    /// Run the manager event loop.
    pub async fn run(mut self) {
        info!("run manager started");
        info!("allowed file paths: {:?}", self.config.allowed_file_paths);
        info!("allowed urls: {:?}", self.config.allowed_urls);

        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                ManagerCommand::Ping { rx } => {
                    debug!("received `Ping` command");
                    let _ = rx.send(Ok(()));
                }
                ManagerCommand::Submit {
                    source,
                    inputs,
                    target,
                    index_on,
                    rx,
                } => {
                    debug!(
                        ?source,
                        ?inputs,
                        ?target,
                        ?index_on,
                        "received `Submit` command"
                    );
                    let result = self
                        .handle_submit(source, self.config.engine.clone(), inputs, target, index_on)
                        .await;
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
                ManagerCommand::GetInvocation { id, rx } => {
                    debug!(?id, "received `GetInvocation` command");
                    let result = self.handle_get_invocation(id).await;
                    let _ = rx.send(result);
                }
                ManagerCommand::ListInvocations { limit, offset, rx } => {
                    debug!(?limit, ?offset, "received `ListInvocations` command");
                    let result = self.handle_list_invocations(limit, offset).await;
                    let _ = rx.send(result);
                }
                ManagerCommand::Shutdown { rx } => {
                    debug!("received `Shutdown` command");
                    info!("run manager shutting down");
                    drop(self.events);
                    let _ = rx.send(Ok(()));
                    break;
                }
            }
        }

        info!("run manager stopped");
    }

    /// Select the target workflow or task to execute from the document.
    ///
    /// The priority is set as follows:
    ///
    /// 1. If target provided, find workflow or task with that name
    /// 2. If no target, use workflow if present
    /// 3. If no target and no workflow, use single task if exactly one exists
    /// 4. Otherwise error
    fn select_target(document: &Document, target: Option<&str>) -> ManagerResult<String> {
        if let Some(target_name) = target {
            // Explicit target provided, find workflow or task by name
            if let Some(workflow) = document.workflow()
                && workflow.name() == target_name
            {
                return Ok(workflow.name().to_string());
            }

            if document.task_by_name(target_name).is_some() {
                return Ok(target_name.to_string());
            }

            Err(ManagerError::TargetNotFound(target_name.to_string()))
        } else {
            // No target provided, try to infer
            if let Some(workflow) = document.workflow() {
                // Document has a workflow, use it
                Ok(workflow.name().to_string())
            } else {
                // No workflow, check tasks
                let tasks: Vec<_> = document.tasks().collect();
                match tasks.len() {
                    0 => Err(ManagerError::NoExecutableTarget),
                    1 => Ok(tasks[0].name().to_string()),
                    _ => Err(ManagerError::TargetRequired),
                }
            }
        }
    }

    /// Handle run submission.
    async fn handle_submit(
        &mut self,
        source: String,
        config: wdl::engine::Config,
        inputs: serde_json::Value,
        target: Option<String>,
        index_on: Option<String>,
    ) -> ManagerResult<SubmitResponse> {
        // Validate the source
        let source = AllowedSource::validate(&source, &self.config)?;

        // Analyze and validate WDL document
        let analysis_result = crate::execution::analyze_wdl_document(&source)
            .await
            .map_err(ManagerError::Analysis)?;

        let document = analysis_result.document();

        // Select target workflow or task to execute
        let target_name = Self::select_target(document, target.as_deref())?;

        // Validate analysis results
        crate::execution::validate_analysis_results(&analysis_result)
            .await
            .map_err(ManagerError::Analysis)?;

        // Lazily create invocation on first run submission.
        let invocation_id = if let Some(id) = self.invocation_id {
            id
        } else {
            let id = Uuid::new_v4();
            let username = whoami::username();
            self.db
                .create_invocation(id, InvocationMethod::Server, &username)
                .await?;
            self.invocation_id = Some(id);
            id
        };

        // Generate run name and id.
        let run_id = Uuid::new_v4();
        let run_generated_name = generate_run_name();

        // Create run directory.
        let timestamp = Utc::now();
        let run_dir_name = format!("{}/{}", target_name, timestamp.format("%F_%H%M%S%f"));
        let run_dir = self
            .output_dir
            .ensure_workflow_run(&run_dir_name)
            .map_err(ManagerError::from)?;

        // Create `_latest` symlink.
        // SAFETY: we know that the `runs/` directory should be the parent here.
        let parent = run_dir.root().parent().unwrap();
        let latest = parent.join(LATEST);
        let _ = std::fs::remove_file(&latest);

        if let Some(basename) = run_dir.root().file_name() {
            #[cfg(unix)]
            let result = std::os::unix::fs::symlink(basename, &latest);

            #[cfg(windows)]
            let result = std::os::windows::fs::symlink_dir(basename, &latest);

            if let Err(e) = result {
                tracing::trace!(
                    "failed to create `_latest` symlink at `{}`: {}",
                    latest.display(),
                    e
                );
            }
        }

        // Create the run database entry
        self.db
            .create_run(
                run_id,
                invocation_id,
                &run_generated_name,
                source.as_str(),
                &inputs.to_string(),
                run_dir.relative_path().to_str().expect("path is not UTF-8"),
            )
            .await?;

        // Create run context
        let started_at = Utc::now();
        let ctx = RunContext {
            run_id,
            run_generated_name: run_generated_name.clone(),
            started_at,
        };

        // Create cancellation context from engine config
        let cancellation = wdl::engine::CancellationContext::new(config.failure_mode);

        // Spawn run execution task
        let semaphore = self.semaphore.clone();
        let db = self.db.clone();
        let ctx_clone = ctx.clone();
        let target_name_clone = target_name.clone();
        let document = analysis_result.document().clone();
        let cancellation_clone = cancellation.clone();
        let events = self.events.clone();
        let handle = tokio::spawn(async move {
            // Acquire semaphore permit if concurrency limit is set
            let _permit = if let Some(ref sem) = semaphore {
                // SAFETY: the semaphore is Arc-wrapped and held by the manager for its
                // entire lifetime. It is never explicitly closed. If this fails, it
                // indicates a catastrophic programming error (e.g., memory corruption),
                // and panicking to fail-fast is appropriate.
                Some(sem.acquire().await.expect("semaphore closed"))
            } else {
                None
            };

            info!(
                "run `{}` ({}) execution started",
                &ctx_clone.run_generated_name, run_id
            );

            // Execute the task or workflow
            if let Err(e) = crate::execution::execute_task_or_workflow(
                db,
                &ctx_clone,
                document,
                config,
                cancellation_clone,
                events,
                &target_name_clone,
                &inputs,
                &run_dir,
                index_on.as_deref(),
            )
            .await
            {
                tracing::error!(
                    "run `{}` ({}) failed: {}",
                    &ctx_clone.run_generated_name,
                    run_id,
                    e
                );
            }
        });

        self.runs.insert(
            run_id,
            RunHandle {
                handle,
                context: ctx,
                cancellation,
            },
        );

        Ok(SubmitResponse {
            id: run_id,
            name: run_generated_name,
        })
    }

    /// Handle get status request.
    async fn handle_get_status(&self, id: Uuid) -> ManagerResult<StatusResponse> {
        let run = self
            .db
            .get_run(id)
            .await?
            .ok_or(ManagerError::RunNotFound)?;

        Ok(StatusResponse { run })
    }

    /// Handle list request.
    async fn handle_list(
        &self,
        status: Option<RunStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> ManagerResult<ListResponse> {
        let runs = self.db.list_runs(status, limit, offset).await?;
        let total = self.db.count_runs(status).await?;
        Ok(ListResponse { runs, total })
    }

    /// Handle cancel request.
    async fn handle_cancel(&mut self, id: Uuid) -> ManagerResult<CancelResponse> {
        // Check if run exists.
        let run = self
            .db
            .get_run(id)
            .await?
            .ok_or(ManagerError::RunNotFound)?;

        // Only running, queued, or canceling runs can be cancelled.
        if !matches!(
            run.status,
            RunStatus::Running | RunStatus::Queued | RunStatus::Canceling
        ) {
            return Err(ManagerError::CannotCancel(run.status));
        }

        if let Some(task) = self.runs.get(&id) {
            let state = task.cancellation.cancel();

            // Map cancellation context state to database status
            let db_status = match state {
                CancellationContextState::NotCanceled => {
                    unreachable!("`cancel()` should always transition to a canceled state")
                }
                CancellationContextState::Waiting => RunStatus::Canceling,
                CancellationContextState::Canceling => RunStatus::Canceled,
            };

            self.db
                .update_run_status(id, db_status, Some(task.context.started_at), None)
                .await?;

            if db_status == RunStatus::Canceled {
                self.runs.remove(&id);
            }
        }

        Ok(CancelResponse { id })
    }

    /// Handle get outputs request.
    async fn handle_get_outputs(&self, id: Uuid) -> ManagerResult<OutputsResponse> {
        let run = self
            .db
            .get_run(id)
            .await?
            .ok_or(ManagerError::RunNotFound)?;

        let outputs = run
            .outputs
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok());

        Ok(OutputsResponse { outputs })
    }

    /// Handle getting invocation by ID.
    async fn handle_get_invocation(&self, id: Uuid) -> ManagerResult<InvocationResponse> {
        let invocation = self
            .db
            .get_invocation(id)
            .await?
            .ok_or(ManagerError::RunNotFound)?;

        Ok(InvocationResponse { invocation })
    }

    /// Handle listing invocations.
    async fn handle_list_invocations(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> ManagerResult<ListInvocationsResponse> {
        let invocations = self.db.list_invocations(limit, offset).await?;
        Ok(ListInvocationsResponse { invocations })
    }
}

/// Create a new run manager handle.
pub fn spawn_manager(
    config: ExecutionConfig,
    db: Arc<dyn Database>,
    events: wdl::engine::Events,
) -> mpsc::Sender<ManagerCommand> {
    let (tx, rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);
    let manager = RunManager::new(config, db, rx, events);
    tokio::spawn(manager.run());
    tx
}
