//! Utility functions for the Sprocket command line tool.

use std::path::PathBuf;
use std::fs;
use std::io::Read;

use anyhow::{Result, Context};

/// Determines if a file is a YAML file based on its extension.
pub fn is_yaml_file(path: &PathBuf) -> bool {
    if let Some(extension) = path.extension() {
        let ext = extension.to_string_lossy().to_lowercase();
        return ext == "yaml" || ext == "yml";
    }
    false
}

/// Converts YAML content to JSON.
pub fn convert_yaml_to_json(yaml_path: &PathBuf) -> Result<String> {
    let mut file = fs::File::open(yaml_path)
        .with_context(|| format!("Failed to open YAML file: {}", yaml_path.display()))?;
    
    let mut content = String::new();
    file.read_to_string(&mut content)
        .with_context(|| format!("Failed to read YAML file: {}", yaml_path.display()))?;
    
    // Parse YAML to a serde_json::Value
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(&content)
        .context("Failed to parse YAML content")?;
    
    // Convert to JSON string
    let json_value = convert_yaml_value_to_json(yaml_value);
    let json_string = serde_json::to_string_pretty(&json_value)
        .context("Failed to convert to JSON string")?;
    
    Ok(json_string)
}

/// Recursively converts a serde_yaml::Value to a serde_json::Value.
pub fn convert_yaml_value_to_json(yaml_value: serde_yaml::Value) -> serde_json::Value {
    match yaml_value {
        serde_yaml::Value::Null => serde_json::Value::Null,
        serde_yaml::Value::Bool(b) => serde_json::Value::Bool(b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_json::Value::Number(serde_json::Number::from(i))
            } else if let Some(f) = n.as_f64() {
                serde_json::json!(f)
            } else {
                serde_json::Value::Null
            }
        },
        serde_yaml::Value::String(s) => serde_json::Value::String(s),
        serde_yaml::Value::Sequence(seq) => {
            let json_seq = seq.into_iter()
                .map(convert_yaml_value_to_json)
                .collect();
            serde_json::Value::Array(json_seq)
        },
        serde_yaml::Value::Mapping(map) => {
            let mut json_map = serde_json::Map::new();
            for (k, v) in map {
                if let serde_yaml::Value::String(key) = k {
                    json_map.insert(key, convert_yaml_value_to_json(v));
                }
            }
            serde_json::Value::Object(json_map)
        },
        _ => serde_json::Value::Null,
    }
}

/// Creates a temporary JSON file from a YAML file.
pub fn create_temp_json_from_yaml(yaml_path: &PathBuf) -> Result<PathBuf> {
    let json_content = convert_yaml_to_json(yaml_path)
        .context("Failed to convert YAML to JSON")?;
    
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join("sprocket_temp_input.json");
    
    fs::write(&temp_file, json_content)
        .context("Failed to write temporary JSON file")?;
    
    Ok(temp_file)
}

/// Gets the appropriate input path for a file, converting YAML to JSON if necessary.
pub fn get_input_path(input_path: &PathBuf) -> Result<PathBuf> {
    if is_yaml_file(input_path) {
        create_temp_json_from_yaml(input_path)
    } else {
        Ok(input_path.clone())
    }
} 