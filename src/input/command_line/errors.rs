//! Error handling for input overrides.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum OverrideError {
    #[error("Invalid override format. Expected 'key=value', got '{0}'")]
    InvalidFormat(String),

    #[error("Empty key in override")]
    EmptyKey,

    #[error("Empty path in override")]
    EmptyPath,

    #[error("Invalid path: '{0}'. Contains empty component")]
    EmptyPathComponent(String),

    #[error("Missing value after '='")]
    MissingValue,

    #[error("Trailing comma not allowed in array")]
    TrailingComma,

    #[error("Leading comma not allowed in array")]
    LeadingComma,

    #[error("Consecutive commas not allowed in array")]
    ConsecutiveCommas,

    #[error("Unclosed bracket in array: '{0}'")]
    UnclosedBracket(String),

    #[error("Unbalanced brackets in array: '{0}'")]
    UnbalancedBrackets(String),

    #[error("Empty element in array: '{0}'")]
    EmptyArrayElement(String),

    #[error("Path conflict: '{0}' conflicts with '{1}'")]
    PathConflict(String, String),

    #[error("Cannot set value at '{0}': parent is not an object")]
    ParentNotObject(String),

    #[error("Failed to navigate to key: {0}")]
    NavigationFailed(String),
} 