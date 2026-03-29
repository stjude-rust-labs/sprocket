//! API models and handlers.

use std::sync::Arc;

use axum::Router;
use bon::Builder;
use tokio::sync::mpsc;

use crate::system::v1::{db::Database, exec::svc::run_manager::RunManagerCmd};

pub mod v1;

/// A sender for run manager commands.
type RunManagerTx = mpsc::Sender<RunManagerCmd>;

#[derive(Clone)]
pub(crate) struct DbHandle(pub(crate) Arc<dyn Database>);

impl std::fmt::Debug for DbHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbHandle").finish()
    }
}

impl std::ops::Deref for DbHandle {
    type Target = Arc<dyn Database>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Application state.
#[derive(Builder, Clone, Debug)]
pub struct AppState {
    /// The run manager command transmitter.
    run_manager_tx: RunManagerTx,
    pub(crate) db: DbHandle,
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
