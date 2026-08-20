//! The task monitoring service.

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use crankshaft::events::Event as CrankshaftEvent;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::error::TryRecvError;
use tokio_util::sync::CancellationToken;
use tracing::error;
use uuid::Uuid;
use wdl::engine::CLEANUP_TASK_NAME_PREFIX;
use wdl::engine::EngineEvent;

use crate::system::v1::db::Database;
use crate::system::v1::db::LogSource;
use crate::system::v1::db::TaskStatus;

/// An event received by the monitor, or the loss of the channel carrying it.
enum Incoming {
    /// A Crankshaft event.
    Crankshaft(CrankshaftEvent),
    /// An engine event.
    Engine(EngineEvent),
    /// A channel dropped events because the monitor could not keep up.
    Lagged,
    /// The Crankshaft channel closed.
    CrankshaftClosed,
    /// The engine channel closed.
    EngineClosed,
    /// The monitor was asked to shut down.
    Shutdown,
}

/// A task monitoring service.
///
/// The task monitor service is an independent, async service that subscribes to
/// engine and Crankshaft task events and updates the Sprocket database with
/// information. One task monitor is run per run and keeps track of all of the
/// tasks therein (multiple tasks for a workflow run or a single task for a task
/// run).
///
/// The two event channels are independent and carry no ordering relative to one
/// another, so a task's submission may be observed before the engine events
/// that precede it. Every transition the monitor performs is therefore
/// monotonic in the database: a status only ever advances.
#[allow(missing_debug_implementations)]
pub struct TaskMonitorSvc {
    /// The run to associate with monitored tasks.
    run_id: Uuid,
    /// A handle to the database.
    db: Arc<dyn Database>,
    /// The Crankshaft events receiver.
    crankshaft: broadcast::Receiver<CrankshaftEvent>,
    /// The engine events receiver.
    engine: broadcast::Receiver<EngineEvent>,
    /// Signals that the run has finished and the monitor should reconcile and
    /// exit.
    shutdown: CancellationToken,
    /// A map from Crankshaft task IDs to task name.
    ///
    /// The task name is only communicated once using the
    /// [`CrankshaftEvent::TaskCreated`] event. As such, we need to store the
    /// task name, since it's used to construct the unique key for a task's
    /// database entry.
    task_names: HashMap<u64, String>,
    /// The names of tasks that have a database row but have not been observed
    /// reaching a terminal status.
    unfinished: HashSet<String>,
}

