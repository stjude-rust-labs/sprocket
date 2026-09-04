//! Database schema and operations for provenance tracking in v1.

use std::future::Future;
use std::time::Duration;

use async_trait::async_trait;
use chrono::DateTime;
use chrono::Utc;
use serde_json::Value;
use thiserror::Error;
use tokio_retry2::Retry;
use tokio_retry2::RetryError;
use tokio_retry2::strategy::ExponentialBackoff;
use uuid::Uuid;

pub mod models;
pub mod sqlite;

pub use models::IndexLogEntry;
pub use models::LogSource;
pub use models::Run;
pub use models::RunStatus;
pub use models::Session;
pub use models::SprocketCommand;
pub use models::Task;
pub use models::TaskLog;
pub use models::TaskStatus;
pub use sqlite::SqliteDatabase;

/// Database errors.
#[derive(Debug, Error)]
pub enum DatabaseError {
    /// A database error.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),

    /// A whoami error.
    #[error(transparent)]
    WhoAmI(#[from] whoami::Error),

    /// A migration error.
    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),

    /// An I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// A JSON serialization error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// Invalid database schema version.
    #[error("invalid database schema version: expected `{expected}`, found `{found}`")]
    InvalidVersion {
        /// Expected version.
        expected: String,
        /// Found version.
        found: String,
    },

    /// Resource not found.
    #[error("{0}")]
    NotFound(String),
}

/// Result type for database operations.
pub type Result<T> = std::result::Result<T, DatabaseError>;

/// A page of database records and the total number of matching records.
#[derive(Debug)]
pub struct ReadPage<T> {
    /// The records in the requested page.
    pub records: Vec<T>,
    /// The total number of matching records before pagination.
    pub total: i64,
}

/// Returns a total that is no smaller than the end of the returned page.
fn page_total<T>(records: &[T], total: i64, offset: Option<i64>) -> i64 {
    let record_count = i64::try_from(records.len()).unwrap_or(i64::MAX);
    let page_end = offset.unwrap_or_default().saturating_add(record_count);
    total.max(page_end)
}

/// The maximum number of attempts made by [`retry_on_lock`] before giving up
/// and returning the last transient error.
///
/// Real-world GPFS benchmarking observed isolated commit stalls near 20
/// seconds during otherwise clean runs. This wider budget gives transient
/// filesystem contention more time to clear without increasing the timeout
/// of each individual attempt.
const MAX_LOCK_RETRIES: usize = 10;

/// Returns `true` if `error` represents a transient SQLite busy/locked
/// condition (`SQLITE_BUSY` or `SQLITE_LOCKED`) that is worth retrying, as
/// opposed to a permanent error such as a constraint violation or "not
/// found".
///
/// `sqlx`'s SQLite error `code()` returns the *extended* result code (e.g.
/// `773` for `SQLITE_BUSY_TIMEOUT`), so the primary code is recovered by
/// masking with `0xff` rather than comparing the raw value directly.
fn is_transient_lock_error(error: &DatabaseError) -> bool {
    let DatabaseError::Sqlx(sqlx::Error::Database(db_err)) = error else {
        return false;
    };

    db_err
        .code()
        .and_then(|code| code.parse::<i32>().ok())
        .map(|code| matches!(code & 0xFF, 5 | 6)) // SQLITE_BUSY | SQLITE_LOCKED
        .unwrap_or(false)
}

