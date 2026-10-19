//! The task monitoring service.

use std::collections::HashMap;
use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use anyhow::Result;
use chrono::Utc;
use crankshaft_events::Event as CrankshaftEvent;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::error::TryRecvError;
use tokio::time::Interval;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;
use tracing::error;
use uuid::Uuid;
use wdl::engine::CLEANUP_TASK_NAME_PREFIX;
use wdl::engine::EngineEvent;

use crate::system::v1::db::Database;
use crate::system::v1::db::LogSource;
use crate::system::v1::db::TaskStatus;
use crate::system::v1::db::TaskWrite;
use crate::system::v1::db::retry_on_lock;

/// How often buffered task writes are flushed to the database, absent a
/// buffer-size-triggered flush.
///
/// This bounds how stale `get_task`/`get_task_logs`/CLI status polling can be
/// relative to what the monitor has actually observed; it does not affect
/// correctness, since every buffered write is still applied (just later).
const FLUSH_INTERVAL: Duration = Duration::from_millis(250);

/// The maximum number of buffered writes kept before an out-of-band flush is
/// triggered, bounding memory use for a chatty task (e.g. a burst of log
/// output) between ticks.
const MAX_BUFFERED_WRITES: usize = 256;

/// In-memory record of a task's call id and attempt number.
///
/// The monitor is the sole writer of a task's row, and knows this
/// information at the moment it creates the row; tracking it here lets
/// [`EngineEvent::TaskRetrying`] look up the failed attempt's call id and
/// attempt number without reading the database, which would otherwise race
/// a create that is still sitting in the write buffer, unflushed.
#[derive(Debug, Clone, Default)]
struct TaskMeta {
    /// The stable WDL call path shared by every attempt of the same call.
    call_id: Option<String>,
    /// The 0-based execution attempt number within the call.
    attempt: i64,
}

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
    /// The periodic flush tick elapsed.
    Tick,
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
///
/// Rather than writing to the database once per observed event, the monitor
/// buffers the writes an event implies and periodically flushes a batch of
/// them together (see [`Database::apply_task_writes`]), so that many
/// logically-separate operations collected over a short window can share a
/// single commit. A flush happens whichever comes first of [`FLUSH_INTERVAL`]
/// elapsing, the buffer reaching [`MAX_BUFFERED_WRITES`], or the monitor
/// shutting down; task state visible to readers (`get_task`, `get_task_logs`,
/// CLI status polling) can therefore lag what the monitor has observed by up
/// to one flush interval, but no buffered write is ever dropped.
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
    /// Buffered task writes awaiting a flush; see the type-level
    /// documentation for the batching rationale.
    pending: Vec<TaskWrite>,
    /// In-memory call id/attempt tracking per task name; see [`TaskMeta`].
    task_meta: HashMap<String, TaskMeta>,
    /// Fires on [`FLUSH_INTERVAL`] to trigger a periodic flush of `pending`.
    flush_tick: Interval,
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
        let mut flush_tick = tokio::time::interval(FLUSH_INTERVAL);
        // A late tick (e.g. because the loop was busy handling a burst of
        // events) should not fire repeatedly to "catch up"; the next tick
        // simply arrives one full interval after the late one.
        flush_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        Self {
            run_id,
            db,
            crankshaft,
            engine,
            shutdown,
            task_names: HashMap::new(),
            unfinished: HashSet::new(),
            pending: Vec::new(),
            task_meta: HashMap::new(),
            flush_tick,
        }
    }

    /// Runs the monitor loop.
    ///
    /// The monitor listens for events from the engine and from Crankshaft and
    /// updates the database accordingly. It ends when it is shut down or when
    /// both event channels have closed, reconciling any task left in a
    /// non-terminal status on the way out.
    pub async fn run(mut self) -> Result<()> {
        let mut crankshaft_open = true;
        let mut engine_open = true;

        while crankshaft_open || engine_open {
            // The receivers are only borrowed for the duration of the select,
            // so that handling an event can take the monitor
            // mutably.
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
                _ = self.flush_tick.tick() => Incoming::Tick,
            };

            match incoming {
                Incoming::Crankshaft(event) => self.handle_crankshaft_event(event),
                Incoming::Engine(event) => self.handle_engine_event(event),
                Incoming::Lagged => {
                    error!(
                        "task event handler lagged; task entries in database may not reflect the \
                         true status",
                    );
                }
                Incoming::CrankshaftClosed => crankshaft_open = false,
                Incoming::EngineClosed => engine_open = false,
                Incoming::Tick => self.flush().await,
                Incoming::Shutdown => break,
            }

            // A tick-triggered flush already just ran; this only catches the
            // size-cap case for a burst of events arriving between ticks.
            if self.pending.len() >= MAX_BUFFERED_WRITES {
                self.flush().await;
            }
        }

        // A shutdown can arrive while events are still buffered. Those events
        // are the record of what actually happened, so they are
        // consumed before any task is written off as unfinished.
        self.drain().await;
        self.reconcile();

        // Every write observed for this run, including reconciliation, is
        // flushed together as the monitor's final batch.
        self.flush_final().await
    }

    /// Flushes every buffered write to the database as a single batch.
    ///
    /// A failed flush remains buffered in its original order and is retried
    /// by the next size, timer, or shutdown-triggered flush.
    async fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }

        let count = self.pending.len();
        if let Err(e) = apply_pending(&mut self.pending, |writes| {
            self.db.apply_task_writes(writes)
        })
        .await
        {
            error!("failed to flush {count} buffered task write(s): {e:#}");
        }
    }

    /// Flushes the final batch with bounded retries for transient lock errors.
    async fn flush_final(&mut self) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }

        let writes = self.pending.clone();
        retry_on_lock(|| self.db.apply_task_writes(writes.clone())).await?;
        self.pending.clear();
        Ok(())
    }

    /// Consumes every event still buffered on either channel.
    async fn drain(&mut self) {
        loop {
            match self.crankshaft.try_recv() {
                Ok(event) => self.handle_crankshaft_event(event),
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            }
        }

        loop {
            match self.engine.try_recv() {
                Ok(event) => self.handle_engine_event(event),
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
    fn reconcile(&mut self) {
        let completed_at = Utc::now();
        for name in self.unfinished.drain() {
            self.pending
                .push(TaskWrite::Canceled { name, completed_at });
        }
    }

    /// Handles a received engine event.
    fn handle_engine_event(&mut self, event: EngineEvent) {
        if let Err(e) = self.handle_engine_event_inner(event) {
            error!("{e:#}");
        }
    }

    /// Builds the buffered writes implied by a received engine event.
    fn handle_engine_event_inner(&mut self, event: EngineEvent) -> Result<()> {
        match event {
            EngineEvent::TaskInitializing { id, name, attempt } => {
                let attempt = attempt.try_into().unwrap_or(i64::MAX);
                self.task_meta.insert(
                    name.clone(),
                    TaskMeta {
                        call_id: Some(id.clone()),
                        attempt,
                    },
                );
                self.pending.push(TaskWrite::Create {
                    name: name.clone(),
                    run_id: self.run_id,
                    status: TaskStatus::Initializing,
                    call_id: Some(id),
                    attempt,
                });
                self.unfinished.insert(name);
            }
            EngineEvent::TaskLocalizing { name } => {
                self.pending.push(TaskWrite::Localizing { name });
            }
            EngineEvent::TaskExecuting { name, constraints } => {
                let constraints = serde_json::to_string(&constraints)
                    .context("failed to serialize task constraints")?;
                self.pending
                    .push(TaskWrite::Constraints { name, constraints });
            }
            EngineEvent::TaskRetrying {
                prior_name,
                next_name,
                cause,
            } => {
                // Record the cause on the attempt that failed; that row has
                // usually already reached a terminal status.
                let cause =
                    serde_json::to_string(&cause).context("failed to serialize retry cause")?;
                self.pending.push(TaskWrite::RetryCause {
                    name: prior_name.clone(),
                    cause,
                });

                // Create the successor's row, inheriting the call id and
                // attempt number from the failed attempt. An evaluator retry
                // is followed by a `TaskInitializing` event that raises the
                // attempt number; a backend-local resubmission is not, and
                // remains part of the same attempt. The prior attempt's
                // metadata is looked up in memory rather than read back from
                // the database, since its own creation may still be sitting
                // unflushed in this same buffer.
                let TaskMeta { call_id, attempt } =
                    self.task_meta.get(&prior_name).cloned().unwrap_or_default();
                self.task_meta.insert(
                    next_name.clone(),
                    TaskMeta {
                        call_id: call_id.clone(),
                        attempt,
                    },
                );
                self.pending.push(TaskWrite::Create {
                    name: next_name.clone(),
                    run_id: self.run_id,
                    status: TaskStatus::Initializing,
                    call_id,
                    attempt,
                });
                self.unfinished.insert(next_name);
            }
            EngineEvent::ReusedCachedExecutionResult { id, name } => {
                // The task may never have been announced as initializing if
                // that event is still in flight on the other
                // channel.
                self.task_meta.insert(
                    name.clone(),
                    TaskMeta {
                        call_id: Some(id.clone()),
                        attempt: 0,
                    },
                );
                self.pending.push(TaskWrite::Create {
                    name: name.clone(),
                    run_id: self.run_id,
                    status: TaskStatus::Initializing,
                    call_id: Some(id),
                    attempt: 0,
                });
                self.pending.push(TaskWrite::Cached {
                    name: name.clone(),
                    completed_at: Utc::now(),
                });
                self.unfinished.remove(&name);
            }
            EngineEvent::TaskParked | EngineEvent::TaskUnparked { .. } => {
                // Parking is a property of the host's resource pool rather than
                // of the task's own progress, and the task
                // remains pending throughout.
            }
            _ => {}
        }

        Ok(())
    }

    /// Handles a received Crankshaft event.
    fn handle_crankshaft_event(&mut self, event: CrankshaftEvent) {
        if let Err(e) = self.handle_crankshaft_event_inner(event) {
            error!("{e:#}");
        }
    }

    /// Builds the buffered writes implied by a received Crankshaft event.
    fn handle_crankshaft_event_inner(&mut self, event: CrankshaftEvent) -> Result<()> {
        match event {
            CrankshaftEvent::TaskCreated {
                id,
                name,
                tes_id: _,
                token: _,
            } => {
                // A backend may run a task on its own behalf rather than on
                // behalf of a WDL task, such as the Docker
                // backend's `chown` of a work directory.
                // Crankshaft reports it like any other task, but it is
                // an implementation detail of running a task rather than
                // something a user submitted, so it is left out
                // of the run's tasks entirely. Dropping its id
                // here is enough: every later event is resolved
                // through `task_names`.
                if name.starts_with(CLEANUP_TASK_NAME_PREFIX) {
                    return Ok(());
                }

                self.task_names.insert(id, name.clone());
                self.pending.push(TaskWrite::Create {
                    name: name.clone(),
                    run_id: self.run_id,
                    status: TaskStatus::Pending,
                    call_id: None,
                    attempt: 0,
                });
                self.pending.push(TaskWrite::Pending { name: name.clone() });
                self.unfinished.insert(name);
            }
            CrankshaftEvent::TaskStarted { id } => {
                if let Some(name) = self.task_names.get(&id).cloned() {
                    self.pending.push(TaskWrite::Started {
                        name,
                        started_at: Utc::now(),
                    });
                }
            }
            CrankshaftEvent::TaskCompleted { id, exit_statuses } => {
                if let Some(name) = self.task_names.get(&id).cloned() {
                    let exit_status = exit_statuses.last().code();
                    self.pending.push(TaskWrite::Completed {
                        name: name.clone(),
                        exit_status,
                        completed_at: Utc::now(),
                    });
                    self.unfinished.remove(&name);
                }
            }
            CrankshaftEvent::TaskFailed { id, message } => {
                if let Some(name) = self.task_names.get(&id).cloned() {
                    self.pending.push(TaskWrite::Failed {
                        name: name.clone(),
                        error: message,
                        completed_at: Utc::now(),
                    });
                    self.unfinished.remove(&name);
                }
            }
            CrankshaftEvent::TaskCanceled { id } => {
                if let Some(name) = self.task_names.get(&id).cloned() {
                    self.pending.push(TaskWrite::Canceled {
                        name: name.clone(),
                        completed_at: Utc::now(),
                    });
                    self.unfinished.remove(&name);
                }
            }
            CrankshaftEvent::TaskPreempted { id } => {
                if let Some(name) = self.task_names.get(&id).cloned() {
                    self.pending.push(TaskWrite::Preempted {
                        name: name.clone(),
                        completed_at: Utc::now(),
                    });
                    self.unfinished.remove(&name);
                }
            }
            CrankshaftEvent::TaskResourceUsage { id, usage } => {
                // Utilization is a cumulative snapshot: the last received
                // sample for each field is authoritative, and the write is
                // guard-free, so repeated samples simply patch over the
                // previous value for the fields they report (see
                // `utilization_patch` for the null-omission rationale).
                if let Some(name) = self.task_names.get(&id).cloned() {
                    let patch = utilization_patch(&usage)
                        .context("failed to serialize task resource usage")?;
                    self.pending.push(TaskWrite::Utilization { name, patch });
                }
            }
            CrankshaftEvent::TaskStdout { id, message } => {
                if let Some(name) = self.task_names.get(&id).cloned() {
                    self.pending.push(TaskWrite::Log {
                        name,
                        source: LogSource::Stdout,
                        chunk: message.to_vec(),
                    });
                }
            }
            CrankshaftEvent::TaskStderr { id, message } => {
                if let Some(name) = self.task_names.get(&id).cloned() {
                    self.pending.push(TaskWrite::Log {
                        name,
                        source: LogSource::Stderr,
                        chunk: message.to_vec(),
                    });
                }
            }
            CrankshaftEvent::ImagePullStarted { id: _, name: _ }
            | CrankshaftEvent::ImagePullFailed {
                id: _,
                name: _,
                message: _,
            }
            | CrankshaftEvent::ImagePullFinished { id: _, name: _ }
            | CrankshaftEvent::TaskContainerCreated {
                id: _,
                container: _,
            }
            | CrankshaftEvent::TaskContainerExited {
                id: _,
                container: _,
                exit_status: _,
            } => {
                // Intentional no-op
            }
        }

        Ok(())
    }
}

/// Applies a copy of the pending writes and clears the buffer only after
/// success.
async fn apply_pending<F, Fut, E>(pending: &mut Vec<TaskWrite>, apply: F) -> Result<(), E>
where
    F: FnOnce(Vec<TaskWrite>) -> Fut,
    Fut: Future<Output = Result<(), E>>,
{
    apply(pending.clone()).await?;
    pending.clear();
    Ok(())
}

/// Serializes a resource usage sample into a JSON Merge Patch fragment
/// suitable for [`Database::update_task_utilization`], omitting any
/// top-level field whose value is `null`.
///
/// A JSON Merge Patch interprets an explicit `null` as "delete this key,"
/// but an absent measurement here means "no information," not "clear the
/// previously recorded value," so such fields must not appear in the patch
/// at all.
fn utilization_patch(usage: &impl serde::Serialize) -> Result<String> {
    let mut value = serde_json::to_value(usage).context("failed to serialize task usage")?;
    if let Some(map) = value.as_object_mut() {
        map.retain(|_, v| !v.is_null());
    }

    serde_json::to_string(&value).context("failed to serialize task utilization patch")
}

#[cfg(test)]
mod tests {
    use anyhow::bail;
    use wdl::engine::RetryCause;

    use super::*;
    use crate::system::v1::db::SprocketCommand;
    use crate::system::v1::db::SqliteDatabase;

    #[tokio::test]
    async fn failed_flush_retains_writes_in_order() {
        let mut pending = vec![
            TaskWrite::Localizing {
                name: "first".to_string(),
            },
            TaskWrite::Localizing {
                name: "second".to_string(),
            },
        ];

        let result = apply_pending(&mut pending, |_writes| async {
            bail!("simulated database failure")
        })
        .await;

        assert!(result.is_err());
        assert!(matches!(
            pending.as_slice(),
            [
                TaskWrite::Localizing { name: first },
                TaskWrite::Localizing { name: second }
            ] if first == "first" && second == "second"
        ));
    }

    #[sqlx::test]
    async fn retry_inherits_metadata_without_reading_the_prior_row(pool: sqlx::SqlitePool) {
        let db = Arc::new(
            SqliteDatabase::from_pool(pool.clone())
                .await
                .expect("failed to create database"),
        );

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");

        let run_id = Uuid::new_v4();
        db.create_run(run_id, session_id, "test-run", "test.wdl", Some("t"), "{}")
            .await
            .expect("failed to create run");

        let (_crankshaft_tx, crankshaft) = broadcast::channel(1);
        let (_engine_tx, engine) = broadcast::channel(1);
        let mut monitor = TaskMonitorSvc::new(
            run_id,
            db.clone(),
            crankshaft,
            engine,
            CancellationToken::new(),
        );

        monitor.handle_engine_event(EngineEvent::TaskInitializing {
            id: "wf.task".to_string(),
            name: "wf-task-attempt-0".to_string(),
            attempt: 2,
        });
        monitor
            .flush_final()
            .await
            .expect("failed to flush initialization");

        sqlx::query("delete from tasks where name = ?")
            .bind("wf-task-attempt-0")
            .execute(&pool)
            .await
            .expect("failed to remove prior task row");

        monitor.handle_engine_event(EngineEvent::TaskRetrying {
            prior_name: "wf-task-attempt-0".to_string(),
            next_name: "wf-task-attempt-0-resubmit-1".to_string(),
            cause: RetryCause::Preempted,
        });
        monitor.flush_final().await.expect("failed to flush retry");

        let successor = db
            .get_task("wf-task-attempt-0-resubmit-1")
            .await
            .expect("failed to get successor");
        assert_eq!(successor.call_id.as_deref(), Some("wf.task"));
        assert_eq!(successor.attempt, 2);
    }
}
