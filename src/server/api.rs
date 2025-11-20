//! API models and handlers.

use axum::Router;
use tokio::sync::mpsc;

use crate::execution::ManagerCommand;

pub mod v1;

/// Application state.
#[derive(Clone, Debug)]
pub struct AppState {
    /// Manager command sender.
    pub manager: mpsc::Sender<ManagerCommand>,
}

/// Create the API router with all versions.
pub fn create_router(state: AppState) -> Router {
    Router::new().nest("/v1", v1::create_router(state))
}
