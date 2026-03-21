//! Server configuration validation tests.

use anyhow::Result;
use sprocket::ServerConfig;
use tempfile::TempDir;

fn make_config(
    output_directory: std::path::PathBuf,
    allowed_file_paths: Vec<std::path::PathBuf>,
    allowed_urls: Vec<String>,
    max_concurrent_runs: Option<usize>,
) -> ServerConfig {
    ServerConfig {
        output_directory,
        allowed_file_paths,
        allowed_urls,
        max_concurrent_runs,
        ..Default::default()
    }
}

#[test]
fn validate_canonicalizes_allowed_file_paths() -> Result<()> {
    let temp = TempDir::new()?;
    let temp_path = temp.path();

    // Create a subdirectory
    let subdir = temp_path.join("subdir");
    std::fs::create_dir(&subdir)?;

    // Use a relative path with `..` that resolves to the subdirectory
    let relative_path = subdir.join("..").join("subdir");

    let mut config = make_config(
        temp_path.to_path_buf(),
        vec![relative_path.clone()],
        vec![],
        None,
    );

    config.validate()?;

    // After validation, path should be canonical (absolute with no `.` or `..`)
    assert_eq!(config.allowed_file_paths.len(), 1);
    let canonical = &config.allowed_file_paths[0];
    assert!(canonical.is_absolute());
    assert_eq!(canonical, &subdir.canonicalize()?);
    assert!(!canonical.to_string_lossy().contains(".."));

    Ok(())
}

#[test]
fn validate_deduplicates_file_paths() -> Result<()> {
    let temp = TempDir::new()?;
    let temp_path = temp.path();

    let subdir = temp_path.join("subdir");
    std::fs::create_dir(&subdir)?;

    // Add the same path twice
    let mut config = make_config(
        temp_path.to_path_buf(),
        vec![subdir.clone(), subdir.clone()],
        vec![],
        None,
    );

    config.validate()?;

    // Should be deduplicated
    assert_eq!(config.allowed_file_paths.len(), 1);

    Ok(())
}

#[test]
fn validate_sorts_file_paths() -> Result<()> {
    let temp = TempDir::new()?;
    let temp_path = temp.path();

    let dir_a = temp_path.join("a");
    let dir_b = temp_path.join("b");
    let dir_c = temp_path.join("c");

    std::fs::create_dir(&dir_a)?;
    std::fs::create_dir(&dir_b)?;
    std::fs::create_dir(&dir_c)?;

    // Add paths in reverse order
    let mut config = make_config(
        temp_path.to_path_buf(),
        vec![dir_c.clone(), dir_a.clone(), dir_b.clone()],
        vec![],
        None,
    );

    config.validate()?;

    // Should be sorted
    assert_eq!(config.allowed_file_paths.len(), 3);
    assert_eq!(config.allowed_file_paths[0], dir_a.canonicalize()?);
    assert_eq!(config.allowed_file_paths[1], dir_b.canonicalize()?);
    assert_eq!(config.allowed_file_paths[2], dir_c.canonicalize()?);

    Ok(())
}

#[test]
fn validate_deduplicates_urls() {
    let mut config = make_config(
        "./out".into(),
        vec![],
        vec![
            "https://example.com/".to_string(),
            "https://example.com/".to_string(),
        ],
        None,
    );

    config.validate().unwrap();

    // Should be deduplicated
    assert_eq!(config.allowed_urls.len(), 1);
}

#[test]
fn validate_sorts_urls() {
    let mut config = make_config(
        "./out".into(),
        vec![],
        vec![
            "https://zzz.com/".to_string(),
            "https://aaa.com/".to_string(),
            "https://mmm.com/".to_string(),
        ],
        None,
    );

    config.validate().unwrap();

    // Should be sorted
    assert_eq!(config.allowed_urls.len(), 3);
    assert_eq!(config.allowed_urls[0], "https://aaa.com/");
    assert_eq!(config.allowed_urls[1], "https://mmm.com/");
    assert_eq!(config.allowed_urls[2], "https://zzz.com/");
}

#[test]
fn default_output_directory_is_out() {
    let config = ServerConfig::default();
    assert_eq!(config.output_directory.to_str().unwrap(), "./out");
}

