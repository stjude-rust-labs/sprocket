//! Common utilities for V1 API handlers.

use tokio::sync::mpsc;
use tokio::sync::oneshot;

use super::error::Error;
use crate::execution::ManagerCommand;
use crate::execution::ManagerResult;

/// Sends a command to the manager and receives the response.
///
/// This helper method is around to keep things DRY.
pub async fn send_command<T>(
    manager: &mpsc::Sender<ManagerCommand>,
    build_command: impl FnOnce(oneshot::Sender<ManagerResult<T>>) -> ManagerCommand,
) -> Result<T, Error> {
    let (tx, rx) = oneshot::channel();

    manager.send(build_command(tx)).await.map_err(|e| {
        tracing::error!("failed to send command to manager: {}", e);
        Error::Internal
    })?;

    match rx.await {
        Err(e) => {
            tracing::error!("manager dropped response channel: {}", e);
            Err(Error::Internal)
        }
        Ok(Err(manager_err)) => {
            tracing::warn!("manager rejected command: {}", manager_err);
            Err(Error::from(manager_err))
        }
        Ok(Ok(response)) => Ok(response),
    }
}
