//! Command-line input override functionality for WDL workflows.
//!
//! This module provides parsing and application of key=value overrides to JSON inputs.
//! It supports all WDL data types and nested structures using dot notation.

use anyhow::Result;
use serde_json::{Value, json};
use std::collections::HashSet;

mod overrides_errors;
pub use overrides_errors::OverrideError;

/// A parsed command line override with path components and value.
#[derive(Debug, Clone)]
pub struct InputOverride {
    /// Path components (e.g., ["workflow", "task", "param"])
    pub path: Vec<String>,
    /// Value to set at this path
    pub value: OverrideValue,
}

/// Represents different types of values that can be provided via CLI.
#[derive(Debug, Clone)]
pub enum OverrideValue {
    /// String value
    String(String),
    /// Integer value
    Integer(i64),
    /// Floating point value
    Float(f64),
    /// Boolean value
    Boolean(bool),
    /// Array of values
    Array(Vec<OverrideValue>),
    /// Null value
    Null,
}

impl InputOverride {
    /// Parses a key=value string into an InputOverride.
    ///
    /// # Arguments
    ///
    /// * `input` - A string in the format "key=value"
    ///
    /// # Returns
    ///
    /// An InputOverride or an error if parsing fails
    pub fn parse(input: &str) -> Result<Self, OverrideError> {
        let parts: Vec<&str> = input.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err(OverrideError::InvalidFormat(input.to_string()));
        }

        let key = parts[0].trim();
        let value = parts[1].trim();

        if key.is_empty() {
            return Err(OverrideError::EmptyKey);
        }

        let path = parse_path(key)?;
        let value = parse_value(value)?;

        Ok(Self { path, value })
    }
}

/// Parses a dot-separated path into components.
fn parse_path(path: &str) -> Result<Vec<String>, OverrideError> {
    if path.is_empty() {
        return Err(OverrideError::EmptyPath);
    }

    let components: Vec<String> = path.split('.')
                                     .map(|s| s.trim().to_string())
                                     .collect();
    
    // Check for empty components (e.g., "workflow..param")
    if components.iter().any(|s| s.is_empty()) {
        return Err(OverrideError::EmptyPathComponent(path.to_string()));
    }

    // Ensure the path has at least one component
    if components.is_empty() {
        return Err(OverrideError::EmptyPath);
    }

    Ok(components)
}

/// Parses a string into an appropriate OverrideValue based on content.
fn parse_value(input: &str) -> Result<OverrideValue, OverrideError> {
    if input.is_empty() {
        return Err(OverrideError::MissingValue);
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

/// Parses a comma-separated string into an array.
fn parse_flat_array(input: &str) -> Result<OverrideValue, OverrideError> {
    if input.is_empty() {
        return Err(OverrideError::EmptyArray);
    }

    if input.ends_with(',') {
        return Err(OverrideError::TrailingComma);
    }

    if input.starts_with(',') {
        return Err(OverrideError::LeadingComma);
    }

    // Check for consecutive commas
    if input.contains(",,") {
        return Err(OverrideError::ConsecutiveCommas);
    }

    let values = input
        .split(',')
        .map(|s| parse_value(s.trim()))
        .collect::<Result<Vec<_>, _>>()?;
    
    if values.is_empty() {
        return Err(OverrideError::EmptyArray);
    }
    
    Ok(OverrideValue::Array(values))
}

/// Parses a nested array using bracket notation.
fn parse_array_value(input: &str) -> Result<OverrideValue, OverrideError> {
    if !input.ends_with(']') {
        return Err(OverrideError::UnclosedBracket(input.to_string()));
    }

    // Check for balanced brackets
    let open_count = input.chars().filter(|&c| c == '[').count();
    let close_count = input.chars().filter(|&c| c == ']').count();
    
    if open_count != close_count {
        return Err(OverrideError::UnbalancedBrackets(input.to_string()));
    }

    let inner = &input[1..input.len()-1];
    
    // Handle empty array
    if inner.trim().is_empty() {
        return Ok(OverrideValue::Array(vec![]));
    }

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
                    return Err(OverrideError::UnmatchedClosingBracket(input.to_string()));
                }
            }
            ',' if depth == 0 => {
                if !current.is_empty() {
                    values.push(parse_value(current.trim())?);
                    current.clear();
                } else {
                    return Err(OverrideError::EmptyArrayElement(input.to_string()));
                }
            }
            _ => current.push(c),
        }
    }

    if depth != 0 {
        return Err(OverrideError::UnmatchedOpeningBracket(input.to_string()));
    }

    if !current.is_empty() {
        values.push(parse_value(current.trim())?);
    }

    Ok(OverrideValue::Array(values))
}