#[test]
fn validate_rejects_invalid_url_no_scheme() {
    let mut config = make_config(
        "./out".into(),
        vec![],
        vec!["example.com".to_string()],
        None,
    );

    let result = config.validate();
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("invalid URL"));
}

#[test]
fn validate_rejects_invalid_url_malformed() {
    let mut config = make_config(
        "./out".into(),
        vec![],
        vec!["https://[invalid".to_string()],
        None,
    );

    let result = config.validate();
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("invalid URL"));
}

#[test]
fn validate_rejects_nonexistent_file_path() {
    let mut config = make_config(
        "./out".into(),
        vec!["/this/path/does/not/exist/sprocket-test-nonexistent".into()],
        vec![],
        None,
    );

    let result = config.validate();
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("failed to canonicalize"));
}

#[test]
fn validate_with_empty_allowed_paths() {
    let mut config = make_config("./out".into(), vec![], vec![], None);

    let result = config.validate();
    assert!(result.is_ok());
}

#[test]
fn validate_with_empty_allowed_urls() -> Result<()> {
    let temp = TempDir::new()?;
    let temp_path = temp.path();

    let mut config = make_config(
        temp_path.to_path_buf(),
        vec![temp_path.to_path_buf()],
        vec![],
        None,
    );

    let result = config.validate();
    assert!(result.is_ok());

    Ok(())
}

#[test]
fn validate_preserves_url_case_sensitivity() {
    let mut config = make_config(
        "./out".into(),
        vec![],
        vec![
            "https://Example.com/".to_string(),
            "https://example.com/".to_string(),
        ],
        None,
    );

    config.validate().unwrap();

    // Both should be kept (URLs are case-sensitive in host part for deduplication)
    assert_eq!(config.allowed_urls.len(), 2);
}

#[test]
fn validate_handles_overlapping_file_paths() -> Result<()> {
    let temp = TempDir::new()?;
    let temp_path = temp.path();

    let parent = temp_path.join("parent");
    let child = parent.join("child");

    std::fs::create_dir(&parent)?;
    std::fs::create_dir(&child)?;

    let mut config = make_config(
        temp_path.to_path_buf(),
        vec![parent.clone(), child.clone()],
        vec![],
        None,
    );

    config.validate()?;

    // Both should be kept (overlapping paths are not deduplicated)
    assert_eq!(config.allowed_file_paths.len(), 2);

    Ok(())
}

#[test]
fn validate_deduplicates_equivalent_paths() -> Result<()> {
    let temp = TempDir::new()?;
    let temp_path = temp.path();

    let subdir = temp_path.join("subdir");
    std::fs::create_dir(&subdir)?;

    // Same path via different routes (. and ..)
    let path1 = subdir.clone();
    let path2 = subdir.join(".").join("..");
    let path2 = path2.join("subdir");

    let mut config = make_config(temp_path.to_path_buf(), vec![path1, path2], vec![], None);

    config.validate()?;

    // Should be deduplicated after canonicalization
    assert_eq!(config.allowed_file_paths.len(), 1);

    Ok(())
}

#[test]
fn validate_handles_trailing_slash_in_paths() -> Result<()> {
    let temp = TempDir::new()?;
    let temp_path = temp.path();

    let subdir = temp_path.join("subdir");
    std::fs::create_dir(&subdir)?;

    // PathBuf doesn't actually store trailing slashes, so this test just
    // verifies that paths work correctly
    let mut config = make_config(temp_path.to_path_buf(), vec![subdir.clone()], vec![], None);

    config.validate()?;

    assert_eq!(config.allowed_file_paths.len(), 1);

    Ok(())
}

#[test]
fn validate_with_max_concurrent_workflows_none() {
    let mut config = make_config("./out".into(), vec![], vec![], None);

    let result = config.validate();
    assert!(result.is_ok());
    assert_eq!(config.max_concurrent_runs, None);
}

#[test]
fn validate_rejects_max_concurrent_runs_zero() {
    let mut config = make_config("./out".into(), vec![], vec![], Some(0));

    let result = config.validate();
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert_eq!(err_msg, "`max_concurrent_runs` must be at least 1");
}

#[test]
fn validate_with_max_concurrent_runs_large() {
    let mut config = make_config("./out".into(), vec![], vec![], Some(10000));

    let result = config.validate();
    assert!(result.is_ok());
    assert_eq!(config.max_concurrent_runs, Some(10000));
}