/// Retries `operation` with exponential backoff when it fails with a
/// transient SQLite busy/locked error, and fails fast for any other error.
///
/// This exists as a second line of defense on top of the `busy_timeout`
/// pragma: a single attempt may already block for up to `busy_timeout`
/// before returning `SQLITE_BUSY`, so this helper is for the case where
/// contention outlasts even that wait — observed in production against a
/// database hosted on a network filesystem, where it can otherwise abort or
/// incorrectly record an entire run. Use it to wrap DB writes on the
/// run-execution hot path where that outcome is unacceptable; it is not
/// intended for read-mostly, user-initiated request handlers where surfacing
/// the error immediately (for the caller to retry) is reasonable.
///
/// The backoff delay is capped at 10 seconds to accommodate the same
/// multi-second filesystem stalls without changing `busy_timeout`.
pub async fn retry_on_lock<T, F, Fut>(operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let strategy = ExponentialBackoff::from_millis(500)
        .max_delay(Duration::from_secs(10))
        .take(MAX_LOCK_RETRIES);

    let mut operation = operation;
    Retry::spawn_notify(
        strategy,
        move || {
            let fut = operation();
            async move {
                fut.await.map_err(|e| {
                    if is_transient_lock_error(&e) {
                        RetryError::transient(e)
                    } else {
                        RetryError::permanent(e)
                    }
                })
            }
        },
        |e: &DatabaseError, _| {
            tracing::warn!("database write hit lock contention, retrying: {e}");
        },
    )
    .await
}

/// Parameters for creating a task record.
///
/// See [`Database::create_task`].
#[derive(Debug, Clone, Copy)]
pub struct NewTask<'a> {
    /// The unique name of the task's execution attempt.
    pub name: &'a str,
    /// The UUID of the run managing this task.
    pub run_id: Uuid,
    /// The starting status of the task.
    pub status: TaskStatus,
    /// The stable WDL call path shared by every attempt of the same call.
    ///
    /// `None` when the creation is driven by a backend event, which does not
    /// carry the call id.
    pub call_id: Option<&'a str>,
    /// The 0-based execution attempt number within the call.
    pub attempt: i64,
}

impl<'a> NewTask<'a> {
    /// Creates task parameters for a task observed through a backend event,
    /// which carries neither a call id nor an attempt number.
    pub fn from_backend_event(name: &'a str, run_id: Uuid, status: TaskStatus) -> Self {
        Self {
            name,
            run_id,
            status,
            call_id: None,
            attempt: 0,
        }
    }
}

/// A single buffered database write associated with a task.
///
/// `TaskMonitorSvc` constructs these as it observes engine and backend
/// events and periodically flushes a batch of them via
/// [`Database::apply_task_writes`], rather than issuing one write per event.
/// Each variant owns its data (rather than borrowing, as the trait's
/// per-operation methods do) since a write may sit in the buffer across
/// several `.await` points before it is flushed.
#[derive(Debug, Clone)]
pub enum TaskWrite {
    /// Create a new task row (or fold into an existing one; see
    /// [`Database::create_task`]).
    Create {
        /// The unique name of the task's execution attempt.
        name: String,
        /// The UUID of the run managing this task.
        run_id: Uuid,
        /// The starting status of the task.
        status: TaskStatus,
        /// The stable WDL call path shared by every attempt of the same
        /// call, if known.
        call_id: Option<String>,
        /// The 0-based execution attempt number within the call.
        attempt: i64,
    },
    /// Advance a task to localizing (see
    /// [`Database::update_task_localizing`]).
    Localizing {
        /// The task's name.
        name: String,
    },
    /// Record resolved execution constraints (see
    /// [`Database::update_task_constraints`]).
    Constraints {
        /// The task's name.
        name: String,
        /// The serialized constraints.
        constraints: String,
    },
    /// Record why a task's execution attempt was retried (see
    /// [`Database::update_task_retry_cause`]).
    RetryCause {
        /// The task's name.
        name: String,
        /// The serialized retry cause.
        cause: String,
    },
    /// Merge newly observed resource-utilization fields (see
    /// [`Database::update_task_utilization`]).
    Utilization {
        /// The task's name.
        name: String,
        /// The JSON Merge Patch fragment to apply.
        patch: String,
    },
    /// Advance a task to pending (see [`Database::update_task_pending`]).
    Pending {
        /// The task's name.
        name: String,
    },
    /// Advance a task to cached (see [`Database::update_task_cached`]).
    Cached {
        /// The task's name.
        name: String,
        /// When the cached result was observed.
        completed_at: DateTime<Utc>,
    },
    /// Advance a task to running (see [`Database::update_task_started`]).
    Started {
        /// The task's name.
        name: String,
        /// When the task started running.
        started_at: DateTime<Utc>,
    },
    /// Advance a task to completed (see [`Database::update_task_completed`]).
    Completed {
        /// The task's name.
        name: String,
        /// The task's exit status, if any.
        exit_status: Option<i32>,
        /// When the task completed.
        completed_at: DateTime<Utc>,
    },
    /// Advance a task to failed (see [`Database::update_task_failed`]).
    Failed {
        /// The task's name.
        name: String,
        /// The error message.
        error: String,
        /// When the task failed.
        completed_at: DateTime<Utc>,
    },
    /// Advance a task to canceled (see [`Database::update_task_canceled`]).
    Canceled {
        /// The task's name.
        name: String,
        /// When the task was canceled.
        completed_at: DateTime<Utc>,
    },
    /// Advance a task to preempted (see
    /// [`Database::update_task_preempted`]).
    Preempted {
        /// The task's name.
        name: String,
        /// When the task was preempted.
        completed_at: DateTime<Utc>,
    },
    /// Append a chunk of task log output (see
    /// [`Database::insert_task_log`]).
    Log {
        /// The task's name.
        name: String,
        /// The log's source stream.
        source: LogSource,
        /// The logged bytes.
        chunk: Vec<u8>,
    },
}

