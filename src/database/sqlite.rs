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
use super::Result;
use super::models::IndexLogEntry;
use super::models::Invocation;
use super::models::InvocationMethod;
use super::models::Workflow;
use super::models::WorkflowStatus;

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
#[derive(Debug)]
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
        created_by: Option<String>,
    ) -> Result<Invocation> {
        sqlx::query("insert into invocations (id, method, created_by) values (?, ?, ?)")
            .bind(id.to_string())
            .bind(method.to_string())
            .bind(&created_by)
            .execute(&self.pool)
            .await?;

        let invocation: Invocation = sqlx::query_as(
            "select id, method, created_by, created_at from invocations where id = ?",
        )
        .bind(id.to_string())
        .fetch_one(&self.pool)
        .await?;

        Ok(invocation)
    }

    async fn get_invocation(&self, id: Uuid) -> Result<Option<Invocation>> {
        let invocation: Option<Invocation> = sqlx::query_as(
            "select id, method, created_by, created_at from invocations where id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        Ok(invocation)
    }

    async fn create_workflow(
        &self,
        id: Uuid,
        invocation_id: Uuid,
        name: String,
        source: String,
        inputs: String,
        execution_dir: String,
    ) -> Result<Workflow> {
        sqlx::query(
            "insert into workflows (id, invocation_id, name, source, status, inputs, \
             execution_dir) values (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(invocation_id.to_string())
        .bind(&name)
        .bind(&source)
        .bind("pending")
        .bind(&inputs)
        .bind(&execution_dir)
        .execute(&self.pool)
        .await?;

        let workflow: Workflow = sqlx::query_as(
            "select id, invocation_id, name, source, status, inputs, outputs, error, \
             execution_dir, started_at, completed_at, created_at from workflows where id = ?",
        )
        .bind(id.to_string())
        .fetch_one(&self.pool)
        .await?;

        Ok(workflow)
    }

    async fn update_workflow_status(
        &self,
        id: Uuid,
        status: WorkflowStatus,
        started_at: Option<DateTime<Utc>>,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        sqlx::query(
            "update workflows set status = ?, started_at = ?, completed_at = ? where id = ?",
        )
        .bind(status.to_string())
        .bind(started_at)
        .bind(completed_at)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update_workflow_outputs(&self, id: Uuid, outputs: String) -> Result<()> {
        sqlx::query("update workflows set outputs = ? where id = ?")
            .bind(&outputs)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_workflow_error(&self, id: Uuid, error: String) -> Result<()> {
        sqlx::query("update workflows set error = ? where id = ?")
            .bind(&error)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn get_workflow(&self, id: Uuid) -> Result<Option<Workflow>> {
        let workflow: Option<Workflow> = sqlx::query_as(
            "select id, invocation_id, name, source, status, inputs, outputs, error, \
             execution_dir, started_at, completed_at, created_at from workflows where id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        Ok(workflow)
    }

    async fn list_workflows_by_invocation(&self, invocation_id: Uuid) -> Result<Vec<Workflow>> {
        let workflows: Vec<Workflow> = sqlx::query_as(
            "select id, invocation_id, name, source, status, inputs, outputs, error, \
             execution_dir, started_at, completed_at, created_at from workflows where \
             invocation_id = ? order by created_at",
        )
        .bind(invocation_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        Ok(workflows)
    }

    async fn create_index_log_entry(
        &self,
        workflow_id: Uuid,
        index_path: String,
        target_path: String,
    ) -> Result<IndexLogEntry> {
        let result = sqlx::query(
            "insert into index_log (workflow_id, index_path, target_path) values (?, ?, ?)",
        )
        .bind(workflow_id.to_string())
        .bind(&index_path)
        .bind(&target_path)
        .execute(&self.pool)
        .await?;

        let id = result.last_insert_rowid();

        let entry: IndexLogEntry = sqlx::query_as(
            "select id, workflow_id, index_path, target_path, created_at from index_log where id \
             = ?",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        Ok(entry)
    }

    async fn list_index_log_entries_by_workflow(
        &self,
        workflow_id: Uuid,
    ) -> Result<Vec<IndexLogEntry>> {
        let entries: Vec<IndexLogEntry> = sqlx::query_as(
            "select id, workflow_id, index_path, target_path, created_at from index_log where \
             workflow_id = ? order by created_at",
        )
        .bind(workflow_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        Ok(entries)
    }

    async fn list_latest_index_entries(&self) -> Result<Vec<IndexLogEntry>> {
        let entries: Vec<IndexLogEntry> = sqlx::query_as(
            "select id, workflow_id, index_path, target_path, created_at from latest_index_entries",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(entries)
    }
}
