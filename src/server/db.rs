//! Database operations.

use anyhow::Context;
use sqlx::Pool;
use sqlx::Sqlite;
use sqlx::SqlitePool;

pub mod models;

pub use models::*;

/// Database handle.
#[derive(Debug, Clone)]
pub struct Database {
    /// SQLite connection pool.
    pool: Pool<Sqlite>,
}

impl Database {
    /// Create a new database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if connection fails or migrations fail.
    pub async fn new(database_url: &str, _max_connections: u32) -> anyhow::Result<Self> {
        let pool = SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(database_url.trim_start_matches("sqlite://"))
                .create_if_missing(true),
        )
        .await
        .context("failed to connect to database")?;

        // Enable foreign key constraints (not enabled by default in SQLite).
        sqlx::query("pragma foreign_keys = on")
            .execute(&pool)
            .await
            .context("failed to enable foreign keys")?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("failed to run migrations")?;

        Ok(Self { pool })
    }

    /// Get the underlying pool.
    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }
}

impl super::execution::Database for Database {
    fn update_workflow_running(
        &self,
        workflow_id: &str,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
        let workflow_id = workflow_id.to_string();
        let pool = self.pool.clone();
        async move {
            sqlx::query(
                "update workflows set status = ?, started_at = current_timestamp where id = ?",
            )
            .bind(WorkflowStatus::Running)
            .bind(&workflow_id)
            .execute(&pool)
            .await
            .context("failed to update workflow status to running")?;
            Ok(())
        }
    }

    fn update_workflow_completed(
        &self,
        workflow_id: &str,
        outputs: &serde_json::Value,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
        let workflow_id = workflow_id.to_string();
        let outputs = outputs.clone();
        let pool = self.pool.clone();
        async move {
            sqlx::query(
                "update workflows set status = ?, outputs = ?, completed_at = current_timestamp where id = ?",
            )
            .bind(WorkflowStatus::Completed)
            .bind(&outputs)
            .bind(&workflow_id)
            .execute(&pool)
            .await
            .context("failed to update workflow to completed")?;
            Ok(())
        }
    }

    fn update_workflow_failed(
        &self,
        workflow_id: &str,
        error: &str,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
        let workflow_id = workflow_id.to_string();
        let error = error.to_string();
        let pool = self.pool.clone();
        async move {
            sqlx::query(
                "update workflows set status = ?, error = ?, completed_at = current_timestamp where id = ?",
            )
            .bind(WorkflowStatus::Failed)
            .bind(&error)
            .bind(&workflow_id)
            .execute(&pool)
            .await
            .context("failed to update workflow status to failed")?;
            Ok(())
        }
    }
}
