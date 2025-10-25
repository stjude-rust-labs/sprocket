//! Workflow manager actor.

pub mod actor;
pub mod commands;
mod helpers;

pub use actor::spawn_manager;
pub use commands::ManagerCommand;
