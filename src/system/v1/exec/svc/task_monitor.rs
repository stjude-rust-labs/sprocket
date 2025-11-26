//! The task monitoring service.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use crankshaft::events::Event as CrankshaftEvent;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tracing::error;
use uuid::Uuid;

use crate::system::v1::db::Database;
use crate::system::v1::db::LogSource;

/// A receiver of Crankshaft events.
type Rx = broadcast::Receiver<CrankshaftEvent>;

/// A task monitoring service.
///
/// The task monitor service is an independent, async service that subscribes to
/// Crankshaft task events and updates the Sprocket database with information.
/// One task monitor is run per run and keeps track of all of the tasks therein
/// (multiple tasks for a workflow run or a single task for a task run).
#[allow(missing_debug_implementations)]
pub struct TaskMonitorSvc {
    /// The run to associate with monitored tasks.
    run_id: Uuid,
    /// A handle to the database.
    db: Arc<dyn Database>,
    /// The Crankshaft events receiver.
    rx: Rx,
    /// A map from Crankshaft task IDs to task name.
    ///
    /// The task name is only communicated once using the
    /// [`CrankshaftEvent::Created`] event. As such, we need to store the task
    /// name, since it's used to construct the unique key for a task's database
    /// entry.
    task_names: HashMap<u64, String>,
}

impl TaskMonitorSvc {
    /// Create a new task monitor.
    pub fn new(run_id: Uuid, db: Arc<dyn Database>, rx: Rx) -> Self {
        Self {
            run_id,
            db,
            rx,
            task_names: HashMap::new(),
        }
    }

    /// Runs the monitor loop.
    ///
    /// The monitor loop listens for events from Crankshaft and updates the
    /// database accordingly. When the broadcast channel is closed, the service
    /// automatically ends its execution.
    pub async fn run(mut self) {
        loop {
            match self.rx.recv().await {
                Ok(event) => {
                    if let Err(e) = self.handle_event(event).await {
                        error!("{e:#}");
                    }
                }
                Err(RecvError::Lagged(_)) => {
                    error!(
                        "task event handler lagged; task entries \
                        in database may not reflect the true status",
                    );
                }
                Err(RecvError::Closed) => {
                    // The events channel closed, exit the monitoring service
                    break;
                }
            }
        }
    }

    /// Handles a received Crankshaft event.
    async fn handle_event(&mut self, event: CrankshaftEvent) -> Result<()> {
        match event {
            CrankshaftEvent::TaskCreated {
                id,
                name,
                tes_id: _,
                token: _,
            } => {
                self.task_names.insert(id, name.clone());
                self.db.create_task(&name, self.run_id).await?;
            }
            CrankshaftEvent::TaskStarted { id } => {
                if let Some(name) = self.task_names.get(&id) {
                    self.db.update_task_started(name, Utc::now()).await?;
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
                if let Some(name) = self.task_names.get(&id) {
                    let exit_status = exit_statuses.last().code();
                    self.db
                        .update_task_completed(name, exit_status, Utc::now())
                        .await?;
                }
            }
            CrankshaftEvent::TaskFailed { id, message } => {
                if let Some(name) = self.task_names.get(&id) {
                    self.db
                        .update_task_failed(name, &message, Utc::now())
                        .await?;
                }
            }
            CrankshaftEvent::TaskCanceled { id } => {
                if let Some(name) = self.task_names.get(&id) {
                    self.db.update_task_canceled(name, Utc::now()).await?;
                }
            }
            CrankshaftEvent::TaskPreempted { id } => {
                if let Some(name) = self.task_names.get(&id) {
                    self.db.update_task_preempted(name, Utc::now()).await?;
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
