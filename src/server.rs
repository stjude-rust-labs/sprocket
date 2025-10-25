//! REST API server for executing WDL workflows.
//!
//! Sprocket Server provides a high-performance API for submitting and managing
//! concurrent WDL workflow executions with persistent storage.

mod api;
mod config;
mod db;
mod execution;
mod manager;
mod names;
mod router;

pub use config::Config;
pub use router::run;
