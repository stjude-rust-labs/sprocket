//! API models and handlers.

use tokio::sync::mpsc;

use super::manager::ManagerCommand;

pub mod error;
pub mod models;
pub mod workflows;

/// Application state.
#[derive(Clone)]
pub struct AppState {
    /// Manager command sender.
    pub manager: mpsc::Sender<ManagerCommand>,
}
