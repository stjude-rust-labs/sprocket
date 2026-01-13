//! The run manager service.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use serde_json::Value as JsonValue;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::info;
use tracing::trace;
use uuid::Uuid;
use wdl::engine::CancellationContext;
use wdl::engine::CancellationContextState;
use wdl::engine::Config as EngineConfig;
use wdl::engine::Events;

use crate::system::v1::db::Database;
use crate::system::v1::fs::OutputDirectory;
use crate::system::v1::db::DatabaseError;
use crate::system::v1::db::Session;
use crate::system::v1::db::SprocketCommand;
use crate::system::v1::db::LogSource;
use crate::system::v1::db::RunStatus;
use crate::system::v1::db::TaskStatus;
use crate::system::v1::exec::AllowedSource;
use crate::system::v1::exec::ConfigError;
use crate::system::v1::exec::ExecutionConfig;
use crate::system::v1::exec::RunContext;
use crate::system::v1::exec::names::generate_run_name;
use crate::system::v1::exec::svc::TaskMonitorSvc;

pub(crate) mod commands;

pub use commands::*;

/// Channel capacity for events.
///
/// This number represents a reasonable, arbitrary buffer size to handle burst
/// event production.
const EVENTS_CHANNEL_CAPACITY: usize = 2048;

/// The name for the "latest" symlink.
const LATEST: &str = "_latest";

/// A receiver for commands issued to the run manager service.
type Rx = mpsc::Receiver<RunManagerCmd>;

/// Creates an session entry in the database for a server.
async fn create_server_session(db: Arc<dyn Database>) -> Result<Session, DatabaseError> {
    let id = Uuid::new_v4();
    let username = whoami::username();
    db
        .create_session(id, SprocketCommand::Server, &username)
        .await
}

/// The run manager service.
///
/// The run manager service is an actor that executes WDL tasks and workflows
/// using all of the conventions of Sprocket (e.g., instantiating a run
/// directory, indexing desired outputs, etc). It is the main entrypoint to WDL
/// evaluation in Sprocket.
#[allow(missing_debug_implementations)]
pub struct RunManagerSvc {
    /// The configuration for execution.
    config: ExecutionConfig,
    /// The output directory root.
    output_dir: OutputDirectory,
    /// A handle to the database.
    db: Arc<dyn Database>,
    /// Session ID for this server instance.
    ///
    /// This field keeps track of which session entry in the database this
    /// manager service is associated with.
    ///
    /// The field is `None` until the first run is submitted, at which point it
    /// is lazily created and persisted to the database.
    session_id: Option<Uuid>,
    /// The receiver for commands.
    rx: Rx,
    /// A semaphore for limiting concurrent runs.
    semaphore: Option<Arc<Semaphore>>,
    /// A mapping of runs to cancellation contexts.
    ///
    /// A [`tokio::sync::Mutex`] is used because the [`run()`][Self::run] future
    /// must be `Send`.
    runs: Arc<Mutex<HashMap<Uuid, CancellationContext>>>,
}

impl RunManagerSvc {
    /// Create a new run manager.
    pub fn new(config: ExecutionConfig, db: Arc<dyn Database>, rx: Rx) -> Self {
        let semaphore = config
            .max_concurrent_runs
            .map(|max| Arc::new(Semaphore::new(max)));

        let output_dir = OutputDirectory::new(&config.output_directory);

        Self {
            config,
            output_dir,
            db,
            // NOTE: this is empty upon creation, but it's created lazily upon
            // the first run.
            session_id: None,
            rx,
            semaphore,
            runs: Default::default(),
        }
    }