impl OverrideValue {
    /// Converts the override value to a JSON value.
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

    /// Get a human-readable type name for error messages
    pub fn type_name(&self) -> &'static str {
        match self {
            OverrideValue::String(_) => "string",
            OverrideValue::Integer(_) => "integer",
            OverrideValue::Float(_) => "float",
            OverrideValue::Boolean(_) => "boolean",
            OverrideValue::Array(_) => "array",
            OverrideValue::Null => "null",
        }
    }
}

/// Applies a list of overrides to a JSON value.
///
/// # Arguments
///
/// * `base` - The base JSON value to modify
/// * `overrides` - A slice of InputOverride objects to apply
///
/// # Returns
///
/// The modified JSON value or an error
pub fn apply_overrides(mut base: Value, overrides: &[InputOverride]) -> Result<Value, OverrideError> {
    // Check for path conflicts
    check_path_conflicts(overrides)?;
    
    for override_value in overrides {
        apply_single_override(&mut base, override_value)?;
    }
    Ok(base)
}

/// Checks for conflicting paths in the overrides.
///
/// For example, setting both "workflow" and "workflow.param" is a conflict.
fn check_path_conflicts(overrides: &[InputOverride]) -> Result<(), OverrideError> {
    let mut paths: HashSet<String> = HashSet::new();
    let mut prefixes: HashSet<String> = HashSet::new();
    
    for input_override in overrides {
        let path_str = input_override.path.join(".");
        
        // Check if this path is a prefix of any existing path
        for existing in &paths {
            if existing.starts_with(&path_str) && existing.len() > path_str.len() && existing.chars().nth(path_str.len()) == Some('.') {
                return Err(OverrideError::PathConflict(path_str.clone(), existing.clone()));
            }
        }
        
        // Check if any existing prefix is a prefix of this path
        for prefix in &prefixes {
            if path_str.starts_with(prefix) && path_str.len() > prefix.len() && path_str.chars().nth(prefix.len()) == Some('.') {
                return Err(OverrideError::PathConflict(path_str.clone(), prefix.clone()));
            }
        }
        
        paths.insert(path_str.clone());
        prefixes.insert(path_str);
    }
    
    Ok(())
}

/// Applies a single override to a JSON value.
fn apply_single_override(json: &mut Value, input_override: &InputOverride) -> Result<(), OverrideError> {
    let mut current = json;
    
    // Navigate to the correct location
    for (i, key) in input_override.path.iter().enumerate() {
        if i == input_override.path.len() - 1 {
            // Set the final value
            if let Some(obj) = current.as_object_mut() {
                obj.insert(key.clone(), input_override.value.to_json());
            } else {
                return Err(OverrideError::ParentNotObject(key.clone()));
            }
            break;
        }

        // Create/navigate intermediate objects
        if let Some(obj) = current.as_object_mut() {
            if !obj.contains_key(key) {
                obj.insert(key.clone(), json!({}));
            }
            current = obj.get_mut(key)
                .ok_or_else(|| OverrideError::NavigationFailed(key.clone()))?;
        } else {
            return Err(OverrideError::ParentNotObject(key.clone()));
        }
    }

    Ok(())
}

