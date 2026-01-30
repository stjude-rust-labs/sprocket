//! SQLite database implementation.

use std::path::Path;
use std::str::FromStr;

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

/// Configure 5-second timeout when database is locked to prevent spurious
/// failures.
const SQLITE_BUSY_TIMEOUT: &str = "5000";

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
            .journal_mode(SqliteJournalMode::Wal)
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

        sqlx::query("insert into sessions (uuid, subcommand, created_by) values (?, ?, ?)")
            .bind(id.to_string())
            .bind(subcommand)
            .bind(created_by)
            .execute(&self.pool)
            .await?;

        let session: Session = sqlx::query_as(
            "select uuid, subcommand, created_by, created_at from sessions where uuid = ?",
        )
        .bind(id.to_string())
        .fetch_one(&self.pool)
        .await?;

        Ok(session)
    }

    async fn get_session(&self, id: Uuid) -> Result<Option<Session>> {
        let session: Option<Session> = sqlx::query_as(
            "select uuid, subcommand, created_by, created_at from sessions where uuid = ?",
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
            "select uuid, subcommand, created_by, created_at from sessions order by created_at \
             desc limit ? offset ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(sessions)
    }

    async fn create_run(
        &self,
        id: Uuid,
        session_id: Uuid,
        name: &str,
        source: &str,
        target: &str,
        inputs: &str,
        directory: &str,
    ) -> Result<Run> {
        debug_assert!(!name.is_empty(), "`name` cannot be empty for a run");
        debug_assert!(!source.is_empty(), "`source` cannot be empty for a run");
        debug_assert!(!target.is_empty(), "`target` cannot be empty for a run");
        debug_assert!(
            !directory.is_empty(),
            "`directory` cannot be empty for a run"
        );

        sqlx::query(
            "insert into runs (uuid, session_id, name, source, target, status, inputs, directory) \
             select ?, s.id, ?, ?, ?, ?, ?, ? from sessions s where s.uuid = ?",
        )
        .bind(id.to_string())
        .bind(name)
        .bind(source)
        .bind(target)
        .bind(RunStatus::Queued)
        .bind(inputs)
        .bind(directory)
        .bind(session_id.to_string())
        .execute(&self.pool)
        .await?;

        let run: Run = sqlx::query_as(
            "select r.uuid, s.uuid as session_uuid, r.name, r.source, r.target, r.status, \
             r.inputs, r.outputs, r.error, r.directory, r.index_directory, r.started_at, \
             r.completed_at, r.created_at from runs r join sessions s on r.session_id = s.id \
             where r.uuid = ?",
        )
        .bind(id.to_string())
        .fetch_one(&self.pool)
        .await?;

        Ok(run)
    }

    async fn update_run_status(&self, id: Uuid, status: RunStatus) -> Result<()> {
        sqlx::query("update runs set status = ? where uuid = ?")
            .bind(status)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(())
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

    async fn create_index_log_entry(
        &self,
        run_id: Uuid,
        link_path: &str,
        target_path: &str,
    ) -> Result<IndexLogEntry> {
        let result = sqlx::query(
            "insert into index_log (run_id, link_path, target_path) select r.id, ?, ? from runs r \
             where r.uuid = ?",
        )
        .bind(link_path)
        .bind(target_path)
        .bind(run_id.to_string())
        .execute(&self.pool)
        .await?;

        let id = result.last_insert_rowid();

        let entry: IndexLogEntry = sqlx::query_as(
            "select i.id, r.uuid as run_uuid, i.link_path, i.target_path, i.created_at from \
             index_log i join runs r on i.run_id = r.id where i.id = ?",
        )
        .bind(id)
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

    async fn create_task(&self, name: &str, run_id: Uuid) -> Result<Task> {
        sqlx::query(
            "insert into tasks (name, run_id, status)
             select ?, r.id, ? from runs r where r.uuid = ?",
        )
        .bind(name)
        .bind(TaskStatus::Pending)
        .bind(run_id.to_string())
        .execute(&self.pool)
        .await?;

        self.get_task(name).await
    }

    async fn update_task_started(&self, name: &str, started_at: DateTime<Utc>) -> Result<bool> {
        let result = sqlx::query(
            "update tasks
             set status = ?, started_at = ?
             where name = ?",
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
             where name = ?",
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
             where name = ?",
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
             where name = ?",
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
             where name = ?",
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

        let mut q = sqlx::query_as(&query);

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

        let mut q = sqlx::query_scalar(&query);

        if let Some(run_id) = run_id {
            q = q.bind(run_id.to_string());
        }
        if let Some(status) = status {
            q = q.bind(status);
        }

        let count: i64 = q.fetch_one(&self.pool).await?;
        Ok(count)
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

        let mut q = sqlx::query_as(&query);
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

        let mut q = sqlx::query_scalar(&query);
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
                "test_task",
                "{}",
                "/tmp/run",
            )
            .await
            .expect("failed to create run");

        assert_eq!(run.uuid, run_id);
        assert_eq!(run.session_uuid, session_id);
        assert_eq!(run.name, "test-run");
        assert_eq!(run.source, "test.wdl");
        assert_eq!(run.target, "test_task");
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
            "test_task",
            "{}",
            "/tmp/run",
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
                "test_task",
                "{}",
                "/tmp/run",
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
            "task_a",
            "{}",
            "/tmp/run1",
        )
        .await
        .expect("failed to create run");

        let run2_id = Uuid::new_v4();
        db.create_run(
            run2_id,
            session_id,
            "run2",
            "test.wdl",
            "task_b",
            "{}",
            "/tmp/run2",
        )
        .await
        .expect("failed to create run");

        let run3_id = Uuid::new_v4();
        db.create_run(
            run3_id,
            session_id,
            "run3",
            "test.wdl",
            "task_c",
            "{}",
            "/tmp/run3",
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
            "test_task",
            "{}",
            "/tmp/run",
        )
        .await
        .expect("failed to create run");

        let task = db
            .create_task("my_task", run_id)
            .await
            .expect("failed to create task");

        assert_eq!(task.name, "my_task");
        assert_eq!(task.run_uuid, run_id);
        assert_eq!(task.status, TaskStatus::Pending);
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
            "test_task",
            "{}",
            "/tmp/run",
        )
        .await
        .expect("failed to create run");

        let created_task = db
            .create_task("my_task", run_id)
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
            "task_a",
            "{}",
            "/tmp/run1",
        )
        .await
        .expect("failed to create run");

        let run2_id = Uuid::new_v4();
        db.create_run(
            run2_id,
            session_id,
            "test-run2",
            "test.wdl",
            "task_b",
            "{}",
            "/tmp/run2",
        )
        .await
        .expect("failed to create run");

        db.create_task("task1", run1_id)
            .await
            .expect("failed to create task");

        db.create_task("task2", run1_id)
            .await
            .expect("failed to create task");

        db.create_task("task3", run2_id)
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
            "test_task",
            "{}",
            "/tmp/run",
        )
        .await
        .expect("failed to create run");

        db.create_task("my_task", run_id)
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
            "test_task",
            "{}",
            "/tmp/run",
        )
        .await
        .expect("failed to create run");

        db.create_task("my_task", run_id)
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
}