    /// Runs the event loop.
    pub async fn run(mut self) {
        info!("run manager service started");
        info!("allowed file paths: {:?}", self.config.allowed_file_paths);
        info!("allowed urls: {:?}", self.config.allowed_urls);

        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                RunManagerCmd::Ping { rx } => {
                    trace!("received `Ping` command");
                    let _ = rx.send(Ok(()));
                }
                RunManagerCmd::Submit {
                    source,
                    inputs,
                    target,
                    index_on,
                    rx,
                } => {
                    trace!(
                        ?source,
                        ?inputs,
                        ?target,
                        ?index_on,
                        "received `Submit` command"
                    );

                    // Lazily create session on first run submission.
                    let session_id = if let Some(id) = self.session_id {
                        id
                    } else {
                        match create_server_session(self.db.clone()).await {
                            Ok(session) => {
                                let id = session.id;
                                self.session_id = Some(id);
                                id
                            }
                            Err(e) => {
                                let _ = rx.send(Err(SubmitRunError::Database(e)));
                                continue;
                            }
                        }
                    };

                    let result = submit_run(
                        self.db.clone(),
                        &self.output_dir,
                        &self.config,
                        self.semaphore.clone(),
                        self.runs.clone(),
                        session_id,
                        source,
                        self.config.engine.clone(),
                        inputs,
                        target,
                        index_on,
                    )
                    .await;
                    let _ = rx.send(result);
                }
                RunManagerCmd::GetStatus { id, rx } => {
                    trace!(?id, "received `GetStatus` command");
                    let result = get_run(&self.db, id).await;
                    let _ = rx.send(result);
                }
                RunManagerCmd::List {
                    status,
                    limit,
                    offset,
                    rx,
                } => {
                    trace!(?status, ?limit, ?offset, "received `List` command");
                    let result = list_runs(&self.db, status, limit, offset).await;
                    let _ = rx.send(result);
                }
                RunManagerCmd::Cancel { id, rx } => {
                    trace!(?id, "received `Cancel` command");
                    let result = cancel_run(&self.db, &self.runs, id).await;
                    let _ = rx.send(result);
                }
                RunManagerCmd::GetOutputs { id, rx } => {
                    trace!(?id, "received `GetOutputs` command");
                    let result = get_run_outputs(&self.db, id).await;
                    let _ = rx.send(result);
                }
                RunManagerCmd::GetSession { id, rx } => {
                    trace!(?id, "received `GetSession` command");
                    let result = get_session_for_run(&self.db, id).await;
                    let _ = rx.send(result);
                }
                RunManagerCmd::ListSessions { limit, offset, rx } => {
                    trace!(?limit, ?offset, "received `ListSessions` command");
                    let result = list_sessions(&self.db, limit, offset).await;
                    let _ = rx.send(result);
                }
                RunManagerCmd::ListTasks {
                    run_id,
                    status,
                    limit,
                    offset,
                    rx,
                } => {
                    trace!(
                        ?run_id,
                        ?status,
                        ?limit,
                        ?offset,
                        "received `ListTasks` command"
                    );
                    let result = list_tasks(&self.db, run_id, status, limit, offset).await;
                    let _ = rx.send(result);
                }
                RunManagerCmd::GetTask { name, rx } => {
                    trace!(?name, "received `GetTask` command");
                    let result = get_task(&self.db, name).await;
                    let _ = rx.send(result);
                }
                RunManagerCmd::GetTaskLogs {
                    name,
                    stream,
                    limit,
                    offset,
                    rx,
                } => {
                    trace!(
                        ?name,
                        ?stream,
                        ?limit,
                        ?offset,
                        "received `GetTaskLogs` command"
                    );
                    let result = get_task_logs(&self.db, name, stream, limit, offset).await;
                    let _ = rx.send(result);
                }
                RunManagerCmd::Shutdown { rx } => {
                    trace!("received `Shutdown` command");
                    info!("run manager service is shutting down");
                    let _ = rx.send(Ok(()));
                    break;
                }
            }
        }

        info!("run manager service stopped");
    }

    /// Spawns a new run manager service and returns:
    ///
    /// - the join handle of the event loop, and
    /// - the sender channel
    pub fn spawn(
        channel_buffer_size: usize,
        config: ExecutionConfig,
        db: Arc<dyn Database>,
    ) -> (JoinHandle<()>, mpsc::Sender<RunManagerCmd>) {
        let (tx, rx) = mpsc::channel(channel_buffer_size);
        let manager = Self::new(config, db, rx);
        let handle = tokio::spawn(manager.run());
        (handle, tx)
    }
}

