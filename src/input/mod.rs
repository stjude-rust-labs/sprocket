//! Input processing functionality for Sprocket.

pub mod yaml;
pub mod command_line;

// Re-export the main functionality from the yaml module
pub use yaml::InputFormat;
pub use yaml::get_json_file_path;
pub use yaml::get_json_string;
pub use yaml::parse_input_file;

// Re-export command line functionality
pub use command_line::{CommandLineInput, InputValue, apply_inputs};
