//! Tests for workflow name generation.

use sprocket_server::names::generate_workflow_name;

#[test]
fn test_generate_workflow_name_format() {
    let name = generate_workflow_name();

    // Should be in format: word-word-xxxxxx (e.g., "happy-elephant-a1b2c3").
    let parts: Vec<&str> = name.split('-').collect();
    assert_eq!(parts.len(), 3, "name should have 3 parts: adjective-noun-hex");

    // First two parts should be lowercase letters.
    for part in &parts[0..2] {
        assert!(!part.is_empty(), "name parts should not be empty");
        assert_eq!(part, &part.to_lowercase(), "name should be lowercase");
        assert!(
            part.chars().all(|c| c.is_ascii_alphabetic()),
            "word parts should only contain letters"
        );
    }

    // Last part should be 6 hex digits.
    let hex_part = parts[2];
    assert_eq!(hex_part.len(), 6, "hex part should be 6 characters");
    assert!(
        hex_part.chars().all(|c| c.is_ascii_hexdigit()),
        "hex part should only contain hex digits"
    );
}

#[test]
fn test_generate_workflow_name_uniqueness() {
    // Generate 100 names and ensure they're all unique.
    let names: Vec<String> = (0..100).map(|_| generate_workflow_name()).collect();
    let unique: std::collections::HashSet<_> = names.iter().collect();

    assert_eq!(names.len(), unique.len(), "all generated names should be unique");
}
