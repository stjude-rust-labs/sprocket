//! Input processing functionality for Sprocket.

pub mod command_line;
pub mod yaml;

// Re-export the main functionality
pub use command_line::{CommandLineInput, InputValue, apply_inputs};
pub use yaml::*;
