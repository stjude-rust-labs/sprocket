//! SQLite database implementation.

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use chrono::DateTime;
use chrono::Utc;
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::sqlite::SqliteJournalMode;
use sqlx::sqlite::SqliteSynchronous;
use uuid::Uuid;

use super::Database;
use super::DatabaseError;
use super::Result;
use super::models::IndexLogEntry;
use super::models::LogSource;
use super::models::Run;
use super::models::RunStatus;
use super::models::Session;
use super::models::SprocketCommand;
use super::models::Task;
use super::models::TaskLog;
use super::models::TaskStatus;

/// Default page size for pagination.
const DEFAULT_PAGE_SIZE: i64 = 100;

/// Default offset for pagination.
const DEFAULT_OFFSET: i64 = 0;

/// SQLite connection string prefix.
const SQLITE_CONNECTION_PREFIX: &str = "sqlite:";

/// Store temporary tables and indices in memory for faster operations.
const SQLITE_TEMP_STORE: &str = "memory";

/// Set memory-mapped I/O size to 4GiB for improved read performance.
const SQLITE_MMAP_SIZE: &str = "4294967296";

/// Set page size to 32KB to reduce I/O operations for sequential scans.
const SQLITE_PAGE_SIZE: &str = "32768";

/// Enable foreign key constraint enforcement for referential integrity.
const SQLITE_FOREIGN_KEYS: &str = "on";

/// Metadata key for the Sprocket directory schema version.
const VERSION_KEY: &str = "version";

/// Expected Sprocket directory schema version.
const EXPECTED_VERSION: &str = "1";

/// Configure 30-second timeout when database is locked to allow concurrent
/// processes time to complete their writes under heavy parallel access.
const SQLITE_BUSY_TIMEOUT: &str = "30000";

/// Allocate approximately 8MB for SQLite page cache for improved query
/// performance.
const SQLITE_CACHE_SIZE: &str = "2000";

/// SQLite database implementation.
#[derive(Debug, Clone)]
pub struct SqliteDatabase {
    /// The underlying SQLite connection pool.
    pool: SqlitePool,
}

impl SqliteDatabase {
    /// Create a new SQLite database connection from a path.
    ///
    /// Migrations are run upon a successful connection pool being established.
    pub async fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let database_url = format!("{}//{}", SQLITE_CONNECTION_PREFIX, path.display());
        let options = SqliteConnectOptions::from_str(&database_url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Delete)
            .synchronous(SqliteSynchronous::Normal)
            .pragma("temp_store", SQLITE_TEMP_STORE)
            .pragma("mmap_size", SQLITE_MMAP_SIZE)
            .pragma("page_size", SQLITE_PAGE_SIZE)
            .pragma("foreign_keys", SQLITE_FOREIGN_KEYS)
            .pragma("busy_timeout", SQLITE_BUSY_TIMEOUT)
            .pragma("cache_size", SQLITE_CACHE_SIZE);

