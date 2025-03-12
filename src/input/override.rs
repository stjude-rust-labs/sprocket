//! Implementation of command-line input overrides.

use anyhow::{Result, bail};
use serde_json::{Value, json};

/// Represents a parsed command line override
#[derive(Debug, Clone)]
pub struct InputOverride {
    /// The path components (e.g., ["workflow", "task", "param"])
    pub path: Vec<String>,
    /// The value to set at this path
    pub value: OverrideValue,
}

/// Represents different types of values that can be provided via CLI
#[derive(Debug, Clone)]
pub enum OverrideValue {
    /// A string value
    String(String),
    /// An integer value
    Integer(i64),
    /// A floating point value
    Float(f64),
    /// A boolean value
    Boolean(bool),
    /// An array of values
    Array(Vec<OverrideValue>),
    /// A null value
    Null,
}

impl InputOverride {
    /// Parse a key=value string into an InputOverride
    pub fn parse(input: &str) -> Result<Self> {
        let parts: Vec<&str> = input.splitn(2, '=').collect();
        if parts.len() != 2 {
            bail!("Invalid override format. Expected 'key=value', got '{}'", input);
        }

        let path = parse_path(parts[0])?;
        let value = parse_value(parts[1])?;

        Ok(Self { path, value })
    }
}

/// Parse dot notation path into components
fn parse_path(path: &str) -> Result<Vec<String>> {
    if path.is_empty() {
        bail!("Empty path in override");
    }
    Ok(path.split('.').map(String::from).collect())
}

/// Parse a value string into an OverrideValue
fn parse_value(input: &str) -> Result<OverrideValue> {
    if input.is_empty() {
        bail!("Missing value after '='");
    }

    // Handle quoted strings
    if input.starts_with('"') && input.ends_with('"') {
        return Ok(OverrideValue::String(input[1..input.len()-1].to_string()));
    }

    // Handle arrays with brackets
    if input.starts_with('[') {
        return parse_array_value(input);
    }

    // Handle flat arrays (comma-separated)
    if input.contains(',') {
        return parse_flat_array(input);
    }

    // Handle null
    if input == "null" {
        return Ok(OverrideValue::Null);
    }

    // Try parsing other primitive types
    if let Ok(i) = input.parse::<i64>() {
        return Ok(OverrideValue::Integer(i));
    }
    if let Ok(f) = input.parse::<f64>() {
        return Ok(OverrideValue::Float(f));
    }
    if input == "true" || input == "false" {
        return Ok(OverrideValue::Boolean(input == "true"));
    }

    // Default to string
    Ok(OverrideValue::String(input.to_string()))
}

/// Parse a flat comma-separated array
fn parse_flat_array(input: &str) -> Result<OverrideValue> {
    if input.ends_with(',') {
        bail!("Trailing comma not allowed");
    }

    let values = input
        .split(',')
        .map(|s| parse_value(s.trim()))
        .collect::<Result<Vec<_>>>()?;
    
    Ok(OverrideValue::Array(values))
}

/// Parse a nested array using brackets
fn parse_array_value(input: &str) -> Result<OverrideValue> {
    if !input.ends_with(']') {
        bail!("Unclosed bracket in array");
    }

    let inner = &input[1..input.len()-1];
    let mut values = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for c in inner.chars() {
        match c {
            '[' => {
                depth += 1;
                current.push(c);
            }
            ']' => {
                depth -= 1;
                current.push(c);
                if depth < 0 {
                    bail!("Unmatched closing bracket");
                }
            }
            ',' if depth == 0 => {
                if !current.is_empty() {
                    values.push(parse_value(current.trim())?);
                    current.clear();
                }
            }
            _ => current.push(c),
        }
    }

    if depth != 0 {
        bail!("Unmatched opening bracket");
    }

    if !current.is_empty() {
        values.push(parse_value(current.trim())?);
    }

    Ok(OverrideValue::Array(values))
}

impl OverrideValue {
    /// Convert the override value to a JSON value
    pub fn to_json(&self) -> Value {
        match self {
            OverrideValue::String(s) => json!(s),
            OverrideValue::Integer(i) => json!(i),
            OverrideValue::Float(f) => json!(f),
            OverrideValue::Boolean(b) => json!(b),
            OverrideValue::Array(arr) => {
                json!(arr.iter().map(|v| v.to_json()).collect::<Vec<_>>())
            }
            OverrideValue::Null => Value::Null,
        }
    }
}

/// Apply a list of overrides to a JSON value
pub fn apply_overrides(mut base: Value, overrides: &[InputOverride]) -> Result<Value> {
    for override_value in overrides {
        apply_single_override(&mut base, override_value)?;
    }
    Ok(base)
}

