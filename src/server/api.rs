//! API models and handlers.

use std::sync::Arc;

use axum::Router;
use bon::Builder;
use tokio::sync::mpsc;

use crate::system::v1::db::Database;
use crate::system::v1::exec::svc::run_manager::RunManagerCmd;

pub mod v1;

/// A sender for run manager commands.
type RunManagerTx = mpsc::Sender<RunManagerCmd>;

/// Application state.
#[derive(Builder, Clone)]
pub struct AppState {
    /// The run manager command transmitter.
    run_manager_tx: RunManagerTx,
    /// Database handle used for read-only API queries.
    pub(crate) db: Arc<dyn Database>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("run_manager_tx", &self.run_manager_tx)
            .field("db", &"<dyn Database>")
            .finish()
    }
}

impl AppState {
    /// Gets the run manager tx channel.
    pub fn run_manager_tx(&self) -> &RunManagerTx {
        &self.run_manager_tx
    }
}

/// Create the API router with all versions.
pub fn create_router(state: AppState) -> Router {
    Router::new().nest("/v1", v1::create_router(state))
}
