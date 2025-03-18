//! Input processing functionality for Sprocket.

pub mod yaml;

// Re-export the main functionality from the yaml module
pub use yaml::InputFormat;
pub use yaml::get_json_file_path;
// Removed unused export: pub use yaml::get_json_string;
pub use yaml::parse_input_file;

