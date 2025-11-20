//! Workflow manager actor.

pub mod actor;
pub mod commands;

pub use actor::spawn_manager;
pub use commands::ManagerCommand;
