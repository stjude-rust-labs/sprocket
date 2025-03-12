//! Input processing functionality for Sprocket.

pub mod yaml;
pub mod override_mod;

// Re-export the main functionality from the yaml module
pub use yaml::InputFormat;
pub use yaml::get_json_file_path;
pub use yaml::get_json_string;
pub use yaml::parse_input_file;
pub use override_mod::InputOverride;
pub use override_mod::OverrideValue;
pub use override_mod::apply_overrides;
