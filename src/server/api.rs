//! API models and handlers.

use axum::Router;
use bon::Builder;
use tokio::sync::mpsc;

use crate::server::api::v1::info::ServerFailureMode;
use crate::system::v1::exec::svc::run_manager::RunManagerCmd;

pub mod v1;

/// A sender for run manager commands.
type RunManagerTx = mpsc::Sender<RunManagerCmd>;

/// Application state.
#[derive(Builder, Clone, Debug)]
pub struct AppState {
    /// The run manager command transmitter.
    run_manager_tx: RunManagerTx,
    /// The cancellation failure mode the server is configured to use.
    ///
    /// Surfaced via the [`info`](crate::server::api::v1::info) endpoint so
    /// clients (e.g. the `dev server cancel` CLI) can adapt their behavior.
    failure_mode: ServerFailureMode,
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