/// Apply a single override to a JSON value
fn apply_single_override(json: &mut Value, input_override: &InputOverride) -> Result<()> {
    let mut current = json;
    
    // Navigate to the correct location
    for (i, key) in input_override.path.iter().enumerate() {
        if i == input_override.path.len() - 1 {
            // Set the final value
            if let Some(obj) = current.as_object_mut() {
                obj.insert(key.clone(), input_override.value.to_json());
            } else {
                bail!("Cannot set value at '{}': parent is not an object", key);
            }
            break;
        }

        // Create/navigate intermediate objects
        if let Some(obj) = current.as_object_mut() {
            if !obj.contains_key(key) {
                obj.insert(key.clone(), json!({}));
            }
            current = obj.get_mut(key)
                .ok_or_else(|| anyhow::anyhow!("Failed to navigate to key: {}", key))?;
        } else {
            bail!("Cannot navigate to '{}': parent is not an object", key);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_path() {
        assert_eq!(
            parse_path("workflow.task.param").unwrap(),
            vec!["workflow", "task", "param"]
        );
        assert!(parse_path("").is_err());
    }

    #[test]
    fn test_parse_value() {
        assert!(matches!(parse_value("123").unwrap(), OverrideValue::Integer(123)));
        assert!(matches!(parse_value("3.14").unwrap(), OverrideValue::Float(3.14)));
        assert!(matches!(parse_value("true").unwrap(), OverrideValue::Boolean(true)));
        assert!(matches!(parse_value("null").unwrap(), OverrideValue::Null));
        assert!(matches!(
            parse_value("\"hello\"").unwrap(),
            OverrideValue::String(s) if s == "hello"
        ));
    }

    #[test]
    fn test_parse_override() {
        let override_val = InputOverride::parse("workflow.param=123").unwrap();
        assert_eq!(override_val.path, vec!["workflow", "param"]);
        assert!(matches!(override_val.value, OverrideValue::Integer(123)));
    }

    #[test]
    fn test_parse_flat_array() {
        let result = parse_value("1,2,3").unwrap();
        match result {
            OverrideValue::Array(values) => {
                assert_eq!(values.len(), 3);
                assert!(matches!(values[0], OverrideValue::Integer(1)));
                assert!(matches!(values[1], OverrideValue::Integer(2)));
                assert!(matches!(values[2], OverrideValue::Integer(3)));
            }
            _ => panic!("Expected Array"),
        }

        // Test mixed types
        let result = parse_value("1,true,hello").unwrap();
        match result {
            OverrideValue::Array(values) => {
                assert_eq!(values.len(), 3);
                assert!(matches!(values[0], OverrideValue::Integer(1)));
                assert!(matches!(values[1], OverrideValue::Boolean(true)));
                assert!(matches!(values[2], OverrideValue::String(s) if s == "hello"));
            }
            _ => panic!("Expected Array"),
        }

        // Test trailing comma
        assert!(parse_value("1,2,").is_err());
    }

    #[test]
    fn test_parse_nested_array() {
        let result = parse_value("[1,2],[3,4]").unwrap();
        match result {
            OverrideValue::Array(outer) => {
                assert_eq!(outer.len(), 2);
                match &outer[0] {
                    OverrideValue::Array(inner1) => {
                        assert!(matches!(inner1[0], OverrideValue::Integer(1)));
                        assert!(matches!(inner1[1], OverrideValue::Integer(2)));
                    }
                    _ => panic!("Expected inner array"),
                }
                match &outer[1] {
                    OverrideValue::Array(inner2) => {
                        assert!(matches!(inner2[0], OverrideValue::Integer(3)));
                        assert!(matches!(inner2[1], OverrideValue::Integer(4)));
                    }
                    _ => panic!("Expected inner array"),
                }
            }
            _ => panic!("Expected outer array"),
        }
    }

    #[test]
    fn test_array_error_cases() {
        // Unclosed bracket
        assert!(parse_value("[1,2").is_err());
        
        // Unmatched brackets
        assert!(parse_value("[1,2]]").is_err());
        assert!(parse_value("[[1,2]").is_err());
        
        // Empty brackets
        let result = parse_value("[]").unwrap();
        assert!(matches!(result, OverrideValue::Array(v) if v.is_empty()));
        
        // Nested empty brackets
        let result = parse_value("[[], []]").unwrap();
        match result {
            OverrideValue::Array(outer) => {
                assert_eq!(outer.len(), 2);
                assert!(matches!(&outer[0], OverrideValue::Array(v) if v.is_empty()));
                assert!(matches!(&outer[1], OverrideValue::Array(v) if v.is_empty()));
            }
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn test_json_conversion() {
        let override_val = InputOverride::parse("workflow.nested=[1,2],[3,4]").unwrap();
        let json = override_val.value.to_json();
        
        assert_eq!(
            json.to_string(),
            r#"[["1","2"],["3","4"]]"#
        );
    }

    #[test]
    fn test_apply_overrides() {
        let base_json: Value = serde_json::from_str(r#"
            {
                "workflow": {
                    "param1": "old",
                    "nested": {"value": 42}
                }
            }
        "#).unwrap();

        let overrides = vec![
            InputOverride::parse("workflow.param1=new").unwrap(),
            InputOverride::parse("workflow.param2=added").unwrap(),
            InputOverride::parse("workflow.nested.value=43").unwrap()
        ];

        let result = apply_overrides(base_json, &overrides).unwrap();
        
        assert_eq!(
            result["workflow"]["param1"].as_str().unwrap(),
            "new"
        );
        assert_eq!(
            result["workflow"]["param2"].as_str().unwrap(),
            "added"
        );
        assert_eq!(
            result["workflow"]["nested"]["value"].as_i64().unwrap(),
            43
        );
    }
} 