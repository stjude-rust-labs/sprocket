//! The run manager service.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use chrono::Utc;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tracing::error;
use tracing::info;
use tracing::trace;
use tracing::warn;
use uuid::Uuid;
use wdl::engine::CancellationContext;
use wdl::engine::CancellationContextState;
use wdl::engine::Engine;
use wdl::engine::Events;

use crate::analysis::Source;
use crate::config::Config;
use crate::config::FallbackVersion;
use crate::config::ServerConfig;
use crate::system::v1::db::Database;
use crate::system::v1::db::DatabaseError;
use crate::system::v1::db::LogSource;
use crate::system::v1::db::RunStatus;
use crate::system::v1::db::SprocketCommand;
use crate::system::v1::db::TaskStatus;
use crate::system::v1::exec::ConfigError;
use crate::system::v1::exec::JsonObject;
use crate::system::v1::exec::RunnableExecutor;
use crate::system::v1::exec::create_run_record;
use crate::system::v1::exec::create_session;
use crate::system::v1::exec::validate_source;
use crate::system::v1::fs::IndexPath;
use crate::system::v1::fs::OutputDirectory;

pub(crate) mod commands;

pub use commands::*;
use wdl::diagnostics::Mode;

/// Channel capacity for events.
///
/// This number represents a reasonable, arbitrary buffer size to handle burst
/// event production.
const EVENTS_CHANNEL_CAPACITY: usize = 2048;

/// A receiver for commands issued to the run manager service.
type Rx = mpsc::Receiver<RunManagerCmd>;

/// The run manager service.
///
/// The run manager service is an actor that executes WDL tasks and workflows
/// using all of the conventions of Sprocket (e.g., instantiating a run
/// directory, indexing desired outputs, etc). It is the main entrypoint to WDL
/// evaluation in Sprocket.
#[allow(missing_debug_implementations)]
pub struct RunManagerSvc {
    /// The configuration for execution.
    config: ServerConfig,
    /// The WDL evaluation engine for evaluating tasks and workflows.
    engine: Engine,
    /// The fallback WDL version for documents with unrecognized versions.
    fallback_version: FallbackVersion,
    /// Feature flags used during analysis.
    feature_flags: wdl::analysis::FeatureFlags,
    /// Whether to ignore `.sprocketignore` files during document discovery.
    no_ignore: bool,
    /// Module resolver configuration used during analysis.
    modules_config: wdl_modules::resolver::ModulesConfig,
    /// The output directory root.
    output_dir: OutputDirectory,
    /// A handle to the database.
    db: Arc<dyn Database>,
    /// Session ID for this server instance.
    ///
    /// This field keeps track of which session entry in the database this
    /// manager service is associated with.
    ///
    /// Shared with the sweeper task, which must see the same session this
    /// service submits runs against, including one created after startup by
    /// the fallback in the `Submit` arm below.
    session_id: Arc<Mutex<Option<Uuid>>>,
    /// The receiver for commands.
    rx: Rx,
    /// A semaphore for limiting concurrent runs.
    semaphore: Option<Arc<Semaphore>>,
    /// A mapping of runs to cancellation contexts.
    ///
    /// A [`tokio::sync::Mutex`] is used because the [`run()`][Self::run] future
    /// must be `Send`.
    runs: Arc<Mutex<HashMap<Uuid, CancellationContext>>>,
    /// The diagnostic reporting mode.
    report_mode: Mode,
    /// Whether to colorize diagnostics.
    colorize: bool,
}

impl RunManagerSvc {
    /// Create a new run manager.
    pub async fn new(
        config: Config,
        report_mode: Mode,
        colorize: bool,
        db: Arc<dyn Database>,
        rx: Rx,
    ) -> Result<Self> {
        let fallback_version = config.common.wdl.fallback_version;
        let feature_flags = config.common.wdl.feature_flags;
        let no_ignore = config.common.no_ignore;
        let modules_config = config.modules.clone();
        let mut config = config.server;
        let semaphore =
            Option::<usize>::from(config.max_concurrent_runs).map(|n| Arc::new(Semaphore::new(n)));
        let output_dir = OutputDirectory::new(&config.output_dir);
        let engine = Engine::new(std::mem::take(&mut config.engine))
            .await
            .context("failed to create WDL evaluation engine")?;

        Ok(Self {
            config,
            engine,
            fallback_version,
            feature_flags,
            no_ignore,
            modules_config,
            output_dir,
            db,
            // Created eagerly by `run`, with a fallback on first submission.
            session_id: Default::default(),
            rx,
            semaphore,
            runs: Default::default(),
            report_mode,
            colorize,
        })
    }

