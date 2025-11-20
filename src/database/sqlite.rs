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
use super::models::Invocation;
use super::models::InvocationMethod;
use super::models::Run;
use super::models::RunStatus;

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
        Ok(Self { pool })
    }

    /// Get the underlying connection pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl Database for SqliteDatabase {
    async fn create_invocation(
        &self,
        id: Uuid,
        method: InvocationMethod,
        created_by: &str,
    ) -> Result<Invocation> {
        if created_by.is_empty() {
            return Err(DatabaseError::Validation(String::from(
                "`created_by` cannot be empty for an invocation",
            )));
        }

        sqlx::query("insert into invocations (id, method, created_by) values (?, ?, ?)")
            .bind(id)
            .bind(method)
            .bind(created_by)
            .execute(&self.pool)
            .await?;

        let invocation: Invocation = sqlx::query_as(
            "select id, method, created_by, created_at from invocations where id = ?",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        Ok(invocation)
    }

    async fn get_invocation(&self, id: Uuid) -> Result<Option<Invocation>> {
        let invocation: Option<Invocation> = sqlx::query_as(
            "select id, method, created_by, created_at from invocations where id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(invocation)
    }

    async fn list_invocations(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Invocation>> {
        let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE);
        let offset = offset.unwrap_or(DEFAULT_OFFSET);

        let invocations: Vec<Invocation> = sqlx::query_as(
            "select id, method, created_by, created_at from invocations order by created_at desc \
             limit ? offset ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(invocations)
    }

    async fn create_run(
        &self,
        id: Uuid,
        invocation_id: Uuid,
        name: &str,
        source: &str,
        inputs: &str,
        directory: &str,
    ) -> Result<Run> {
        if name.is_empty() {
            return Err(DatabaseError::Validation(String::from(
                "`name` cannot be empty for a run",
            )));
        }
        if source.is_empty() {
            return Err(DatabaseError::Validation(String::from(
                "`source` cannot be empty for a run",
            )));
        }
        if directory.is_empty() {
            return Err(DatabaseError::Validation(String::from(
                "`directory` cannot be empty for a run",
            )));
        }

        sqlx::query(
            "insert into runs (id, invocation_id, name, source, status, inputs, directory) values \
             (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(invocation_id)
        .bind(name)
        .bind(source)
        .bind(RunStatus::Queued)
        .bind(inputs)
        .bind(directory)
        .execute(&self.pool)
        .await?;

        let run: Run = sqlx::query_as(
            "select id, invocation_id, name, source, status, inputs, outputs, error, directory, \
             index_directory, started_at, completed_at, created_at from runs where id = ?",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        Ok(run)
    }

    async fn update_run_status(
        &self,
        id: Uuid,
        status: RunStatus,
        started_at: Option<DateTime<Utc>>,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        sqlx::query("update runs set status = ?, started_at = ?, completed_at = ? where id = ?")
            .bind(status)
            .bind(started_at)
            .bind(completed_at)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_run_outputs(&self, id: Uuid, outputs: &str) -> Result<()> {
        sqlx::query("update runs set outputs = ? where id = ?")
            .bind(outputs)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_run_error(&self, id: Uuid, error: &str) -> Result<()> {
        sqlx::query("update runs set error = ? where id = ?")
            .bind(error)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_run_index_directory(&self, id: Uuid, index_directory: &str) -> Result<bool> {
        let result = sqlx::query("update runs set index_directory = ? where id = ?")
            .bind(index_directory)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_run(&self, id: Uuid) -> Result<Option<Run>> {
        let run: Option<Run> = sqlx::query_as(
            "select id, invocation_id, name, source, status, inputs, outputs, error, directory, \
             index_directory, started_at, completed_at, created_at from runs where id = ?",
        )
        .bind(id)
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
                "select id, invocation_id, name, source, status, inputs, outputs, error, \
                 directory, index_directory, started_at, completed_at, created_at from runs where \
                 status = ? order by created_at desc limit ? offset ?",
            )
            .bind(status)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as(
                "select id, invocation_id, name, source, status, inputs, outputs, error, \
                 directory, index_directory, started_at, completed_at, created_at from runs order \
                 by created_at desc limit ? offset ?",
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

    async fn list_runs_by_invocation(&self, invocation_id: Uuid) -> Result<Vec<Run>> {
        let runs: Vec<Run> = sqlx::query_as(
            "select id, invocation_id, name, source, status, inputs, outputs, error, directory, \
             index_directory, started_at, completed_at, created_at from runs where invocation_id \
             = ? order by created_at",
        )
        .bind(invocation_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(runs)
    }

    async fn create_index_log_entry(
        &self,
        run_id: Uuid,
        index_path: &str,
        target_path: &str,
    ) -> Result<IndexLogEntry> {
        let result =
            sqlx::query("insert into index_log (run_id, index_path, target_path) values (?, ?, ?)")
                .bind(run_id)
                .bind(index_path)
                .bind(target_path)
                .execute(&self.pool)
                .await?;

        let id = result.last_insert_rowid();

        let entry: IndexLogEntry = sqlx::query_as(
            "select id, run_id, index_path, target_path, created_at from index_log where id = ?",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        Ok(entry)
    }

    async fn list_index_log_entries_by_run(&self, run_id: Uuid) -> Result<Vec<IndexLogEntry>> {
        let entries: Vec<IndexLogEntry> = sqlx::query_as(
            "select id, run_id, index_path, target_path, created_at from index_log where run_id = \
             ? order by created_at",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(entries)
    }

    async fn list_latest_index_entries(&self) -> Result<Vec<IndexLogEntry>> {
        let entries: Vec<IndexLogEntry> = sqlx::query_as(
            "select id, run_id, index_path, target_path, created_at from latest_index_entries",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(entries)
    }
}
