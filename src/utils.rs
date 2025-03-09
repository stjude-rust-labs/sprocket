//! Utility functions for the Sprocket command line tool.

use std::path::PathBuf;
use std::fs;

use anyhow::{Result, Context, bail};
use tracing::debug;

/// Represents the format of an input file.
#[derive(Debug, PartialEq)]
pub enum InputFormat {
    /// JSON format
    Json,
    /// YAML format
    Yaml,
}

/// Determines the format of an input file by attempting to parse it.
pub fn detect_input_format(file_path: &PathBuf) -> Result<InputFormat> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;
    
    // Try parsing as JSON first
    if let Ok(_) = serde_json::from_str::<serde_json::Value>(&content) {
        debug!("File detected as JSON: {}", file_path.display());
        return Ok(InputFormat::Json);
    }

    // Then try parsing as YAML
    if let Ok(_) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
        debug!("File detected as YAML: {}", file_path.display());
        return Ok(InputFormat::Yaml);
    }

    // If neither worked, return an error
    bail!("File is neither valid JSON nor valid YAML: {}", file_path.display())
}

/// Converts input file to JSON if necessary.
///
/// If the file is already JSON, returns the original path.
/// If the file is YAML, converts it to JSON and returns the path to the temporary JSON file.
pub fn get_json_path(input_path: &PathBuf) -> Result<PathBuf> {
    match detect_input_format(input_path)? {
        InputFormat::Json => {
            debug!("Input file is already JSON, using as-is");
            Ok(input_path.clone())
        },
        InputFormat::Yaml => {
            debug!("Converting YAML input to JSON");
            convert_yaml_to_json(input_path)
        }
    }
}

/// Converts a YAML file to a temporary JSON file.
///
/// Uses serde to convert from YAML to JSON.
fn convert_yaml_to_json(yaml_path: &PathBuf) -> Result<PathBuf> {
    let content = fs::read_to_string(yaml_path)
        .with_context(|| format!("Failed to read YAML file: {}", yaml_path.display()))?;
    
    // Parse YAML to a serde_yaml::Value
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(&content)
        .context("Failed to parse YAML content")?;
    
    // Convert to JSON using serde's built-in conversion
    let json_value = serde_json::to_value(&yaml_value)
        .context("Failed to convert YAML value to JSON value")?;
    
    let json_string = serde_json::to_string_pretty(&json_value)
        .context("Failed to convert to JSON string")?;
    
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
    fn test_detect_json_format() {
        // Create a temporary JSON file
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"{\"key\": \"value\", \"number\": 42}").unwrap();
        
        // Test detection
        let format = detect_input_format(&file.path().to_path_buf()).unwrap();
        assert_eq!(format, InputFormat::Json);
    }

    #[test]
    fn test_detect_yaml_format() {
        // Create a temporary YAML file
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"key: value\nnumber: 42").unwrap();
        
        // Test detection
        let format = detect_input_format(&file.path().to_path_buf()).unwrap();
        assert_eq!(format, InputFormat::Yaml);
    }

    #[test]
    fn test_invalid_format_detection() {
        // Create a temporary file with invalid content
        // This content is invalid in both JSON and YAML:
        // - For JSON, it's missing quotes and braces
        // - For YAML, it has invalid indentation and structure
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"key: value\n  - invalid indent\n    ]broken: [structure").unwrap();
        
        // Test detection should fail
        let result = detect_input_format(&file.path().to_path_buf());
        assert!(result.is_err());
    }

    #[test]
    fn test_get_json_path_with_json() {
        // Create a temporary JSON file
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"{\"key\": \"value\", \"number\": 42}").unwrap();
        let path = file.path().to_path_buf();
        
        // Test conversion with JSON (should return same path)
        let json_path = get_json_path(&path).unwrap();
        assert_eq!(json_path, path);
    }

    #[test]
    fn test_yaml_to_json_conversion() {
        // Create a temporary YAML file
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"key: value\nnested:\n  inner: 42\nlist:\n  - item1\n  - item2").unwrap();
        
        // Convert to JSON
        let json_path = get_json_path(&file.path().to_path_buf()).unwrap();
        
        // Verify the JSON content
        let json_content = fs::read_to_string(&json_path).unwrap();
        let json_value: serde_json::Value = serde_json::from_str(&json_content).unwrap();
        
        assert_eq!(json_value["key"], "value");
        assert_eq!(json_value["nested"]["inner"], 42);
        assert_eq!(json_value["list"][0], "item1");
        assert_eq!(json_value["list"][1], "item2");
    }
} 