    /// Runs the event loop.
    pub async fn run(mut self) {
        info!("run manager service started");
        info!("allowed file paths: {:?}", self.config.allowed_file_paths);
        info!("allowed urls: {:?}", self.config.allowed_urls);

        // The session must exist before anything is submitted against it, and
        // be heartbeated from that moment: `mark_orphaned_runs` makes no
        // exception for the session doing the sweeping.
        match create_session(self.db.as_ref(), SprocketCommand::Server).await {
            Ok(session) => {
                if let Err(e) = self.db.heartbeat_session(session.uuid, Utc::now()).await {
                    error!(error = %e, "failed to record the initial session heartbeat");
                }

                *self.session_id.lock().await = Some(session.uuid);
            }
            Err(e) => {
                // Not fatal: the first submission retries this, and until it
                // succeeds this process owns no runs to sweep.
                error!(error = %e, "failed to create the server session on startup");
            }
        }

        let sweeper = Self::spawn_sweeper(
            self.db.clone(),
            self.session_id.clone(),
            self.config.heartbeat_interval(),
            self.config.orphan_timeout(),
        );

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

                    // Normally created at startup; retry here so a blip then
                    // does not sideline the service for its whole lifetime.
                    let mut session = self.session_id.lock().await;
                    let session_id = if let Some(id) = *session {
                        id
                    } else {
                        match create_session(self.db.as_ref(), SprocketCommand::Server).await {
                            Ok(created) => {
                                if let Err(e) =
                                    self.db.heartbeat_session(created.uuid, Utc::now()).await
                                {
                                    error!(error = %e, "failed to record session heartbeat");
                                }

                                *session = Some(created.uuid);
                                created.uuid
                            }
                            Err(e) => {
                                let _ = rx.send(Err(SubmitRunError::Database(e)));
                                continue;
                            }
                        }
                    };
                    drop(session);

                    let result = self
                        .submit_run(session_id, source, inputs, target, index_on)
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
                RunManagerCmd::CountRunTasksByStatus { run_id, rx } => {
                    trace!(?run_id, "received `CountRunTasksByStatus` command");
                    let result = count_run_tasks_by_status(&self.db, run_id).await;
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
        sweeper.abort();
    }

    /// Spawns the background task that keeps this server's session marked live
    /// and closes out runs whose owning process is gone, including runs a
    /// `sprocket run` invocation left behind.
    ///
    /// Heartbeating and sweeping share one task, in that order, so a failed
    /// heartbeat skips that round's sweep: without a fresh one this process
    /// cannot tell its own session apart from a dead one, and would orphan its
    /// own live runs.
    ///
    /// The first tick fires immediately, so a server closes out runs inherited
    /// from a process that died long ago at startup rather than an interval
    /// later.
    fn spawn_sweeper(
        db: Arc<dyn Database>,
        session_id: Arc<Mutex<Option<Uuid>>>,
        interval: Duration,
        timeout: Duration,
    ) -> JoinHandle<()> {
        /// The error recorded on runs closed out by the sweep.
        const ORPHANED_RUN_ERROR: &str = "the process that owned this run stopped reporting; the \
                                          run can no longer be observed or canceled";

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            // A late tick must not release a burst of catch-up sweeps.
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

            loop {
                ticker.tick().await;

                let now = Utc::now();

                // Copied out so the guard is not held across the write, which
                // would stall run submission behind every heartbeat.
                let session_id = *session_id.lock().await;
                if let Some(id) = session_id
                    && let Err(e) = db.heartbeat_session(id, now).await
                {
                    error!(
                        error = %e,
                        "failed to record session heartbeat; skipping this orphan sweep"
                    );
                    continue;
                }

                match db
                    .mark_orphaned_runs(ORPHANED_RUN_ERROR, timeout, now)
                    .await
                {
                    Ok(0) => {}
                    Ok(count) => warn!(
                        count,
                        "marked run(s) orphaned: the process that owned them stopped reporting"
                    ),
                    Err(e) => error!(error = %e, "failed to sweep for orphaned runs"),
                }
            }
        })
    }

    /// Spawns a new run manager service and returns:
    ///
    /// - the join handle of the event loop, and
    /// - the sender channel
    pub async fn spawn(
        channel_buffer_size: usize,
        config: Config,
        report_mode: Mode,
        colorize: bool,
        db: Arc<dyn Database>,
    ) -> Result<(JoinHandle<()>, mpsc::Sender<RunManagerCmd>)> {
        let (tx, rx) = mpsc::channel(channel_buffer_size);
        let manager = Self::new(config, report_mode, colorize, db, rx).await?;
        let handle = tokio::spawn(manager.run());
        Ok((handle, tx))
    }

    /// Submits a new run for execution.
    async fn submit_run(
        &self,
        session_id: Uuid,
        source: String,
        inputs: JsonObject,
        target: Option<String>,
        index_on: Option<IndexPath>,
    ) -> Result<SubmitResponse, SubmitRunError> {
        let source = match validate_source(&source, &self.config)? {
            Source::Directory(dir) => crate::analysis::resolve_module_entrypoint(&dir)
                .map_err(SubmitRunError::Analysis)?,
            source => source,
        };

        let (run_id, run_generated_name, _) = create_run_record(
            self.db.as_ref(),
            session_id,
            &source,
            target.as_deref(),
            &serde_json::to_string(&inputs)?,
        )
        .await?;

        let events = Events::new(EVENTS_CHANNEL_CAPACITY);
        let cancellation = CancellationContext::new(self.engine.config().failure_mode);
        let executor = RunnableExecutor::builder()
            .db(self.db.clone())
            .output_dir(self.output_dir.clone())
            .engine(self.engine.clone())
            .events(events.clone())
            .cancellation(cancellation.clone())
            .runs(self.runs.clone())
            .run_id(run_id)
            .run_name(run_generated_name.clone())
            .maybe_fallback_version(self.fallback_version.into())
            .feature_flags(self.feature_flags)
            .no_ignore(self.no_ignore)
            .modules_config(self.modules_config.clone())
            .source(source)
            .maybe_target(target)
            .inputs(inputs)
            .maybe_index_on(index_on)
            .report_mode(self.report_mode)
            .colorize(self.colorize)
            .build();

        let semaphore = self.semaphore.clone();
        let handle = tokio::spawn(async move {
            let _permit = if let Some(ref sem) = semaphore {
                // SAFETY: the semaphore is Arc-wrapped and held by the manager
                // for its entire lifetime. It is never
                // explicitly closed. If this fails, it
                // indicates a catastrophic programming error.
                Some(sem.acquire().await.unwrap())
            } else {
                None
            };

            executor.execute().await;
        });

        self.runs.lock().await.insert(run_id, cancellation);

        Ok(SubmitResponse {
            id: run_id,
            name: run_generated_name,
            events,
            handle,
        })
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
    /// JSON error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
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
    /// The run has no in-memory execution state to cancel.
    ///
    /// The run is non-terminal in the database, but this process has no
    /// `CancellationContext` for it: the run belongs to another process,
    /// either one still executing it or one that died holding it. The sweep in
    /// [`Database::mark_orphaned_runs`] closes out the latter.
    #[error(
        "run `{0}` is not tracked by this server instance and cannot be canceled; it belongs to \
         another process, or was left behind by one that stopped reporting"
    )]
    Orphaned(Uuid),
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
        RunStatus::Running | RunStatus::Analyzing | RunStatus::Queued | RunStatus::Canceling
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
            // (`Canceling`), unless the run reached its outcome in the
            // meantime: the run was already signaled above, and work that is
            // not a task execution — a transfer, say — stops right away.
            CancellationContextState::Waiting => {
                let _ = db.mark_run_canceling(id).await?;
            }
            // If we we `Canceling` back from the call, that means the task
            // is being actively canceled. As such, we can mark it as
            // `Canceled` in the database.
            CancellationContextState::Canceling => {
                db.cancel_run(id, Utc::now()).await?;
                // NOTE: when a run is actually canceled, remove it from the
                // runs map, as it won't remove itself at the
                // end of execution.
                runs_guard.remove(&id);
            }
        }
    } else if run.status == RunStatus::Canceling {
        // The run is no longer in the active runs map but the DB status
        // is still `Canceling`. This happens when the execution completes
        // after a lazy cancellation (first cancel in slow mode) but
        // before the second cancel arrives—the execution returns
        // `Ok(None)` for the cancellation and removes itself from the
        // map without updating the DB. Finalize the cancellation here.
        db.cancel_run(id, Utc::now()).await?;
    } else {
        // `run.status` is `Running` or `Queued` but there is no tracking
        // entry for it in this process. Every DB status transition for a
        // run happens strictly before its entry is removed from `runs`
        // (see `RunnableExecutor::execute`), so this can only mean the run
        // was never submitted by this process in the first place: it was
        // orphaned by a previous server instance. Report this explicitly
        // instead of returning success for a cancellation that did
        // nothing.
        return Err(CancelRunError::Orphaned(id));
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
    let total = db.count_sessions().await?;
    Ok(ListSessionsResponse { sessions, total })
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

/// Counts a run's tasks grouped by status.
async fn count_run_tasks_by_status(
    db: &Arc<dyn Database>,
    run_id: Uuid,
) -> Result<RunTaskCountsResponse, DatabaseError> {
    let counts = db.count_tasks_by_status(run_id).await?;
    Ok(RunTaskCountsResponse { counts })
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
    db.get_task(&name).await?;
    let logs = db.get_task_logs(&name, stream, limit, offset).await?;
    let total = db.count_task_logs(&name, stream).await?;
    Ok(ListTaskLogsResponse { logs, total })
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;
    use wdl::engine::config::FailureMode;

    use super::*;
    use crate::system::v1::db::SprocketCommand;
    use crate::system::v1::db::SqliteDatabase;

    #[sqlx::test]
    async fn cancel_run_errors_on_orphaned_run(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");
        let db: Arc<dyn Database> = Arc::new(db);

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Server, "test-user")
            .await
            .expect("failed to create session");

        let run_id = Uuid::new_v4();
        db.create_run(
            run_id,
            session_id,
            "orphaned-run",
            "test.wdl",
            Some("test_task"),
            "{}",
        )
        .await
        .expect("failed to create run");
        db.update_run_status(run_id, RunStatus::Running)
            .await
            .expect("failed to update run status");

        // No in-memory tracking entry for this run — simulates a run left
        // behind by a server process that stopped reporting, which is what
        // `mark_orphaned_runs` sweeps.
        let runs: Arc<Mutex<HashMap<Uuid, CancellationContext>>> = Default::default();

        let error = cancel_run(&db, &runs, run_id)
            .await
            .expect_err("cancel of an untracked run should error, not silently succeed");
        assert!(matches!(error, CancelRunError::Orphaned(id) if id == run_id));

        // Must be left completely alone: no silent status transition.
        let run = db
            .get_run(run_id)
            .await
            .expect("failed to get run")
            .unwrap();
        assert_eq!(run.status, RunStatus::Running);
    }

    #[sqlx::test]
    async fn cancel_run_transitions_tracked_run(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");
        let db: Arc<dyn Database> = Arc::new(db);

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Server, "test-user")
            .await
            .expect("failed to create session");

        let run_id = Uuid::new_v4();
        db.create_run(
            run_id,
            session_id,
            "tracked-run",
            "test.wdl",
            Some("test_task"),
            "{}",
        )
        .await
        .expect("failed to create run");
        db.update_run_status(run_id, RunStatus::Running)
            .await
            .expect("failed to update run status");

        let cancellation = CancellationContext::new(FailureMode::Slow);
        let runs: Arc<Mutex<HashMap<Uuid, CancellationContext>>> =
            Arc::new(Mutex::new(HashMap::from([(run_id, cancellation)])));

        // First cancel: lazy/`Waiting` — reflected as `Canceling` in the DB,
        // but the run stays tracked so a running task can finish.
        cancel_run(&db, &runs, run_id)
            .await
            .expect("first cancel should succeed");
        let run = db
            .get_run(run_id)
            .await
            .expect("failed to get run")
            .unwrap();
        assert_eq!(run.status, RunStatus::Canceling);
        assert!(runs.lock().await.contains_key(&run_id));

        // Second cancel: forceful/`Canceling` — marked `Canceled` and
        // removed from the tracking map.
        cancel_run(&db, &runs, run_id)
            .await
            .expect("second cancel should succeed");
        let run = db
            .get_run(run_id)
            .await
            .expect("failed to get run")
            .unwrap();
        assert_eq!(run.status, RunStatus::Canceled);
        assert!(!runs.lock().await.contains_key(&run_id));
    }
}
