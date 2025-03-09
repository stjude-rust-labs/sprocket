//! Utility functions for the Sprocket command line tool.

use std::path::PathBuf;
use std::fs;

use anyhow::{Result, Context, bail};
use tracing::debug;

/// Parse result containing either JSON or YAML data
#[derive(Debug)]
pub enum ParsedInput {
    /// JSON data
    Json(serde_json::Value),
    /// YAML data converted to JSON
    Yaml(serde_json::Value),
}

/// Parses an input file as either JSON or YAML.
///
/// Tries to parse as JSON first, then YAML if JSON parsing fails.
/// Returns the parsed data as a JSON value.
pub fn parse_input_file(path: &PathBuf) -> Result<ParsedInput> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;
    
    // Try parsing as JSON first
    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&content) {
        debug!("File parsed as JSON: {}", path.display());
        return Ok(ParsedInput::Json(json_value));
    }

    // Then try parsing as YAML
    if let Ok(yaml_value) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
        debug!("File parsed as YAML: {}", path.display());
        // Convert YAML to JSON
        let json_value = serde_json::to_value(&yaml_value)
            .context("Failed to convert YAML value to JSON value")?;
        return Ok(ParsedInput::Yaml(json_value));
    }

    // If neither worked, return an error
    bail!("File is neither valid JSON nor valid YAML: {}", path.display())
}

/// Gets a JSON string representation of the input file.
///
/// Takes ownership of the input path and parses the file only once.
pub fn get_json_string(input_path: PathBuf) -> Result<String> {
    let parsed = parse_input_file(&input_path)?;
    
    match parsed {
        ParsedInput::Json(json) => {
            debug!("Using JSON input directly");
            serde_json::to_string_pretty(&json)
                .context("Failed to convert JSON value to string")
        },
        ParsedInput::Yaml(json) => {
            debug!("Using YAML input converted to JSON");
            serde_json::to_string_pretty(&json)
                .context("Failed to convert YAML-derived JSON value to string")
        }
    }
}

/// Creates a temporary JSON file from the input file contents.
///
/// This is used when the underlying API requires a file path rather than
/// content in memory.
pub fn create_json_file_from_input(input_path: PathBuf) -> Result<PathBuf> {
    let json_string = get_json_string(input_path)?;
    
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join("sprocket_temp_input.json");
    
    fs::write(&temp_file, json_string)
        .context("Failed to write temporary JSON file")?;
    
    Ok(temp_file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_json_file() {
        // Create a temporary JSON file
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"{\"key\": \"value\", \"number\": 42}").unwrap();
        
        // Test parsing
        let result = parse_input_file(&file.path().to_path_buf()).unwrap();
        match result {
            ParsedInput::Json(json) => {
                assert_eq!(json["key"], "value");
                assert_eq!(json["number"], 42);
            },
            ParsedInput::Yaml(_) => panic!("Should have been detected as JSON"),
        }
    }

    #[test]
    fn test_parse_yaml_file() {
        // Create a temporary YAML file
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"key: value\nnumber: 42").unwrap();
        
        // Test parsing
        let result = parse_input_file(&file.path().to_path_buf()).unwrap();
        match result {
            ParsedInput::Yaml(json) => {
                assert_eq!(json["key"], "value");
                assert_eq!(json["number"], 42);
            },
            ParsedInput::Json(_) => panic!("Should have been detected as YAML"),
        }
    }

    #[test]
    fn test_invalid_format_parsing() {
        // Create a temporary file with invalid content
        // This content is invalid in both JSON and YAML:
        // - For JSON, it's missing quotes and braces
        // - For YAML, it has invalid indentation and structure
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"key: value\n  - invalid indent\n    ]broken: [structure").unwrap();
        
        // Test parsing should fail
        let result = parse_input_file(&file.path().to_path_buf());
        assert!(result.is_err());
    }

    #[test]
    fn test_get_json_string_with_json() {
        // Create a temporary JSON file
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"{\"key\": \"value\", \"number\": 42}").unwrap();
        
        // Get JSON string
        let json_string = get_json_string(file.path().to_path_buf()).unwrap();
        
        // Parse back to verify
        let json: serde_json::Value = serde_json::from_str(&json_string).unwrap();
        assert_eq!(json["key"], "value");
        assert_eq!(json["number"], 42);
    }

    #[test]
    fn test_get_json_string_with_yaml() {
        // Create a temporary YAML file
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"key: value\nnested:\n  inner: 42\nlist:\n  - item1\n  - item2").unwrap();
        
        // Get JSON string
        let json_string = get_json_string(file.path().to_path_buf()).unwrap();
        
        // Parse back to verify
        let json: serde_json::Value = serde_json::from_str(&json_string).unwrap();
        assert_eq!(json["key"], "value");
        assert_eq!(json["nested"]["inner"], 42);
        assert_eq!(json["list"][0], "item1");
        assert_eq!(json["list"][1], "item2");
    }
} 