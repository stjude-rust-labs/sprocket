//! Run name generation tests.

use std::collections::HashSet;

use regex::Regex;
use sprocket::execution::generate_run_name;

#[test]
fn generate_workflow_name_format_and_uniqueness() {
    // Expected format: adjective-noun-xxxxxx (e.g., "happy-elephant-a1b2c3")
    let name = generate_run_name();
    let pattern = Regex::new(r"^[a-z]+-[a-z]+-[0-9a-f]{6}$").unwrap();
    assert!(
        pattern.is_match(&name),
        "name '{}' does not match expected pattern adjective-noun-xxxxxx",
        name
    );

    let parts: Vec<&str> = name.split('-').collect();
    assert_eq!(
        parts.len(),
        3,
        "name should have 3 parts separated by hyphens"
    );

    // First two parts (adjective and noun) should be alphabetic
    assert!(
        parts[0].chars().all(|c| c.is_alphabetic()),
        "adjective should be alphabetic"
    );
    assert!(
        parts[1].chars().all(|c| c.is_alphabetic()),
        "noun should be alphabetic"
    );

    // Third part should be exactly 6 hex digits
    assert_eq!(
        parts[2].len(),
        6,
        "hex suffix should be exactly 6 characters"
    );
    assert!(
        parts[2].chars().all(|c| c.is_ascii_hexdigit()),
        "suffix should be valid hex"
    );

    // Should be all lowercase
    assert!(
        !name.chars().any(|c| c.is_uppercase()),
        "name should be all lowercase"
    );

    // Generate 1000 names and verify all are unique
    let mut names = HashSet::new();
    for _ in 0..1000 {
        let name = generate_run_name();
        names.insert(name);
    }
    assert_eq!(
        names.len(),
        1000,
        "all 1000 generated names should be unique"
    );
}