        let pool = SqlitePool::connect_with(options).await?;
        Self::from_pool(pool).await
    }

    /// Creates a new SQLite connection from an existing pool.
    ///
    /// This method also runs the embedded migrations.
    pub async fn from_pool(pool: SqlitePool) -> Result<Self> {
        sqlx::migrate!("./migrations").run(&pool).await?;

        // Check or initialize version metadata element
        let version: Option<String> =
            sqlx::query_scalar("select value from metadata where key = ?")
                .bind(VERSION_KEY)
                .fetch_optional(&pool)
                .await?;

        match version {
            None => {
                // Initialize version metadata element
                sqlx::query("insert into metadata (key, value) values (?, ?)")
                    .bind(VERSION_KEY)
                    .bind(EXPECTED_VERSION)
                    .execute(&pool)
                    .await?;
            }
            Some(ref v) if v == EXPECTED_VERSION => {
                // Version matches, all good
            }
            Some(v) => {
                return Err(DatabaseError::InvalidVersion {
                    expected: EXPECTED_VERSION.to_string(),
                    found: v,
                });
            }
        }

        Ok(Self { pool })
    }

    /// Get the underlying connection pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl Database for SqliteDatabase {
    async fn create_session(
        &self,
        id: Uuid,
        subcommand: SprocketCommand,
        created_by: &str,
    ) -> Result<Session> {
        debug_assert!(
            !created_by.is_empty(),
            "`created_by` cannot be empty for a session"
        );

        let session: Session = sqlx::query_as(
            "insert into sessions (uuid, subcommand, created_by) values (?, ?, ?) returning uuid, \
             subcommand, created_by, created_at, heartbeat_at",
        )
        .bind(id.to_string())
        .bind(subcommand)
        .bind(created_by)
        .fetch_one(&self.pool)
        .await?;

        Ok(session)
    }

    async fn get_session(&self, id: Uuid) -> Result<Option<Session>> {
        let session: Option<Session> = sqlx::query_as(
            "select uuid, subcommand, created_by, created_at, heartbeat_at from sessions where \
             uuid = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        Ok(session)
    }

    async fn list_sessions(&self, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<Session>> {
        let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE);
        let offset = offset.unwrap_or(DEFAULT_OFFSET);

        let sessions: Vec<Session> = sqlx::query_as(
            "select uuid, subcommand, created_by, created_at, heartbeat_at from sessions order by \
             created_at desc limit ? offset ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(sessions)
    }

    async fn count_sessions(&self) -> Result<i64> {
        let count: (i64,) = sqlx::query_as("select count(*) from sessions")
            .fetch_one(&self.pool)
            .await?;

        Ok(count.0)
    }

    async fn create_run(
        &self,
        id: Uuid,
        session_id: Uuid,
        name: &str,
        source: &str,
        target: Option<&str>,
        inputs: &str,
    ) -> Result<Run> {
        debug_assert!(!name.is_empty(), "`name` cannot be empty for a run");
        debug_assert!(!source.is_empty(), "`source` cannot be empty for a run");

        let run: Run = sqlx::query_as(
            "insert into runs (uuid, session_id, name, source, target, status, inputs) select ?, \
             s.id, ?, ?, ?, ?, ? from sessions s where s.uuid = ? returning uuid, (select uuid \
             from sessions where id = session_id) as session_uuid, name, source, target, status, \
             inputs, outputs, error, directory, index_directory, started_at, completed_at, \
             created_at",
        )
        .bind(id.to_string())
        .bind(name)
        .bind(source)
        .bind(target)
        .bind(RunStatus::Queued)
        .bind(inputs)
        .bind(session_id.to_string())
        .fetch_one(&self.pool)
        .await?;

        Ok(run)
    }

    async fn update_run_target(&self, id: Uuid, target: &str) -> Result<bool> {
        let result = sqlx::query("update runs set target = ? where uuid = ?")
            .bind(target)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_run_status(&self, id: Uuid, status: RunStatus) -> Result<()> {
        sqlx::query("update runs set status = ? where uuid = ?")
            .bind(status)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn mark_run_canceling(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query(
            "update runs
             set status = ?
             where uuid = ? and status not in ('completed', 'failed', 'canceled')",
        )
        .bind(RunStatus::Canceling)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_run_started_at(
        &self,
        id: Uuid,
        started_at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        sqlx::query("update runs set started_at = ? where uuid = ?")
            .bind(started_at)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_run_completed_at(
        &self,
        id: Uuid,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        sqlx::query("update runs set completed_at = ? where uuid = ?")
            .bind(completed_at)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_run_outputs(&self, id: Uuid, outputs: &str) -> Result<()> {
        sqlx::query("update runs set outputs = ? where uuid = ?")
            .bind(outputs)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_run_error(&self, id: Uuid, error: &str) -> Result<()> {
        sqlx::query("update runs set error = ? where uuid = ?")
            .bind(error)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_run_directory(&self, id: Uuid, directory: &str) -> Result<bool> {
        let result = sqlx::query("update runs set directory = ? where uuid = ?")
            .bind(directory)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_run_index_directory(&self, id: Uuid, index_directory: &str) -> Result<bool> {
        let result = sqlx::query("update runs set index_directory = ? where uuid = ?")
            .bind(index_directory)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_run(&self, id: Uuid) -> Result<Option<Run>> {
        let run: Option<Run> = sqlx::query_as(
            "select r.uuid, s.uuid as session_uuid, r.name, r.source, r.target, r.status, \
             r.inputs, r.outputs, r.error, r.directory, r.index_directory, r.started_at, \
             r.completed_at, r.created_at from runs r join sessions s on r.session_id = s.id \
             where r.uuid = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        Ok(run)
    }

    async fn list_runs(
        &self,
        status: Option<RunStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Run>> {
        let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE);
        let offset = offset.unwrap_or(DEFAULT_OFFSET);

        let runs: Vec<Run> = if let Some(status) = status {
            sqlx::query_as(
                "select r.uuid, s.uuid as session_uuid, r.name, r.source, r.target, r.status, \
                 r.inputs, r.outputs, r.error, r.directory, r.index_directory, r.started_at, \
                 r.completed_at, r.created_at from runs r join sessions s on r.session_id = s.id \
                 where r.status = ? order by r.created_at desc limit ? offset ?",
            )
            .bind(status)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as(
                "select r.uuid, s.uuid as session_uuid, r.name, r.source, r.target, r.status, \
                 r.inputs, r.outputs, r.error, r.directory, r.index_directory, r.started_at, \
                 r.completed_at, r.created_at from runs r join sessions s on r.session_id = s.id \
                 order by r.created_at desc limit ? offset ?",
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(runs)
    }

    async fn count_runs(&self, status: Option<RunStatus>) -> Result<i64> {
        let count: (i64,) = if let Some(status) = status {
            sqlx::query_as("select count(*) from runs where status = ?")
                .bind(status)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_as("select count(*) from runs")
                .fetch_one(&self.pool)
                .await?
        };

        Ok(count.0)
    }

    async fn list_runs_by_session(&self, session_id: Uuid) -> Result<Vec<Run>> {
        let runs: Vec<Run> = sqlx::query_as(
            "select r.uuid, s.uuid as session_uuid, r.name, r.source, r.target, r.status, \
             r.inputs, r.outputs, r.error, r.directory, r.index_directory, r.started_at, \
             r.completed_at, r.created_at from runs r join sessions s on r.session_id = s.id \
             where s.uuid = ? order by r.created_at",
        )
        .bind(session_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        Ok(runs)
    }

    async fn heartbeat_session(&self, id: Uuid, at: DateTime<Utc>) -> Result<()> {
        sqlx::query("update sessions set heartbeat_at = ? where uuid = ?")
            .bind(at)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn mark_orphaned_runs(
        &self,
        error: &str,
        timeout: Duration,
        now: DateTime<Utc>,
    ) -> Result<u64> {
        // Saturating: a timeout large enough to underflow the epoch would
        // otherwise panic, and a cutoff at the earliest representable instant
        // matches nothing, which errs toward sweeping less.
        let cutoff = now - chrono::Duration::from_std(timeout).unwrap_or(chrono::Duration::MAX);

        let mut tx = self.pool.begin().await?;

        // `coalesce(heartbeat_at, created_at)` treats a session that never
        // recorded a heartbeat as stale since creation; those predate
        // heartbeat support, so their process cannot still be running. A
        // `sprocket run` from an older binary is swept early for the same
        // reason, until it writes its own terminal status.
        //
        // Both sides go through `unixepoch` because the columns carry
        // different text encodings: `created_at` defaults to SQLite's
        // `current_timestamp` (`YYYY-MM-DD HH:MM:SS`) while `heartbeat_at` and
        // the cutoff are RFC 3339 from sqlx. Compared as text they sort on the
        // separator (`' ' < 'T'`), which would orphan live runs immediately.
        //
        // Tasks are marked first so they don't linger as `pending`/`running`
        // under an `orphaned` run; this reads the runs' pre-update status.
        sqlx::query(
            "update tasks set status = ?, error = ?, completed_at = ? where status in ('pending', \
             'running') and run_id in (select r.id from runs r join sessions s on r.session_id = \
             s.id where unixepoch(coalesce(s.heartbeat_at, s.created_at)) < unixepoch(?) and \
             r.status in ('queued', 'running', 'canceling'))",
        )
        .bind(TaskStatus::Orphaned)
        .bind(error)
        .bind(now)
        .bind(cutoff)
        .execute(&mut *tx)
        .await?;

        // A single bulk statement (not `list_runs`, which pages) so this
        // scales regardless of how many runs were orphaned.
        let result = sqlx::query(
            "update runs set status = ?, error = ?, completed_at = ? where status in ('queued', \
             'running', 'canceling') and session_id in (select id from sessions where \
             unixepoch(coalesce(heartbeat_at, created_at)) < unixepoch(?))",
        )
        .bind(RunStatus::Orphaned)
        .bind(error)
        .bind(now)
        .bind(cutoff)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(result.rows_affected())
    }

    async fn create_index_log_entry(
        &self,
        run_id: Uuid,
        link_path: &str,
        target_path: &str,
    ) -> Result<IndexLogEntry> {
        let entry: IndexLogEntry = sqlx::query_as(
            "insert into index_log (run_id, link_path, target_path) select r.id, ?, ? from runs r \
             where r.uuid = ? returning id, (select uuid from runs where id = run_id) as \
             run_uuid, link_path, target_path, created_at",
        )
        .bind(link_path)
        .bind(target_path)
        .bind(run_id.to_string())
        .fetch_one(&self.pool)
        .await?;

        Ok(entry)
    }

    async fn list_index_log_entries_by_run(&self, run_id: Uuid) -> Result<Vec<IndexLogEntry>> {
        let entries: Vec<IndexLogEntry> = sqlx::query_as(
            "select i.id, r.uuid as run_uuid, i.link_path, i.target_path, i.created_at from \
             index_log i join runs r on i.run_id = r.id where r.uuid = ? order by i.created_at",
        )
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        Ok(entries)
    }

    async fn list_latest_index_entries(&self) -> Result<Vec<IndexLogEntry>> {
        let entries: Vec<IndexLogEntry> = sqlx::query_as(
            "select i.id, r.uuid as run_uuid, i.link_path, i.target_path, i.created_at from \
             latest_index_entries i join runs r on i.run_id = r.id",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(entries)
    }

    async fn create_task(&self, name: &str, run_id: Uuid, status: TaskStatus) -> Result<Task> {
        // The upsert makes this idempotent: a task row is created by whichever
        // event observes the task first, and any later creation leaves the row
        // and its status untouched.
        let task: Task = sqlx::query_as(
            "insert into tasks (name, run_id, status) select ?, r.id, ? from runs r where r.uuid \
             = ? on conflict(\"name\") do update set run_id = tasks.run_id returning name, \
             (select uuid from runs where id = run_id) as run_uuid, status, exit_status, error, \
             created_at, started_at, completed_at",
        )
        .bind(name)
        .bind(status)
        .bind(run_id.to_string())
        .fetch_one(&self.pool)
        .await?;

        Ok(task)
    }

    async fn update_task_localizing(&self, name: &str) -> Result<bool> {
        let result = sqlx::query(
            "update tasks
             set status = ?
             where name = ? and status = 'initializing'",
        )
        .bind(TaskStatus::Localizing)
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_task_pending(&self, name: &str) -> Result<bool> {
        let result = sqlx::query(
            "update tasks
             set status = ?
             where name = ? and status in ('initializing', 'localizing')",
        )
        .bind(TaskStatus::Pending)
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_task_cached(&self, name: &str, completed_at: DateTime<Utc>) -> Result<bool> {
        let result = sqlx::query(
            "update tasks
             set status = ?, completed_at = ?
             where name = ? and status not in ('completed', 'failed', 'canceled', 'preempted', \
             'cached')",
        )
        .bind(TaskStatus::Cached)
        .bind(completed_at)
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_task_started(&self, name: &str, started_at: DateTime<Utc>) -> Result<bool> {
        let result = sqlx::query(
            "update tasks
             set status = ?, started_at = ?
             where name = ? and status in ('initializing', 'localizing', 'pending')",
        )
        .bind(TaskStatus::Running)
        .bind(started_at)
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_task_completed(
        &self,
        name: &str,
        exit_status: Option<i32>,
        completed_at: DateTime<Utc>,
    ) -> Result<bool> {
        let result = sqlx::query(
            "update tasks
             set status = ?, exit_status = ?, completed_at = ?
             where name = ? and status not in ('completed', 'failed', 'canceled', 'preempted', \
             'cached')",
        )
        .bind(TaskStatus::Completed)
        .bind(exit_status)
        .bind(completed_at)
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_task_failed(
        &self,
        name: &str,
        error: &str,
        completed_at: DateTime<Utc>,
    ) -> Result<bool> {
        let result = sqlx::query(
            "update tasks
             set status = ?, error = ?, completed_at = ?
             where name = ? and status not in ('completed', 'failed', 'canceled', 'preempted', \
             'cached')",
        )
        .bind(TaskStatus::Failed)
        .bind(error)
        .bind(completed_at)
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_task_canceled(&self, name: &str, completed_at: DateTime<Utc>) -> Result<bool> {
        let result = sqlx::query(
            "update tasks
             set status = ?, completed_at = ?
             where name = ? and status not in ('completed', 'failed', 'canceled', 'preempted', \
             'cached')",
        )
        .bind(TaskStatus::Canceled)
        .bind(completed_at)
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_task_preempted(&self, name: &str, completed_at: DateTime<Utc>) -> Result<bool> {
        let result = sqlx::query(
            "update tasks
             set status = ?, completed_at = ?
             where name = ? and status not in ('completed', 'failed', 'canceled', 'preempted', \
             'cached')",
        )
        .bind(TaskStatus::Preempted)
        .bind(completed_at)
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_task(&self, name: &str) -> Result<Task> {
        let task: Option<Task> = sqlx::query_as(
            "select t.name, r.uuid as run_uuid, t.status, t.exit_status, t.error,
                    t.created_at, t.started_at, t.completed_at
             from tasks t join runs r on t.run_id = r.id
             where t.name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        task.ok_or(DatabaseError::NotFound)
    }

    async fn list_tasks(
        &self,
        run_id: Option<Uuid>,
        status: Option<TaskStatus>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Task>> {
        let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE);
        let offset = offset.unwrap_or(DEFAULT_OFFSET);

        let mut query = String::from(
            "select t.name, r.uuid as run_uuid, t.status, t.exit_status, t.error,
                    t.created_at, t.started_at, t.completed_at
             from tasks t join runs r on t.run_id = r.id where 1=1",
        );

        if run_id.is_some() {
            query.push_str(" and r.uuid = ?");
        }
        if status.is_some() {
            query.push_str(" and t.status = ?");
        }
        query.push_str(" order by t.created_at desc limit ? offset ?");

        let mut q = sqlx::query_as(sqlx::AssertSqlSafe(query));

        if let Some(run_id) = run_id {
            q = q.bind(run_id.to_string());
        }
        if let Some(status) = status {
            q = q.bind(status);
        }
        q = q.bind(limit).bind(offset);

        let tasks: Vec<Task> = q.fetch_all(&self.pool).await?;
        Ok(tasks)
    }

    async fn count_tasks(&self, run_id: Option<Uuid>, status: Option<TaskStatus>) -> Result<i64> {
        let mut query =
            String::from("select count(*) from tasks t join runs r on t.run_id = r.id where 1=1");

        if run_id.is_some() {
            query.push_str(" and r.uuid = ?");
        }
        if status.is_some() {
            query.push_str(" and t.status = ?");
        }

        let mut q = sqlx::query_scalar(sqlx::AssertSqlSafe(query));

        if let Some(run_id) = run_id {
            q = q.bind(run_id.to_string());
        }
        if let Some(status) = status {
            q = q.bind(status);
        }

        let count: i64 = q.fetch_one(&self.pool).await?;
        Ok(count)
    }

    async fn count_tasks_by_status(&self, run_id: Uuid) -> Result<Vec<(TaskStatus, i64)>> {
        let counts: Vec<(TaskStatus, i64)> = sqlx::query_as(
            "select t.status, count(*) as count
             from tasks t join runs r on t.run_id = r.id
             where r.uuid = ?
             group by t.status",
        )
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        Ok(counts)
    }

    async fn insert_task_log(
        &self,
        task_name: &str,
        source: LogSource,
        chunk: &[u8],
    ) -> Result<()> {
        sqlx::query(
            "insert into task_logs (task_name, source, chunk)
             values (?, ?, ?)",
        )
        .bind(task_name)
        .bind(source)
        .bind(chunk)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_task_logs(
        &self,
        task_name: &str,
        source: Option<LogSource>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<TaskLog>> {
        let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE);
        let offset = offset.unwrap_or(DEFAULT_OFFSET);

        let mut query = String::from(
            "select id, task_name, source, chunk, created_at
             from task_logs
             where task_name = ?",
        );

        if source.is_some() {
            query.push_str(" and source = ?");
        }
        query.push_str(" order by created_at asc limit ? offset ?");

        let mut q = sqlx::query_as(sqlx::AssertSqlSafe(query));
        q = q.bind(task_name);

        if let Some(source) = source {
            q = q.bind(source);
        }
        q = q.bind(limit).bind(offset);

        let logs: Vec<TaskLog> = q.fetch_all(&self.pool).await?;
        Ok(logs)
    }

    async fn count_task_logs(&self, task_name: &str, source: Option<LogSource>) -> Result<i64> {
        let mut query = String::from(
            "select count(*) from task_logs
             where task_name = ?",
        );

        if source.is_some() {
            query.push_str(" and source = ?");
        }

        let mut q = sqlx::query_scalar(sqlx::AssertSqlSafe(query));
        q = q.bind(task_name);

        if let Some(source) = source {
            q = q.bind(source);
        }

        let count: i64 = q.fetch_one(&self.pool).await?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn connect_with_correct_version(pool: SqlitePool) {
        // Manually insert the correct version
        sqlx::query("insert into metadata (key, value) values (?, ?)")
            .bind(VERSION_KEY)
            .bind(EXPECTED_VERSION)
            .execute(&pool)
            .await
            .expect("failed to insert version");

        // Now connect using `from_pool()` — should succeed without error
        let _db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to connect to database with correct version");
    }

    #[sqlx::test]
    async fn connect_with_incorrect_version(pool: SqlitePool) {
        // Manually insert an incorrect version
        let incorrect_version = "999";
        sqlx::query("insert into metadata (key, value) values (?, ?)")
            .bind(VERSION_KEY)
            .bind(incorrect_version)
            .execute(&pool)
            .await
            .expect("failed to insert version");

        // Now connect using `from_pool()` — should fail with `InvalidVersion` error
        let result = SqliteDatabase::from_pool(pool).await;

        assert!(
            result.is_err(),
            "expected error when connecting with incorrect version"
        );

        match result.unwrap_err() {
            DatabaseError::InvalidVersion { expected, found } => {
                assert_eq!(expected, EXPECTED_VERSION);
                assert_eq!(found, incorrect_version);
            }
            other => panic!("expected `InvalidVersion` error, got: {:?}", other),
        }
    }

    #[sqlx::test]
    async fn create_session(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let id = Uuid::new_v4();
        let subcommand = SprocketCommand::Server;
        let created_by = "test-user";

        let session = db
            .create_session(id, subcommand, created_by)
            .await
            .expect("failed to create session");

        assert_eq!(session.uuid, id);
        assert_eq!(session.subcommand, subcommand);
        assert_eq!(session.created_by, created_by);
    }

    #[sqlx::test]
    async fn get_session(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let id = Uuid::new_v4();
        let session = db
            .create_session(id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");

        let retrieved = db
            .get_session(id)
            .await
            .expect("failed to get session")
            .expect("session not found");

        assert_eq!(retrieved.uuid, session.uuid);
        assert_eq!(retrieved.subcommand, session.subcommand);
        assert_eq!(retrieved.created_by, session.created_by);
    }

    #[sqlx::test]
    async fn list_sessions(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        db.create_session(Uuid::new_v4(), SprocketCommand::Run, "user1")
            .await
            .expect("failed to create session");

        db.create_session(Uuid::new_v4(), SprocketCommand::Server, "user2")
            .await
            .expect("failed to create session");

        db.create_session(Uuid::new_v4(), SprocketCommand::Run, "user3")
            .await
            .expect("failed to create session");

        // Test without filtering
        let sessions = db
            .list_sessions(None, None)
            .await
            .expect("failed to list sessions");
        assert_eq!(sessions.len(), 3);

        // Test with limit
        let sessions = db
            .list_sessions(Some(2), None)
            .await
            .expect("failed to list sessions");
        assert_eq!(sessions.len(), 2);

        // Test with offset
        let sessions = db
            .list_sessions(Some(10), Some(1))
            .await
            .expect("failed to list sessions");
        assert_eq!(sessions.len(), 2);
    }

    #[sqlx::test]
    async fn count_sessions(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let count = db.count_sessions().await.expect("failed to count sessions");
        assert_eq!(count, 0);

        db.create_session(Uuid::new_v4(), SprocketCommand::Run, "user1")
            .await
            .expect("failed to create session");

        db.create_session(Uuid::new_v4(), SprocketCommand::Server, "user2")
            .await
            .expect("failed to create session");

        db.create_session(Uuid::new_v4(), SprocketCommand::Run, "user3")
            .await
            .expect("failed to create session");

        let count = db.count_sessions().await.expect("failed to count sessions");
        assert_eq!(count, 3);
    }

    #[sqlx::test]
    async fn create_run(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");

        let run_id = Uuid::new_v4();
        let run = db
            .create_run(
                run_id,
                session_id,
                "test-run",
                "test.wdl",
                Some("test_task"),
                "{}",
            )
            .await
            .expect("failed to create run");

        assert_eq!(run.uuid, run_id);
        assert_eq!(run.session_uuid, session_id);
        assert_eq!(run.name, "test-run");
        assert_eq!(run.source, "test.wdl");
        assert_eq!(run.target, Some(String::from("test_task")));
        assert_eq!(run.status, RunStatus::Queued);
    }

    #[sqlx::test]
    async fn update_run_status(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");

        let run_id = Uuid::new_v4();
        db.create_run(
            run_id,
            session_id,
            "test-run",
            "test.wdl",
            Some("test_task"),
            "{}",
        )
        .await
        .expect("failed to create run");

        db.update_run_status(run_id, RunStatus::Running)
            .await
            .expect("failed to update run status");

        let run = db
            .get_run(run_id)
            .await
            .expect("failed to get run")
            .unwrap();
        assert_eq!(run.status, RunStatus::Running);
    }

    #[sqlx::test]
    async fn mark_run_canceling_never_overwrites_an_outcome(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");

        let run_id = Uuid::new_v4();
        db.create_run(run_id, session_id, "test-run", "test.wdl", Some("t"), "{}")
            .await
            .expect("failed to create run");

        assert!(
            db.mark_run_canceling(run_id)
                .await
                .expect("failed to mark run canceling")
        );

        // A run that reaches its outcome before the cancellation is recorded
        // keeps that outcome; otherwise it would appear to cancel forever.
        db.cancel_run(run_id, Utc::now())
            .await
            .expect("failed to cancel run");
        assert!(
            !db.mark_run_canceling(run_id)
                .await
                .expect("failed to mark run canceling")
        );

        let run = db
            .get_run(run_id)
            .await
            .expect("failed to get run")
            .unwrap();
        assert_eq!(run.status, RunStatus::Canceled);
        assert!(run.completed_at.is_some());
    }

    #[sqlx::test]
    async fn get_run(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");

        let run_id = Uuid::new_v4();
        let created_run = db
            .create_run(
                run_id,
                session_id,
                "test-run",
                "test.wdl",
                Some("test_task"),
                "{}",
            )
            .await
            .expect("failed to create run");

        let retrieved_run = db
            .get_run(run_id)
            .await
            .expect("failed to get run")
            .expect("run not found");

        assert_eq!(retrieved_run.uuid, created_run.uuid);
        assert_eq!(retrieved_run.name, created_run.name);
    }

    #[sqlx::test]
    async fn list_runs(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");

        let run1_id = Uuid::new_v4();
        db.create_run(
            run1_id,
            session_id,
            "run1",
            "test.wdl",
            Some("task_a"),
            "{}",
        )
        .await
        .expect("failed to create run");

        let run2_id = Uuid::new_v4();
        db.create_run(
            run2_id,
            session_id,
            "run2",
            "test.wdl",
            Some("task_b"),
            "{}",
        )
        .await
        .expect("failed to create run");

        let run3_id = Uuid::new_v4();
        db.create_run(
            run3_id,
            session_id,
            "run3",
            "test.wdl",
            Some("task_c"),
            "{}",
        )
        .await
        .expect("failed to create run");

        // Update one run to a different status
        db.update_run_status(run2_id, RunStatus::Running)
            .await
            .expect("failed to update run status");

        // Test without filtering
        let runs = db
            .list_runs(None, None, None)
            .await
            .expect("failed to list runs");
        assert_eq!(runs.len(), 3);

        // Test filtering by status
        let runs = db
            .list_runs(Some(RunStatus::Queued), None, None)
            .await
            .expect("failed to list runs");
        assert_eq!(runs.len(), 2);

        let runs = db
            .list_runs(Some(RunStatus::Running), None, None)
            .await
            .expect("failed to list runs");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].uuid, run2_id);

        // Test with limit
        let runs = db
            .list_runs(None, Some(2), None)
            .await
            .expect("failed to list runs");
        assert_eq!(runs.len(), 2);

        // Test with offset
        let runs = db
            .list_runs(None, Some(10), Some(1))
            .await
            .expect("failed to list runs");
        assert_eq!(runs.len(), 2);
    }

    #[sqlx::test]
    async fn create_task(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");

        let run_id = Uuid::new_v4();
        db.create_run(
            run_id,
            session_id,
            "test-run",
            "test.wdl",
            Some("test_task"),
            "{}",
        )
        .await
        .expect("failed to create run");

        let task = db
            .create_task("my_task", run_id, TaskStatus::Pending)
            .await
            .expect("failed to create task");

        assert_eq!(task.name, "my_task");
        assert_eq!(task.run_uuid, run_id);
        assert_eq!(task.status, TaskStatus::Pending);
    }

    #[sqlx::test]
    async fn create_task_is_idempotent(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");

        let run_id = Uuid::new_v4();
        db.create_run(run_id, session_id, "test-run", "test.wdl", Some("t"), "{}")
            .await
            .expect("failed to create run");

        db.create_task("t", run_id, TaskStatus::Initializing)
            .await
            .expect("failed to create task");
        assert!(
            db.update_task_started("t", Utc::now())
                .await
                .expect("failed to start task")
        );

        // Whichever event observes the task second must not resurrect it: the
        // engine and Crankshaft channels have no ordering between them.
        let task = db
            .create_task("t", run_id, TaskStatus::Pending)
            .await
            .expect("failed to create task");

        assert_eq!(task.status, TaskStatus::Running);
    }

    #[sqlx::test]
    async fn task_status_only_advances(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");

        let run_id = Uuid::new_v4();
        db.create_run(run_id, session_id, "test-run", "test.wdl", Some("t"), "{}")
            .await
            .expect("failed to create run");

        db.create_task("t", run_id, TaskStatus::Initializing)
            .await
            .expect("failed to create task");

        assert!(
            db.update_task_localizing("t")
                .await
                .expect("failed to update task")
        );
        assert!(
            db.update_task_pending("t")
                .await
                .expect("failed to update task")
        );

        // A late localizing event must not drag the task backwards.
        assert!(
            !db.update_task_localizing("t")
                .await
                .expect("failed to update task")
        );
        assert_eq!(
            db.get_task("t").await.expect("failed to get task").status,
            TaskStatus::Pending
        );

        assert!(
            db.update_task_started("t", Utc::now())
                .await
                .expect("failed to update task")
        );
        assert!(
            !db.update_task_pending("t")
                .await
                .expect("failed to update task")
        );

        // The first terminal status wins; reconciliation of a finished task is a
        // no-op.
        assert!(
            db.update_task_completed("t", Some(0), Utc::now())
                .await
                .expect("failed to update task")
        );
        assert!(
            !db.update_task_canceled("t", Utc::now())
                .await
                .expect("failed to update task")
        );

        let task = db.get_task("t").await.expect("failed to get task");
        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(task.exit_status, Some(0));
    }

    #[sqlx::test]
    async fn update_task_cached_is_terminal(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");

        let run_id = Uuid::new_v4();
        db.create_run(run_id, session_id, "test-run", "test.wdl", Some("t"), "{}")
            .await
            .expect("failed to create run");

        db.create_task("t", run_id, TaskStatus::Initializing)
            .await
            .expect("failed to create task");

        let completed_at = Utc::now();
        assert!(
            db.update_task_cached("t", completed_at)
                .await
                .expect("failed to update task")
        );

        let task = db.get_task("t").await.expect("failed to get task");
        assert_eq!(task.status, TaskStatus::Cached);
        assert!(task.completed_at.is_some());

        assert!(
            !db.update_task_started("t", Utc::now())
                .await
                .expect("failed to update task")
        );
        assert_eq!(
            db.get_task("t").await.expect("failed to get task").status,
            TaskStatus::Cached
        );
    }

    #[sqlx::test]
    async fn get_task(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");

        let run_id = Uuid::new_v4();
        db.create_run(
            run_id,
            session_id,
            "test-run",
            "test.wdl",
            Some("test_task"),
            "{}",
        )
        .await
        .expect("failed to create run");

        let created_task = db
            .create_task("my_task", run_id, TaskStatus::Pending)
            .await
            .expect("failed to create task");

        let retrieved_task = db.get_task("my_task").await.expect("failed to get task");

        assert_eq!(retrieved_task.name, created_task.name);
        assert_eq!(retrieved_task.run_uuid, created_task.run_uuid);
    }

    #[sqlx::test]
    async fn list_tasks(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");

        let run1_id = Uuid::new_v4();
        db.create_run(
            run1_id,
            session_id,
            "test-run1",
            "test.wdl",
            Some("task_a"),
            "{}",
        )
        .await
        .expect("failed to create run");

        let run2_id = Uuid::new_v4();
        db.create_run(
            run2_id,
            session_id,
            "test-run2",
            "test.wdl",
            Some("task_b"),
            "{}",
        )
        .await
        .expect("failed to create run");

        db.create_task("task1", run1_id, TaskStatus::Pending)
            .await
            .expect("failed to create task");

        db.create_task("task2", run1_id, TaskStatus::Pending)
            .await
            .expect("failed to create task");

        db.create_task("task3", run2_id, TaskStatus::Pending)
            .await
            .expect("failed to create task");

        // Update one task to running status
        db.update_task_started("task2", Utc::now())
            .await
            .expect("failed to update task");

        // Test without filtering
        let tasks = db
            .list_tasks(None, None, None, None)
            .await
            .expect("failed to list tasks");
        assert_eq!(tasks.len(), 3);

        // Test filtering by run_id
        let tasks = db
            .list_tasks(Some(run1_id), None, None, None)
            .await
            .expect("failed to list tasks");
        assert_eq!(tasks.len(), 2);

        let tasks = db
            .list_tasks(Some(run2_id), None, None, None)
            .await
            .expect("failed to list tasks");
        assert_eq!(tasks.len(), 1);

        // Test filtering by status
        let tasks = db
            .list_tasks(None, Some(TaskStatus::Pending), None, None)
            .await
            .expect("failed to list tasks");
        assert_eq!(tasks.len(), 2);

        let tasks = db
            .list_tasks(None, Some(TaskStatus::Running), None, None)
            .await
            .expect("failed to list tasks");
        assert_eq!(tasks.len(), 1);

        // Test filtering by both run_id and status
        let tasks = db
            .list_tasks(Some(run1_id), Some(TaskStatus::Pending), None, None)
            .await
            .expect("failed to list tasks");
        assert_eq!(tasks.len(), 1);

        // Test with limit
        let tasks = db
            .list_tasks(None, None, Some(2), None)
            .await
            .expect("failed to list tasks");
        assert_eq!(tasks.len(), 2);

        // Test with offset
        let tasks = db
            .list_tasks(None, None, Some(10), Some(1))
            .await
            .expect("failed to list tasks");
        assert_eq!(tasks.len(), 2);
    }

    #[sqlx::test]
    async fn count_tasks_by_status(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");

        let run_id = Uuid::new_v4();
        db.create_run(run_id, session_id, "test-run", "test.wdl", Some("wf"), "{}")
            .await
            .expect("failed to create run");

        let other_run_id = Uuid::new_v4();
        db.create_run(
            other_run_id,
            session_id,
            "other-run",
            "test.wdl",
            Some("wf"),
            "{}",
        )
        .await
        .expect("failed to create run");

        // run_id: two pending, one running, one completed.
        for name in ["t1", "t2", "t3", "t4"] {
            db.create_task(name, run_id, TaskStatus::Pending)
                .await
                .expect("failed to create task");
        }
        db.update_task_started("t3", Utc::now())
            .await
            .expect("failed to update task");
        db.update_task_completed("t4", Some(0), Utc::now())
            .await
            .expect("failed to update task");

        // A task on a different run must not be counted.
        db.create_task("other", other_run_id, TaskStatus::Pending)
            .await
            .expect("failed to create task");

        let counts = db
            .count_tasks_by_status(run_id)
            .await
            .expect("failed to count tasks by status");
        let counts: std::collections::HashMap<TaskStatus, i64> = counts.into_iter().collect();

        assert_eq!(counts.get(&TaskStatus::Pending).copied(), Some(2));
        assert_eq!(counts.get(&TaskStatus::Running).copied(), Some(1));
        assert_eq!(counts.get(&TaskStatus::Completed).copied(), Some(1));
        // Statuses with no tasks are absent.
        assert_eq!(counts.get(&TaskStatus::Failed), None);
        // The total across all statuses for this run is four.
        assert_eq!(counts.values().sum::<i64>(), 4);

        // An unknown run yields no rows.
        let empty = db
            .count_tasks_by_status(Uuid::new_v4())
            .await
            .expect("failed to count tasks by status");
        assert!(empty.is_empty());
    }

    #[sqlx::test]
    async fn insert_task_log(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");

        let run_id = Uuid::new_v4();
        db.create_run(
            run_id,
            session_id,
            "test-run",
            "test.wdl",
            Some("test_task"),
            "{}",
        )
        .await
        .expect("failed to create run");

        db.create_task("my_task", run_id, TaskStatus::Pending)
            .await
            .expect("failed to create task");

        db.insert_task_log("my_task", LogSource::Stdout, b"hello")
            .await
            .expect("failed to insert task log");

        let logs = db
            .get_task_logs("my_task", None, None, None)
            .await
            .expect("failed to get task logs");

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].task_name, "my_task");
        assert_eq!(logs[0].source, LogSource::Stdout);
        assert_eq!(&*logs[0].chunk, b"hello");
    }

    #[sqlx::test]
    async fn get_task_logs(pool: SqlitePool) {
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let session_id = Uuid::new_v4();
        db.create_session(session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");

        let run_id = Uuid::new_v4();
        db.create_run(
            run_id,
            session_id,
            "test-run",
            "test.wdl",
            Some("test_task"),
            "{}",
        )
        .await
        .expect("failed to create run");

        db.create_task("my_task", run_id, TaskStatus::Pending)
            .await
            .expect("failed to create task");

        db.insert_task_log("my_task", LogSource::Stdout, b"line1")
            .await
            .expect("failed to insert task log");

        db.insert_task_log("my_task", LogSource::Stderr, b"line2")
            .await
            .expect("failed to insert task log");

        db.insert_task_log("my_task", LogSource::Stdout, b"line3")
            .await
            .expect("failed to insert task log");

        // Test without filtering
        let logs = db
            .get_task_logs("my_task", None, None, None)
            .await
            .expect("failed to get task logs");
        assert_eq!(logs.len(), 3);

        // Test filtering by source
        let logs = db
            .get_task_logs("my_task", Some(LogSource::Stdout), None, None)
            .await
            .expect("failed to get task logs");
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].source, LogSource::Stdout);
        assert_eq!(logs[1].source, LogSource::Stdout);

        let logs = db
            .get_task_logs("my_task", Some(LogSource::Stderr), None, None)
            .await
            .expect("failed to get task logs");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].source, LogSource::Stderr);

        // Test with limit
        let logs = db
            .get_task_logs("my_task", None, Some(2), None)
            .await
            .expect("failed to get task logs");
        assert_eq!(logs.len(), 2);

        // Test with offset
        let logs = db
            .get_task_logs("my_task", None, Some(10), Some(1))
            .await
            .expect("failed to get task logs");
        assert_eq!(logs.len(), 2);
    }

    #[sqlx::test]
    async fn mark_orphaned_runs(pool: SqlitePool) {
        /// How long a session may go unheartbeated before it is stale.
        const TIMEOUT: Duration = Duration::from_secs(300);
        /// The error recorded on swept runs.
        const ERROR: &str = "the server that owned this run stopped reporting";

        let raw = pool.clone();
        let db = SqliteDatabase::from_pool(pool)
            .await
            .expect("failed to create database");

        let now = Utc::now();

        // A server that died: its session stopped being heartbeated well
        // beyond the timeout.
        let stale_session = Uuid::new_v4();
        db.create_session(stale_session, SprocketCommand::Server, "test-user")
            .await
            .expect("failed to create session");
        db.heartbeat_session(stale_session, now - chrono::Duration::minutes(10))
            .await
            .expect("failed to heartbeat session");

        // More orphaned runs than a single page (`DEFAULT_PAGE_SIZE`), to
        // prove the sweep does not truncate on a large backlog.
        let orphaned_running_count = usize::try_from(DEFAULT_PAGE_SIZE).unwrap() + 50;
        let mut orphaned_running = Vec::with_capacity(orphaned_running_count);
        for i in 0..orphaned_running_count {
            let run_id = Uuid::new_v4();
            db.create_run(
                run_id,
                stale_session,
                &format!("orphaned-running-{i}"),
                "test.wdl",
                Some("test_task"),
                "{}",
            )
            .await
            .expect("failed to create run");
            db.update_run_status(run_id, RunStatus::Running)
                .await
                .expect("failed to update run status");

            // A task still `running`, as if its executor died mid-task.
            let task_name = format!("orphaned-task-{i}");
            db.create_task(&task_name, run_id, TaskStatus::Pending)
                .await
                .expect("failed to create task");
            db.update_task_started(&task_name, now)
                .await
                .expect("failed to update task");

            orphaned_running.push(run_id);
        }

        // One orphaned `Queued` run and one orphaned `Canceling` run, whose
        // lazy cancellation was never finalized.
        let queued_id = Uuid::new_v4();
        db.create_run(
            queued_id,
            stale_session,
            "orphaned-queued",
            "test.wdl",
            Some("test_task"),
            "{}",
        )
        .await
        .expect("failed to create run");

        let canceling_id = Uuid::new_v4();
        db.create_run(
            canceling_id,
            stale_session,
            "orphaned-canceling",
            "test.wdl",
            Some("test_task"),
            "{}",
        )
        .await
        .expect("failed to create run");
        db.update_run_status(canceling_id, RunStatus::Canceling)
            .await
            .expect("failed to update run status");

        // A run that legitimately completed should be left untouched.
        let completed_id = Uuid::new_v4();
        db.create_run(
            completed_id,
            stale_session,
            "completed-run",
            "test.wdl",
            Some("test_task"),
            "{}",
        )
        .await
        .expect("failed to create run");
        db.complete_run(completed_id, now)
            .await
            .expect("failed to complete run");

        // A concurrently live server sharing this database. Its in-flight work
        // must survive another server's sweep, or one server would fail
        // another's healthy runs out from under it.
        let live_session = Uuid::new_v4();
        db.create_session(live_session, SprocketCommand::Server, "test-user")
            .await
            .expect("failed to create session");
        db.heartbeat_session(live_session, now)
            .await
            .expect("failed to heartbeat session");
        let live_run_id = Uuid::new_v4();
        db.create_run(
            live_run_id,
            live_session,
            "live-server-run",
            "test.wdl",
            Some("test_task"),
            "{}",
        )
        .await
        .expect("failed to create run");
        db.update_run_status(live_run_id, RunStatus::Running)
            .await
            .expect("failed to update run status");
        let live_task_name = "live-server-task";
        db.create_task(live_task_name, live_run_id, TaskStatus::Pending)
            .await
            .expect("failed to create task");
        db.update_task_started(live_task_name, now)
            .await
            .expect("failed to update task");

        // A session predating heartbeat support: `heartbeat_at` is null, so
        // staleness falls back to `created_at`. Backdated through raw SQL to
        // carry SQLite's `current_timestamp` format rather than sqlx's.
        let legacy_session = Uuid::new_v4();
        db.create_session(legacy_session, SprocketCommand::Server, "test-user")
            .await
            .expect("failed to create session");
        sqlx::query("update sessions set created_at = '2020-01-01 00:00:00' where uuid = ?")
            .bind(legacy_session.to_string())
            .execute(&raw)
            .await
            .expect("failed to backdate session");
        let legacy_run_id = Uuid::new_v4();
        db.create_run(
            legacy_run_id,
            legacy_session,
            "legacy-run",
            "test.wdl",
            Some("test_task"),
            "{}",
        )
        .await
        .expect("failed to create run");
        db.update_run_status(legacy_run_id, RunStatus::Running)
            .await
            .expect("failed to update run status");

        // A server session created moments ago that has not heartbeated yet.
        // Its `created_at` is in SQLite's text format while the cutoff is
        // RFC 3339; compared as text it would look stale immediately.
        let fresh_session = Uuid::new_v4();
        db.create_session(fresh_session, SprocketCommand::Server, "test-user")
            .await
            .expect("failed to create session");
        let fresh_run_id = Uuid::new_v4();
        db.create_run(
            fresh_run_id,
            fresh_session,
            "fresh-run",
            "test.wdl",
            Some("test_task"),
            "{}",
        )
        .await
        .expect("failed to create run");
        db.update_run_status(fresh_run_id, RunStatus::Running)
            .await
            .expect("failed to update run status");
        let fresh_task_name = "fresh-task";
        db.create_task(fresh_task_name, fresh_run_id, TaskStatus::Pending)
            .await
            .expect("failed to create task");
        db.update_task_started(fresh_task_name, now)
            .await
            .expect("failed to update task");

        // A live `sprocket run` heartbeats the session it owns just as a
        // server does, and must survive a sweep while still executing.
        let cli_session_id = Uuid::new_v4();
        db.create_session(cli_session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");
        db.heartbeat_session(cli_session_id, now)
            .await
            .expect("failed to heartbeat session");
        let cli_run_id = Uuid::new_v4();
        db.create_run(
            cli_run_id,
            cli_session_id,
            "cli-run-in-progress",
            "test.wdl",
            Some("test_task"),
            "{}",
        )
        .await
        .expect("failed to create run");
        db.update_run_status(cli_run_id, RunStatus::Running)
            .await
            .expect("failed to update run status");
        let cli_task_name = "cli-task-in-progress";
        db.create_task(cli_task_name, cli_run_id, TaskStatus::Pending)
            .await
            .expect("failed to create task");
        db.update_task_started(cli_task_name, now)
            .await
            .expect("failed to update task");

        // A `sprocket run` killed outright stops heartbeating like any other
        // owner, and is swept on the same terms.
        let dead_cli_session_id = Uuid::new_v4();
        db.create_session(dead_cli_session_id, SprocketCommand::Run, "test-user")
            .await
            .expect("failed to create session");
        db.heartbeat_session(dead_cli_session_id, now - chrono::Duration::minutes(10))
            .await
            .expect("failed to heartbeat session");
        let dead_cli_run_id = Uuid::new_v4();
        db.create_run(
            dead_cli_run_id,
            dead_cli_session_id,
            "cli-run-killed",
            "test.wdl",
            Some("test_task"),
            "{}",
        )
        .await
        .expect("failed to create run");
        db.update_run_status(dead_cli_run_id, RunStatus::Running)
            .await
            .expect("failed to update run status");
        let dead_cli_task_name = "cli-task-killed";
        db.create_task(dead_cli_task_name, dead_cli_run_id, TaskStatus::Pending)
            .await
            .expect("failed to create task");
        db.update_task_started(dead_cli_task_name, now)
            .await
            .expect("failed to update task");

        let count = db
            .mark_orphaned_runs(ERROR, TIMEOUT, now)
            .await
            .expect("failed to mark orphaned runs");
        // Everything under the stale session (running + queued + canceling),
        // the single legacy run, and the killed CLI run.
        assert_eq!(count, orphaned_running_count as u64 + 4);

        for run_id in &orphaned_running {
            let run = db
                .get_run(*run_id)
                .await
                .expect("failed to get run")
                .unwrap();
            assert_eq!(run.status, RunStatus::Orphaned);
            assert_eq!(run.error.as_deref(), Some(ERROR));
            assert!(run.completed_at.is_some());
        }

        let queued = db
            .get_run(queued_id)
            .await
            .expect("failed to get run")
            .unwrap();
        assert_eq!(queued.status, RunStatus::Orphaned);

        let canceling = db
            .get_run(canceling_id)
            .await
            .expect("failed to get run")
            .unwrap();
        assert_eq!(canceling.status, RunStatus::Orphaned);

        // Swept on `created_at` because it never recorded a heartbeat.
        let legacy = db
            .get_run(legacy_run_id)
            .await
            .expect("failed to get run")
            .unwrap();
        assert_eq!(legacy.status, RunStatus::Orphaned);

        // Untouched: already terminal before the sweep ran.
        let completed = db
            .get_run(completed_id)
            .await
            .expect("failed to get run")
            .unwrap();
        assert_eq!(completed.status, RunStatus::Completed);
        assert_eq!(completed.error, None);

        // Untouched: owned by a concurrently live server.
        let live_run = db
            .get_run(live_run_id)
            .await
            .expect("failed to get run")
            .unwrap();
        assert_eq!(live_run.status, RunStatus::Running);
        assert_eq!(live_run.error, None);

        // Untouched: created moments ago, heartbeat still pending.
        let fresh_run = db
            .get_run(fresh_run_id)
            .await
            .expect("failed to get run")
            .unwrap();
        assert_eq!(fresh_run.status, RunStatus::Running);
        assert_eq!(fresh_run.error, None);

        // Untouched: a `sprocket run` that is still reporting.
        let cli_run = db
            .get_run(cli_run_id)
            .await
            .expect("failed to get run")
            .unwrap();
        assert_eq!(cli_run.status, RunStatus::Running);
        assert_eq!(cli_run.error, None);

        // Swept: a `sprocket run` that stopped reporting. Nothing is left to
        // drive this run either, so it gets the same treatment as a server's.
        let dead_cli_run = db
            .get_run(dead_cli_run_id)
            .await
            .expect("failed to get run")
            .unwrap();
        assert_eq!(dead_cli_run.status, RunStatus::Orphaned);
        assert_eq!(dead_cli_run.error.as_deref(), Some(ERROR));

        // Tasks under swept runs were orphaned too; the live, fresh, and
        // still-reporting CLI tasks are all still `running`.
        let mut running: Vec<String> = db
            .list_tasks(None, Some(TaskStatus::Running), None, None)
            .await
            .expect("failed to list tasks")
            .into_iter()
            .map(|t| t.name)
            .collect();
        running.sort();
        assert_eq!(
            running,
            vec![
                cli_task_name.to_string(),
                fresh_task_name.to_string(),
                live_task_name.to_string(),
            ]
        );

        let orphaned_tasks = db
            .list_tasks(None, Some(TaskStatus::Orphaned), Some(1000), None)
            .await
            .expect("failed to list tasks");
        assert_eq!(orphaned_tasks.len(), orphaned_running_count + 1);

        // Running it again is a no-op: nothing left to sweep.
        let count = db
            .mark_orphaned_runs(ERROR, TIMEOUT, now)
            .await
            .expect("failed to mark orphaned runs");
        assert_eq!(count, 0);
    }
}
