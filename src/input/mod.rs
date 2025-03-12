//! Input processing functionality for Sprocket.

pub mod yaml;
pub mod command_line;

// Re-export the main functionality
pub use yaml::*;
pub use command_line::{CommandLineInput, InputValue, apply_inputs};
