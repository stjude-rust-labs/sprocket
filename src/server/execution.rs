//! Workflow execution trait and types.

use anyhow::Result;

/// Trait for database persistence during workflow execution.
///
/// This trait abstracts the database operations needed during workflow
/// execution, allowing the executor to work with different database backends.
pub trait Database: Send + Sync {
    /// Update workflow status to running.
    fn update_workflow_running(
        &self,
        workflow_id: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Update workflow status to completed with outputs.
    fn update_workflow_completed(
        &self,
        workflow_id: &str,
        outputs: &serde_json::Value,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Update workflow status to failed with error message.
    fn update_workflow_failed(
        &self,
        workflow_id: &str,
        error: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
}