/// A database trait containing needed provenance operations.
#[async_trait]
pub trait Database: Send + Sync {
    /// Create a new session.
    async fn create_session(
        &self,
        id: Uuid,
        subcommand: SprocketCommand,
        created_by: &str,
    ) -> Result<Session>;

    /// Get a session by ID.
    async fn get_session(&self, id: Uuid) -> Result<Option<Session>>;

    /// Get a session by ID, returning an error if it does not exist.
    async fn read_session(&self, id: Uuid) -> Result<Session> {
        self.get_session(id)
            .await?
            .ok_or_else(|| DatabaseError::NotFound(format!("the run with id `{id}` was not found")))
    }

    /// List sessions.
    async fn list_sessions(&self, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<Session>>;

    /// Count total sessions.
    async fn count_sessions(&self) -> Result<i64>;

    /// List sessions and return the total count before pagination.
    async fn read_sessions(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<ReadPage<Session>> {
        let (records, total) =
            tokio::try_join!(self.list_sessions(limit, offset), self.count_sessions())?;
        let total = page_total(&records, total, offset);
        Ok(ReadPage { records, total })
    }

    /// Create a new run.
    ///
    /// The `target` parameter is `None` when the user did not provide a target.
    /// The resolved target should be set later via
    /// [`update_run_target`](Self::update_run_target).
    async fn create_run(
        &self,
        id: Uuid,
        session_id: Uuid,
        name: &str,
        source: &str,
        target: Option<&str>,
        inputs: &str,
    ) -> Result<Run>;

    /// Update run target after resolution.
    ///
    /// Returns `true` if a run was updated, `false` if the run was not found.
    #[must_use = "the return value indicates whether a run was updated"]
    async fn update_run_target(&self, id: Uuid, target: &str) -> Result<bool>;

    /// Update run status.
    async fn update_run_status(&self, id: Uuid, status: RunStatus) -> Result<()>;

    /// Update run started at.
    async fn update_run_started_at(
        &self,
        id: Uuid,
        started_at: Option<DateTime<Utc>>,
    ) -> Result<()>;

    /// Update run completed at.
    async fn update_run_completed_at(
        &self,
        id: Uuid,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<()>;

    /// Update run outputs.
    async fn update_run_outputs(&self, id: Uuid, outputs: &str) -> Result<()>;

    /// Update run error.
    async fn update_run_error(&self, id: Uuid, error: &str) -> Result<()>;

    /// Update run directory.
    ///
    /// Returns `true` if a run was updated, `false` if the run was not found.
    #[must_use = "the return value indicates whether a run was updated"]
    async fn update_run_directory(&self, id: Uuid, directory: &str) -> Result<bool>;

    /// Update run index directory.
    ///
    /// Returns `true` if a run was updated, `false` if the run was
    /// not found.
    #[must_use = "the return value indicates whether a run was updated"]
    async fn update_run_index_directory(&self, id: Uuid, index_directory: &str) -> Result<bool>;

    /// Get a run by ID.
    async fn get_run(&self, id: Uuid) -> Result<Option<Run>>;

    /// Get a run by ID, returning an error if it does not exist.
    async fn read_run(&self, id: Uuid) -> Result<Run> {
        self.get_run(id)
            .await?
            .ok_or_else(|| DatabaseError::NotFound(format!("run not found: `{id}`")))
    }

    /// Get parsed outputs for a run.
    async fn read_run_outputs(&self, id: Uuid) -> Result<Option<Value>> {
        let run = self.get_run(id).await?.ok_or_else(|| {
            DatabaseError::NotFound(format!("the run with id `{id}` was not found"))
        })?;

        Ok(run
            .outputs
            .as_ref()
            .and_then(|outputs| serde_json::from_str(outputs).ok()))
    }

    /// List runs with optional filtering and pagination.
    async fn list_runs(
        &self,
        status: Option<RunStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Run>>;

    /// Count runs with optional filtering.
    async fn count_runs(&self, status: Option<RunStatus>) -> Result<i64>;

    /// List runs and return the total count before pagination.
    async fn read_runs(
        &self,
        status: Option<RunStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<ReadPage<Run>> {
        let (records, total) = tokio::try_join!(
            self.list_runs(status, limit, offset),
            self.count_runs(status)
        )?;
        let total = page_total(&records, total, offset);
        Ok(ReadPage { records, total })
    }

    /// List runs by session ID.
    async fn list_runs_by_session(&self, session_id: Uuid) -> Result<Vec<Run>>;

    /// Create an index log entry.
    async fn create_index_log_entry(
        &self,
        run_id: Uuid,
        link_path: &str,
        target_path: &str,
    ) -> Result<IndexLogEntry>;

    /// List index log entries by run ID.
    async fn list_index_log_entries_by_run(&self, run_id: Uuid) -> Result<Vec<IndexLogEntry>>;

    /// List the latest index log entry for each unique index path.
    async fn list_latest_index_entries(&self) -> Result<Vec<IndexLogEntry>>;

    /// Create a task record in the given starting status.
    ///
    /// Creation is idempotent: if the task already exists, its current record
    /// and status are returned unchanged, except that a `call_id` is filled in
    /// when the existing row has none and the `attempt` number is raised to
    /// the larger of the two. Engine and Crankshaft events arrive on
    /// independent channels with no ordering between them, so either may be
    /// the first to observe a task, and only the engine's announcement carries
    /// the call id and attempt number.
    async fn create_task(&self, task: NewTask<'_>) -> Result<Task>;

    /// Record the resolved execution constraints for a task.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_constraints(&self, name: &str, constraints: &str) -> Result<bool>;

    /// Record why a task's execution attempt was retried.
    ///
    /// This is set on the row of the attempt that failed, which has usually
    /// already reached a terminal status, so no status guard applies.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_retry_cause(&self, name: &str, cause: &str) -> Result<bool>;

    /// Merge newly observed resource utilization fields for a task into
    /// whatever has already been recorded.
    ///
    /// `patch` is a JSON object containing only the fields being reported;
    /// it is merged into the existing `utilization` column server-side (as a
    /// [JSON Merge Patch](https://www.rfc-editor.org/rfc/rfc7386)) rather
    /// than replacing it outright, so fields reported by one source (e.g.
    /// the engine) are not clobbered by a later report from another source
    /// (e.g. the backend) that doesn't set them. Callers must omit any key
    /// whose value is not currently known, since a JSON Merge Patch
    /// interprets an explicit `null` as "delete this key" rather than "no
    /// information."
    ///
    /// This is recorded at the attempt's termination, which races the
    /// completion event on the other channel, so the row is usually already
    /// in a terminal status and no status guard applies.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_utilization(&self, name: &str, patch: &str) -> Result<bool>;

    /// Advance a task to localizing its inputs.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found or
    /// has already advanced past initializing.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_localizing(&self, name: &str) -> Result<bool>;

    /// Advance a task to pending, meaning it has been submitted to a backend
    /// and is awaiting scheduling.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found or
    /// has already advanced past localizing.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_pending(&self, name: &str) -> Result<bool>;

    /// Update a task as served from the call cache.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found or
    /// has already reached a terminal status.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_cached(&self, name: &str, completed_at: DateTime<Utc>) -> Result<bool>;

    /// Update task with started timestamp.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found or
    /// has already advanced past pending.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_started(&self, name: &str, started_at: DateTime<Utc>) -> Result<bool>;

    /// Update task with completion data.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found or
    /// has already reached a terminal status.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_completed(
        &self,
        name: &str,
        exit_status: Option<i32>,
        completed_at: DateTime<Utc>,
    ) -> Result<bool>;

    /// Update task with failure data.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found or
    /// has already reached a terminal status.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_failed(
        &self,
        name: &str,
        error: &str,
        completed_at: DateTime<Utc>,
    ) -> Result<bool>;

    /// Update task as canceled.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found or
    /// has already reached a terminal status.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_canceled(&self, name: &str, completed_at: DateTime<Utc>) -> Result<bool>;

    /// Update task as preempted.
    ///
    /// Returns `true` if a task was updated, `false` if it was not found or
    /// has already reached a terminal status.
    #[must_use = "the return value indicates whether a task was updated"]
    async fn update_task_preempted(&self, name: &str, completed_at: DateTime<Utc>) -> Result<bool>;

    /// Get task by name.
    async fn get_task(&self, name: &str) -> Result<Task>;

    /// Get a task by name.
    async fn read_task(&self, name: &str) -> Result<Task> {
        self.get_task(name).await
    }

    /// List all tasks with pagination and optional filters.
    async fn list_tasks(
        &self,
        run_id: Option<Uuid>,
        status: Option<TaskStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Task>>;

    /// Count total tasks with optional filters.
    async fn count_tasks(&self, run_id: Option<Uuid>, status: Option<TaskStatus>) -> Result<i64>;

    /// List tasks and return the total count before pagination.
    async fn read_tasks(
        &self,
        run_id: Option<Uuid>,
        status: Option<TaskStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<ReadPage<Task>> {
        let (records, total) = tokio::try_join!(
            self.list_tasks(run_id, status, limit, offset),
            self.count_tasks(run_id, status)
        )?;
        let total = page_total(&records, total, offset);
        Ok(ReadPage { records, total })
    }

    /// Count the tasks of a run grouped by status.
    ///
    /// Only statuses that have at least one task are returned.
    async fn count_tasks_by_status(&self, run_id: Uuid) -> Result<Vec<(TaskStatus, i64)>>;

    /// Count the tasks of a run grouped by status.
    async fn read_run_task_counts(&self, run_id: Uuid) -> Result<Vec<(TaskStatus, i64)>> {
        self.count_tasks_by_status(run_id).await
    }

    /// Insert a task log entry.
    async fn insert_task_log(&self, task_name: &str, source: LogSource, chunk: &[u8])
    -> Result<()>;

    /// Get task logs with pagination and optional source filter.
    async fn get_task_logs(
        &self,
        task_name: &str,
        source: Option<LogSource>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<TaskLog>>;

    /// Count task logs with optional source filter.
    async fn count_task_logs(&self, task_name: &str, source: Option<LogSource>) -> Result<i64>;

    /// Get task logs and return the total count before pagination.
    async fn read_task_logs(
        &self,
        task_name: &str,
        source: Option<LogSource>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<ReadPage<TaskLog>> {
        self.read_task(task_name).await?;
        let (records, total) = tokio::try_join!(
            self.get_task_logs(task_name, source, limit, offset),
            self.count_task_logs(task_name, source)
        )?;
        let total = page_total(&records, total, offset);
        Ok(ReadPage { records, total })
    }

    /// Apply a batch of buffered task writes together.
    ///
    /// `TaskMonitorSvc` buffers the task-related writes it would otherwise
    /// issue one at a time (task creation, status transitions, utilization,
    /// and log chunks) and periodically flushes them through this method
    /// instead, so that many logically-separate operations collected over a
    /// short window share a single commit rather than paying its cost once
    /// per event. Writes are applied in the order they appear in `writes`,
    /// which preserves per-task ordering since the monitor only ever
    /// appends to the buffer in the order it observes events.
    ///
    /// The default implementation preserves the same effect (but not the
    /// commit-count reduction) by calling the corresponding per-operation
    /// method on this trait once for each buffered write, with no shared
    /// transaction; backends should override this method to apply the
    /// whole batch within a single transaction.
    async fn apply_task_writes(&self, writes: Vec<TaskWrite>) -> Result<()> {
        for write in writes {
            match write {
                TaskWrite::Create {
                    name,
                    run_id,
                    status,
                    call_id,
                    attempt,
                } => {
                    self.create_task(NewTask {
                        name: &name,
                        run_id,
                        status,
                        call_id: call_id.as_deref(),
                        attempt,
                    })
                    .await?;
                }
                TaskWrite::Localizing { name } => {
                    self.update_task_localizing(&name).await?;
                }
                TaskWrite::Constraints { name, constraints } => {
                    self.update_task_constraints(&name, &constraints).await?;
                }
                TaskWrite::RetryCause { name, cause } => {
                    self.update_task_retry_cause(&name, &cause).await?;
                }
                TaskWrite::Utilization { name, patch } => {
                    self.update_task_utilization(&name, &patch).await?;
                }
                TaskWrite::Pending { name } => {
                    self.update_task_pending(&name).await?;
                }
                TaskWrite::Cached { name, completed_at } => {
                    self.update_task_cached(&name, completed_at).await?;
                }
                TaskWrite::Started { name, started_at } => {
                    self.update_task_started(&name, started_at).await?;
                }
                TaskWrite::Completed {
                    name,
                    exit_status,
                    completed_at,
                } => {
                    self.update_task_completed(&name, exit_status, completed_at)
                        .await?;
                }
                TaskWrite::Failed {
                    name,
                    error,
                    completed_at,
                } => {
                    self.update_task_failed(&name, &error, completed_at).await?;
                }
                TaskWrite::Canceled { name, completed_at } => {
                    self.update_task_canceled(&name, completed_at).await?;
                }
                TaskWrite::Preempted { name, completed_at } => {
                    self.update_task_preempted(&name, completed_at).await?;
                }
                TaskWrite::Log {
                    name,
                    source,
                    chunk,
                } => {
                    self.insert_task_log(&name, source, &chunk).await?;
                }
            }
        }
        Ok(())
    }

    /// Transition a run to `Running` status with `started_at` timestamp.
    ///
    /// The default implementation performs two separate writes (one commit
    /// each); every run takes this path, so implementations backed by a
    /// database that pays a high per-commit cost (e.g. SQLite on a network
    /// filesystem) should override this to perform both writes in a single
    /// statement/transaction instead.
    async fn start_run(&self, id: Uuid, started_at: DateTime<Utc>) -> Result<()> {
        self.update_run_status(id, RunStatus::Running).await?;
        self.update_run_started_at(id, Some(started_at)).await?;
        Ok(())
    }

    /// Transition a run to `Completed` status with `completed_at` timestamp.
    ///
    /// See the note on [`start_run`](Self::start_run) about overriding for a
    /// single-write implementation.
    async fn complete_run(&self, id: Uuid, completed_at: DateTime<Utc>) -> Result<()> {
        self.update_run_status(id, RunStatus::Completed).await?;
        self.update_run_completed_at(id, Some(completed_at)).await?;
        Ok(())
    }

    /// Transition a run to `Failed` status with error message and
    /// `completed_at` timestamp.
    ///
    /// See the note on [`start_run`](Self::start_run) about overriding for a
    /// single-write implementation.
    async fn fail_run(&self, id: Uuid, error: &str, completed_at: DateTime<Utc>) -> Result<()> {
        self.update_run_status(id, RunStatus::Failed).await?;
        self.update_run_error(id, error).await?;
        self.update_run_completed_at(id, Some(completed_at)).await?;
        Ok(())
    }

    /// Transition a run to `Canceled` status with `completed_at` timestamp.
    ///
    /// See the note on [`start_run`](Self::start_run) about overriding for a
    /// single-write implementation.
    async fn cancel_run(&self, id: Uuid, completed_at: DateTime<Utc>) -> Result<()> {
        self.update_run_status(id, RunStatus::Canceled).await?;
        self.update_run_completed_at(id, Some(completed_at)).await?;
        Ok(())
    }

    /// Transition a run to `Completed` status with `completed_at` timestamp
    /// and record its outputs.
    ///
    /// This combines what would otherwise be two separate writes
    /// ([`update_run_outputs`](Self::update_run_outputs) followed by
    /// [`complete_run`](Self::complete_run)) into a single logical
    /// operation, halving the commits needed on the always-executed
    /// run-success path. The default implementation still performs the two
    /// underlying writes (for backends that haven't overridden either);
    /// implementations should override this to perform both writes in a
    /// single transaction/statement where possible (see the note on
    /// [`start_run`](Self::start_run)).
    async fn complete_run_with_outputs(
        &self,
        id: Uuid,
        completed_at: DateTime<Utc>,
        outputs: &str,
    ) -> Result<()> {
        self.update_run_outputs(id, outputs).await?;
        self.complete_run(id, completed_at).await?;
        Ok(())
    }

    /// Records a liveness heartbeat on a session.
    ///
    /// `sprocket server` and `sprocket run` use this to keep their sessions
    /// live.
    async fn heartbeat_session(&self, id: Uuid, at: DateTime<Utc>) -> Result<()>;

    /// Marks non-terminal runs and their tasks owned by a stale session
    /// `Orphaned`.
    ///
    /// A session is stale when it records no heartbeat within `timeout`; one
    /// that never recorded one uses its `created_at`. Its owner can no longer
    /// drive or cancel its runs. Live owners keep their sessions fresh, so this
    /// is safe to run continuously across processes sharing one database.
    ///
    /// Implementations must use a bulk statement per table to avoid the
    /// [`list_runs`](Self::list_runs) page limit.
    ///
    /// Returns the number of runs marked orphaned.
    async fn mark_orphaned_runs(
        &self,
        error: &str,
        timeout: Duration,
        now: DateTime<Utc>,
    ) -> Result<u64>;
    /// Transition a run to `Canceling` status.
    ///
    /// Returns `true` if the run was updated, `false` if it was not found or
    /// has already reached a terminal status.
    ///
    /// Cancellation is requested by signaling the run and then recording the
    /// request, so a run that finishes in between — which is the common case
    /// when the work being canceled is a transfer rather than a task — would
    /// otherwise have its outcome overwritten by the request to cancel it, and
    /// would appear to be canceling forever.
    #[must_use = "the return value indicates whether a run was updated"]
    async fn mark_run_canceling(&self, id: Uuid) -> Result<bool>;
}