/// Error type for submitting a run.
#[derive(Debug, Error)]
pub enum SubmitRunError {
    /// Configuration error.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// Analysis error.
    #[error("{0}")]
    Analysis(#[source] anyhow::Error),
    /// Target selection error.
    #[error(transparent)]
    TargetSelection(#[from] crate::system::v1::exec::SelectTargetError),
    /// Database error.
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Submits a new run for execution.
#[allow(clippy::too_many_arguments)]
async fn submit_run(
    db: Arc<dyn Database>,
    output_dir: &OutputDirectory,
    execution_config: &ExecutionConfig,
    semaphore: Option<Arc<Semaphore>>,
    runs: Arc<Mutex<HashMap<Uuid, CancellationContext>>>,
    session_id: Uuid,
    source: String,
    engine_config: EngineConfig,
    inputs: JsonValue,
    target: Option<String>,
    index_on: Option<String>,
) -> Result<SubmitResponse, SubmitRunError> {
    // Ensure the source is allowed
    let source = AllowedSource::validate(&source, execution_config)?;

    // Analyze the document and validate the result
    let result = crate::system::v1::exec::analyze_wdl_document(&source)
        .await
        .map_err(SubmitRunError::Analysis)?;
    crate::system::v1::exec::validate_analysis_results(&result)
        .await
        .map_err(SubmitRunError::Analysis)?;
    let document = result.document();

    // Select target workflow or task to execute
    let target = crate::system::v1::exec::select_target(document, target.as_deref())?;

    // Generate run name and id
    let run_id = Uuid::new_v4();
    let run_generated_name = generate_run_name();

    // Create run directory
    let run_dir_name =
        PathBuf::from(target.name()).join(format!("{}", Utc::now().format("%F_%H%M%S%f")));
    let run_dir = output_dir.ensure_workflow_run(run_dir_name)?;

    // Create `_latest` symlink
    // SAFETY: we know that the `runs/` directory should be the parent here.
    let run_dir_parent = run_dir.root().parent().unwrap();
    let latest_symlink = run_dir_parent.join(LATEST);
    let _ = std::fs::remove_file(&latest_symlink);

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

    // Create the run database entry
    db.create_run(
        run_id,
        session_id,
        &run_generated_name,
        source.as_str(),
        &inputs.to_string(),
        run_dir.relative_path().to_str().expect("path is not UTF-8"),
    )
    .await?;

    // Construct the run context
    let ctx = RunContext {
        run_id,
        run_generated_name: run_generated_name.clone(),
        // NOTE: this will be used to update the `started_at` field in the
        // database for this run.
        started_at: Utc::now(),
    };

    // Create cancellation context
    let cancellation = CancellationContext::new(engine_config.failure_mode);

    // Subscribe to all events during evaluation
    let events = Events::new(EVENTS_CHANNEL_CAPACITY);

    // Spawn run execution task
    let async_semaphore = semaphore;
    let async_db = db;
    let async_ctx = ctx.clone();
    let async_target = target.clone();
    let async_document = result.document().clone();
    let async_cancellation = cancellation.clone();
    let async_runs = runs.clone();
    let async_events = events.clone();

    let handle = tokio::spawn(async move {
        // Acquire semaphore permit if concurrency limit is set
        let _ = if let Some(ref sem) = async_semaphore {
            // SAFETY: the semaphore is Arc-wrapped and held by the manager for its
            // entire lifetime. It is never explicitly closed. If this fails, it
            // indicates a catastrophic programming error.
            Some(sem.acquire().await.expect("semaphore closed"))
        } else {
            None
        };

        info!(
            "run `{}` ({}) execution started",
            &async_ctx.run_generated_name, run_id
        );

        // Spawn task monitor if crankshaft events are enabled
        //
        // SAFETY: because we subscribe to all events above, the Crankshaft
        // subscriber should always be available to us here.
        let crankshaft_rx = async_events.subscribe_crankshaft().unwrap();
        let task_monitor_svc = TaskMonitorSvc::new(run_id, async_db.clone(), crankshaft_rx);
        // NOTE: the monitor service closes itself when the events sender is
        // dropped, so it will clean itself up—no need to keep this handle.
        tokio::spawn(task_monitor_svc.run());

        // Execute the target
        if let Err(e) = crate::system::v1::exec::execute_target(
            async_db,
            &async_ctx,
            async_document,
            engine_config,
            async_cancellation,
            async_events,
            async_target,
            &inputs,
            &run_dir,
            index_on.as_deref(),
        )
        .await
        {
            tracing::error!(
                "run `{}` ({}) failed: {}",
                &async_ctx.run_generated_name,
                run_id,
                e
            );
        }

        // Remove the run from the run map
        async_runs.lock().await.remove(&run_id);
    });

    runs.lock().await.insert(run_id, cancellation);

    Ok(SubmitResponse {
        id: run_id,
        name: run_generated_name,
        events,
        handle,
    })
}

/// Error type for getting a run.
#[derive(Debug, Error)]
pub enum GetRunError {
    /// Database error.
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// Run not found.
    #[error("run not found: `{0}`")]
    NotFound(Uuid),
}

/// Gets a run by ID.
async fn get_run(db: &Arc<dyn Database>, id: Uuid) -> Result<RunResponse, GetRunError> {
    let run = db.get_run(id).await?;
    match run {
        Some(run) => Ok(RunResponse { run }),
        None => Err(GetRunError::NotFound(id)),
    }
}

/// Lists all runs given the filter criteria.
async fn list_runs(
    db: &Arc<dyn Database>,
    status: Option<RunStatus>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<ListRunsResponse, DatabaseError> {
    let runs = db.list_runs(status, limit, offset).await?;
    let total = db.count_runs(status).await?;
    Ok(ListRunsResponse { runs, total })
}

/// Error type for canceling a run.
#[derive(Debug, Error)]
pub enum CancelRunError {
    /// Database error.
    #[error("database error: {0}")]
    Database(#[from] crate::system::v1::db::DatabaseError),
    /// Run not found.
    #[error("run not found: `{0}`")]
    NotFound(Uuid),
    /// Invalid status for cancellation.
    #[error(
        "only running, queued, or canceling runs can be canceled; run `{id}` has status `{status}`"
    )]
    InvalidStatus {
        /// The run ID.
        id: Uuid,
        /// The current status.
        status: RunStatus,
    },
}

/// Attempts to cancel a run that is in progress.
async fn cancel_run(
    db: &Arc<dyn Database>,
    runs: &Arc<Mutex<HashMap<Uuid, CancellationContext>>>,
    id: Uuid,
) -> Result<CancelRunResponse, CancelRunError> {
    let run = db.get_run(id).await?.ok_or(CancelRunError::NotFound(id))?;

    if !matches!(
        run.status,
        RunStatus::Running | RunStatus::Queued | RunStatus::Canceling
    ) {
        return Err(CancelRunError::InvalidStatus {
            id,
            status: run.status,
        });
    }

    let mut runs_guard = runs.lock().await;

    if let Some(ctx) = runs_guard.get(&id) {
        let state = ctx.cancel();

        match state {
            CancellationContextState::NotCanceled => {
                unreachable!("calling `cancel()` should always transition to a canceled state")
            }
            // Getting a `Waiting` state means that we're in lazy
            // cancellation mode. In this case, we should report to the
            // database that we're in the process of canceling
            // (`Canceling`).
            CancellationContextState::Waiting => {
                db.update_run_status(id, RunStatus::Canceling).await?;
            }
            // If we we `Canceling` back from the call, that means the task
            // is being actively canceled. As such, we can mark it as
            // `Canceled` in the database.
            CancellationContextState::Canceling => {
                db.cancel_run(id, Utc::now()).await?;
                // NOTE: when a run is actually canceled, remove it from the runs
                // map, as it won't remove itself at the end of execution.
                runs_guard.remove(&id);
            }
        }
    }

    Ok(CancelRunResponse { id })
}

/// Error type for getting run outputs.
#[derive(Debug, Error)]
pub enum GetRunOutputsError {
    /// Database error.
    #[error("database error: {0}")]
    Database(#[from] crate::system::v1::db::DatabaseError),
    /// Run not found.
    #[error("the run with id `{0}` was not found")]
    NotFound(Uuid),
}

/// Attempts to get the outputs for a run.
async fn get_run_outputs(
    db: &Arc<dyn Database>,
    id: Uuid,
) -> Result<RunOutputsResponse, GetRunOutputsError> {
    let run = db
        .get_run(id)
        .await?
        .ok_or(GetRunOutputsError::NotFound(id))?;

    let outputs = run
        .outputs
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok());

    Ok(RunOutputsResponse { outputs })
}

/// Gets all sessions given the filter criteria.
async fn list_sessions(
    db: &Arc<dyn Database>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<ListSessionsResponse, DatabaseError> {
    let sessions = db.list_sessions(limit, offset).await?;
    Ok(ListSessionsResponse { sessions })
}

/// Error type for getting an session.
#[derive(Debug, Error)]
pub enum GetSessionError {
    /// Database error.
    #[error("database error: {0}")]
    Database(#[from] crate::system::v1::db::DatabaseError),
    /// Session not found.
    #[error("the run with id `{0}` was not found")]
    NotFound(Uuid),
}

/// Gets the session entry associated with a run.
async fn get_session_for_run(
    db: &Arc<dyn Database>,
    id: Uuid,
) -> Result<SessionResponse, GetSessionError> {
    let session = db
        .get_session(id)
        .await?
        .ok_or(GetSessionError::NotFound(id))?;

    Ok(SessionResponse { session })
}

/// Gets all tasks given the filter criteria.
async fn list_tasks(
    db: &Arc<dyn Database>,
    run_id: Option<Uuid>,
    status: Option<TaskStatus>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<ListTasksResponse, DatabaseError> {
    let tasks = db.list_tasks(run_id, status, limit, offset).await?;
    let total = db.count_tasks(run_id, status).await?;
    Ok(ListTasksResponse { tasks, total })
}

/// Gets a task with a given name.
async fn get_task(db: &Arc<dyn Database>, name: String) -> Result<GetTaskResponse, DatabaseError> {
    let task = db.get_task(&name).await?;
    Ok(GetTaskResponse { task })
}

/// Gets the logs for a task with a name given the filter criteria.
async fn get_task_logs(
    db: &Arc<dyn Database>,
    name: String,
    stream: Option<LogSource>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<ListTaskLogsResponse, DatabaseError> {
    let logs = db.get_task_logs(&name, stream, limit, offset).await?;
    let total = db.count_task_logs(&name, stream).await?;
    Ok(ListTaskLogsResponse { logs, total })
}