impl TaskMonitorSvc {
    /// Create a new task monitor.
    pub fn new(
        run_id: Uuid,
        db: Arc<dyn Database>,
        crankshaft: broadcast::Receiver<CrankshaftEvent>,
        engine: broadcast::Receiver<EngineEvent>,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            run_id,
            db,
            crankshaft,
            engine,
            shutdown,
            task_names: HashMap::new(),
            unfinished: HashSet::new(),
        }
    }

    /// Runs the monitor loop.
    ///
    /// The monitor listens for events from the engine and from Crankshaft and
    /// updates the database accordingly. It ends when it is shut down or when
    /// both event channels have closed, reconciling any task left in a
    /// non-terminal status on the way out.
    pub async fn run(mut self) {
        let mut crankshaft_open = true;
        let mut engine_open = true;

        while crankshaft_open || engine_open {
            // The receivers are only borrowed for the duration of the select, so
            // that handling an event can take the monitor mutably.
            let incoming = select! {
                biased;
                _ = self.shutdown.cancelled() => Incoming::Shutdown,
                r = self.crankshaft.recv(), if crankshaft_open => match r {
                    Ok(event) => Incoming::Crankshaft(event),
                    Err(RecvError::Lagged(_)) => Incoming::Lagged,
                    Err(RecvError::Closed) => Incoming::CrankshaftClosed,
                },
                r = self.engine.recv(), if engine_open => match r {
                    Ok(event) => Incoming::Engine(event),
                    Err(RecvError::Lagged(_)) => Incoming::Lagged,
                    Err(RecvError::Closed) => Incoming::EngineClosed,
                },
            };

            match incoming {
                Incoming::Crankshaft(event) => {
                    if let Err(e) = self.handle_crankshaft_event(event).await {
                        error!("{e:#}");
                    }
                }
                Incoming::Engine(event) => {
                    if let Err(e) = self.handle_engine_event(event).await {
                        error!("{e:#}");
                    }
                }
                Incoming::Lagged => {
                    error!(
                        "task event handler lagged; task entries in database may not reflect the \
                         true status",
                    );
                }
                Incoming::CrankshaftClosed => crankshaft_open = false,
                Incoming::EngineClosed => engine_open = false,
                Incoming::Shutdown => break,
            }
        }

        // A shutdown can arrive while events are still buffered. Those events are
        // the record of what actually happened, so they are consumed before any
        // task is written off as unfinished.
        self.drain().await;
        self.reconcile().await;
    }

    /// Consumes every event still buffered on either channel.
    async fn drain(&mut self) {
        loop {
            match self.crankshaft.try_recv() {
                Ok(event) => {
                    if let Err(e) = self.handle_crankshaft_event(event).await {
                        error!("{e:#}");
                    }
                }
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            }
        }

        loop {
            match self.engine.try_recv() {
                Ok(event) => {
                    if let Err(e) = self.handle_engine_event(event).await {
                        error!("{e:#}");
                    }
                }
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            }
        }
    }

    /// Marks any task that never reached a terminal status as canceled.
    ///
    /// A task can be left in a non-terminal status when the run fails or is
    /// canceled before the task reaches its backend, in which case no event
    /// announcing its fate is ever emitted. The run's own error carries why;
    /// this only ensures the task does not appear to be working forever.
    async fn reconcile(&mut self) {
        let completed_at = Utc::now();
        for name in self.unfinished.drain() {
            match self.db.update_task_canceled(&name, completed_at).await {
                Ok(_) => {}
                Err(e) => error!("failed to reconcile status of task `{name}`: {e:#}"),
            }
        }
    }

    /// Handles a received engine event.
    async fn handle_engine_event(&mut self, event: EngineEvent) -> Result<()> {
        match event {
            EngineEvent::TaskInitializing { id: _, name } => {
                self.db
                    .create_task(&name, self.run_id, TaskStatus::Initializing)
                    .await?;
                self.unfinished.insert(name);
            }
            EngineEvent::TaskLocalizing { name } => {
                let _ = self.db.update_task_localizing(&name).await?;
            }
            EngineEvent::ReusedCachedExecutionResult { id: _, name } => {
                // The task may never have been announced as initializing if that
                // event is still in flight on the other channel.
                self.db
                    .create_task(&name, self.run_id, TaskStatus::Initializing)
                    .await?;
                let _ = self.db.update_task_cached(&name, Utc::now()).await?;
                self.unfinished.remove(&name);
            }
            EngineEvent::TaskParked | EngineEvent::TaskUnparked { .. } => {
                // Parking is a property of the host's resource pool rather than
                // of the task's own progress, and the task
                // remains pending throughout.
            }
        }

        Ok(())
    }

    /// Handles a received Crankshaft event.
    async fn handle_crankshaft_event(&mut self, event: CrankshaftEvent) -> Result<()> {
        match event {
            CrankshaftEvent::TaskCreated {
                id,
                name,
                tes_id: _,
                token: _,
            } => {
                // A backend may run a task on its own behalf rather than on behalf
                // of a WDL task, such as the Docker backend's `chown` of a work
                // directory. Crankshaft reports it like any other task, but it is
                // an implementation detail of running a task rather than something
                // a user submitted, so it is left out of the run's tasks entirely.
                // Dropping its id here is enough: every later event is resolved
                // through `task_names`.
                if name.starts_with(CLEANUP_TASK_NAME_PREFIX) {
                    return Ok(());
                }

                self.task_names.insert(id, name.clone());
                self.db
                    .create_task(&name, self.run_id, TaskStatus::Pending)
                    .await?;
                let _ = self.db.update_task_pending(&name).await?;
                self.unfinished.insert(name);
            }
            CrankshaftEvent::TaskStarted { id } => {
                if let Some(name) = self.task_names.get(&id) {
                    let _ = self.db.update_task_started(name, Utc::now()).await?;
                }
            }
            CrankshaftEvent::TaskContainerCreated {
                id: _,
                container: _,
            } => {
                // Intentional no-op
            }
            CrankshaftEvent::TaskContainerExited {
                id: _,
                container: _,
                exit_status: _,
            } => {
                // Intentional no-op
            }
            CrankshaftEvent::TaskCompleted { id, exit_statuses } => {
                if let Some(name) = self.task_names.get(&id).cloned() {
                    let exit_status = exit_statuses.last().code();
                    let _ = self
                        .db
                        .update_task_completed(&name, exit_status, Utc::now())
                        .await?;
                    self.unfinished.remove(&name);
                }
            }
            CrankshaftEvent::TaskFailed { id, message } => {
                if let Some(name) = self.task_names.get(&id).cloned() {
                    let _ = self
                        .db
                        .update_task_failed(&name, &message, Utc::now())
                        .await?;
                    self.unfinished.remove(&name);
                }
            }
            CrankshaftEvent::TaskCanceled { id } => {
                if let Some(name) = self.task_names.get(&id).cloned() {
                    let _ = self.db.update_task_canceled(&name, Utc::now()).await?;
                    self.unfinished.remove(&name);
                }
            }
            CrankshaftEvent::TaskPreempted { id } => {
                if let Some(name) = self.task_names.get(&id).cloned() {
                    let _ = self.db.update_task_preempted(&name, Utc::now()).await?;
                    self.unfinished.remove(&name);
                }
            }
            CrankshaftEvent::TaskStdout { id, message } => {
                if let Some(name) = self.task_names.get(&id) {
                    self.db
                        .insert_task_log(name, LogSource::Stdout, &message)
                        .await?;
                }
            }
            CrankshaftEvent::TaskStderr { id, message } => {
                if let Some(name) = self.task_names.get(&id) {
                    self.db
                        .insert_task_log(name, LogSource::Stderr, &message)
                        .await?;
                }
            }
        }

        Ok(())
    }
}
