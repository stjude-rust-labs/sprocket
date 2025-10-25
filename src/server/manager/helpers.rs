//! Helper functions for workflow execution.

use anyhow::Context;
use anyhow::Result;

use crate::server::db::Database;
use crate::server::db::WorkflowStatus;

/// Update workflow status to running.
pub async fn update_workflow_running(workflow_id: &str, db: &Database) -> Result<()> {
    sqlx::query("update workflows set status = ?, started_at = current_timestamp where id = ?")
        .bind(WorkflowStatus::Running)
        .bind(workflow_id)
        .execute(db.pool())
        .await
        .context("failed to update workflow status to running")?;

    Ok(())
}

/// Update workflow status to completed with outputs.
pub async fn update_workflow_completed(
    workflow_id: &str,
    outputs: &serde_json::Value,
    db: &Database,
) -> Result<()> {
    sqlx::query(
        "update workflows set status = ?, outputs = ?, completed_at = current_timestamp where id = ?",
    )
    .bind(WorkflowStatus::Completed)
    .bind(outputs)
    .bind(workflow_id)
    .execute(db.pool())
    .await
    .context("failed to update workflow to completed")?;

    Ok(())
}

/// Update workflow status to failed with error message.
pub async fn update_workflow_failed(workflow_id: &str, error: &str, db: &Database) -> Result<()> {
    sqlx::query(
        "update workflows set status = ?, error = ?, completed_at = current_timestamp where id = ?",
    )
    .bind(WorkflowStatus::Failed)
    .bind(error)
    .bind(workflow_id)
    .execute(db.pool())
    .await
    .context("failed to update workflow status to failed")?;

    Ok(())
}
